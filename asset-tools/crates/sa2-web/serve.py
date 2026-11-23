"""Local server for the recompiled SA2 in a browser tab. Binds to 127.0.0.1.

Serves the page (www/), the two wasm builds, and, read-only, the player's own
folders the game reads, each under /fs/<name>/ (the game sees /<name>/):

    python -I serve.py --game game.wasm --mount steam="C:/.../Sonic Adventure 2"
        --mount disc=... --mount rels=... --file research/main.dol=...
        [--env KEY=VALUE ...] [--port 8420] [--open]

/fs/<path>?stat answers {"dir": bool, "size": n}; /fs/<dir>?ls lists a folder
as [[name, is_dir, size], ...]; a plain GET reads a file (Range supported).
The page needs cross-origin isolation for SharedArrayBuffer, so every reply
carries COOP/COEP headers.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import http.server
import webbrowser

HERE = os.path.dirname(os.path.abspath(__file__))
WWW = os.path.join(HERE, 'www')
TYPES = {'.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm',
         '.json': 'application/json', '.css': 'text/css'}


def pair(s):
    k, _, v = s.partition('=')
    if not k or not v:
        raise argparse.ArgumentTypeError(f'expected NAME=VALUE, got {s!r}')
    return k.strip('/'), v


ap = argparse.ArgumentParser()
ap.add_argument('--game', required=True, help='the game build (sa2-host.wasm)')
ap.add_argument('--pkg', default=os.path.join(WWW, 'pkg'), help="the page build (wasm-bindgen's output)")
ap.add_argument('--mount', type=pair, action='append', default=[], help='name=folder')
ap.add_argument('--file', type=pair, action='append', default=[], help='path=file')
ap.add_argument('--env', type=pair, action='append', default=[], help='KEY=VALUE for the game')
ap.add_argument('--port', type=int, default=8420)
ap.add_argument('--open', action='store_true', help='open the page in the default browser')
ap.add_argument('--export', metavar='DIR', help="write a static site to DIR instead of serving: the page, the game build and the --file/--mount files (never the player's steam or disc mounts)")
args = ap.parse_args()
MOUNTS = {k: os.path.realpath(v) for k, v in args.mount}
FILES = {k: os.path.realpath(v) for k, v in args.file}
ENV = dict(args.env)


def local(rel):
    """The file or folder `rel` (under /fs/) names, or None. Never outside a mount."""
    rel = rel.strip('/')
    if rel in FILES:
        return FILES[rel]
    name, _, rest = rel.partition('/')
    root = MOUNTS.get(name)
    if root is None:
        # A parent of --file paths ("research") is a folder of them.
        return None
    p = os.path.realpath(os.path.join(root, *[s for s in rest.split('/') if s]))
    return p if p == root or p.startswith(root + os.sep) else None


def virtual_dir(rel):
    """Names under `rel` that only --file and --mount paths make up."""
    rel = rel.strip('/')
    pre = rel + '/' if rel else ''
    out = {}
    for k in list(FILES) + list(MOUNTS):
        if k.startswith(pre):
            head, _, tail = k[len(pre):].partition('/')
            is_dir = bool(tail) or k in MOUNTS
            out[head] = (is_dir, 0 if is_dir else os.path.getsize(FILES[k]))
    return out


def export(out):
    """The hosted build: static files plus fs-index.json, which answers the
    worker's stat and ls where no serve.py runs. The player's own Steam folder
    and disc are never copied; the page reads those from what they pick."""
    if os.path.exists(out):
        sys.exit(f'{out} exists; give a new folder')
    entries = {}  # path -> [is_dir, size]

    def add(rel, src):
        dst = os.path.join(out, 'fs', *rel.split('/'))
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(src, dst)
        entries[rel] = [False, os.path.getsize(src)]
        parts = rel.split('/')
        for i in range(1, len(parts)):
            entries.setdefault('/'.join(parts[:i]), [True, 0])

    for rel, src in FILES.items():
        add(rel, src)
    for name, root in MOUNTS.items():
        if name in ('steam', 'disc'):
            continue
        for d, _, names in os.walk(root):
            for n in names:
                add(name + '/' + os.path.relpath(os.path.join(d, n), root).replace(os.sep, '/'), os.path.join(d, n))
    shutil.copytree(WWW, out, ignore=shutil.ignore_patterns('pkg'), dirs_exist_ok=True)
    shutil.copytree(args.pkg, os.path.join(out, 'pkg'))
    shutil.copyfile(args.game, os.path.join(out, 'game.wasm'))
    # The game build compressed (628 MB -> about 23 MB with brotli 9): the page
    # fetches game.wasm.br first and the host sends it as brotli-encoded wasm.
    subprocess.run(['brotli', '-q', '9', '-f', '-o', os.path.join(out, 'game.wasm.br'), os.path.join(out, 'game.wasm')], check=True)
    with open(os.path.join(out, 'env.json'), 'w') as f:
        json.dump({k: v for k, v in ENV.items() if k not in ('SA2_STEAM_DIR', 'SA2_DISC_DIR', 'SA2_DISC_FIRST')}, f)
    with open(os.path.join(out, 'fs-index.json'), 'w') as f:
        json.dump(entries, f)
    # Shared memory between the game's threads needs cross-origin isolation.
    with open(os.path.join(out, '_headers'), 'w') as f:
        f.write('/*\n  Cross-Origin-Opener-Policy: same-origin\n  Cross-Origin-Embedder-Policy: require-corp\n')
        f.write('/game.wasm.br\n  Content-Type: application/wasm\n  Content-Encoding: br\n  Cache-Control: no-cache\n')
    print(f'{out}: {sum(not d for d, _ in entries.values())} files under fs/')


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *a):
        pass

    def head(self, code, ctype, length, extra=()):
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(length))
        # Builds may be kept and checked again (Chrome then reuses the code it
        # compiled from them); everything else, the game's files included, not.
        cached = any(k == 'ETag' for k, _ in extra)
        self.send_header('Cache-Control', 'no-cache' if cached else 'no-store')
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cross-Origin-Resource-Policy', 'same-origin')
        for k, v in extra:
            self.send_header(k, v)
        self.end_headers()

    def body(self, code, ctype, data):
        self.head(code, ctype, len(data))
        self.wfile.write(data)

    def json(self, obj, code=200):
        self.body(code, 'application/json', json.dumps(obj).encode())

    def send_file(self, path, cache=False):
        size = os.path.getsize(path)
        start, end = 0, size - 1
        rng = self.headers.get('Range', '')
        code, extra = 200, [('Accept-Ranges', 'bytes')]
        if cache:
            tag = f'"{size}-{os.stat(path).st_mtime_ns}"'
            extra.append(('ETag', tag))
            if self.headers.get('If-None-Match') == tag:
                self.head(304, 'application/wasm', 0, extra)
                return
        if rng.startswith('bytes='):
            a, _, b = rng[6:].partition('-')
            start = int(a) if a else 0
            end = min(int(b), size - 1) if b else size - 1
            if start >= size:
                self.head(416, 'application/octet-stream', 0, [('Content-Range', f'bytes */{size}')])
                return
            code = 206
            extra.append(('Content-Range', f'bytes {start}-{end}/{size}'))
        ctype = TYPES.get(os.path.splitext(path)[1].lower(), 'application/octet-stream')
        n = end - start + 1
        self.head(code, ctype, n, extra)
        with open(path, 'rb') as f:
            f.seek(start)
            while n > 0:
                chunk = f.read(min(n, 1 << 20))
                if not chunk:
                    break
                self.wfile.write(chunk)
                n -= len(chunk)

    def do_GET(self):
        path, _, query = self.path.partition('?')
        path = path.split('#', 1)[0]
        if path.startswith('/fs/') or path == '/fs':
            return self.fs(path[4:], query)
        if path == '/env.json':
            return self.json(ENV)
        if path == '/game.wasm':
            return self.send_file(os.path.realpath(args.game), cache=True)
        if path.startswith('/pkg/'):
            p = os.path.realpath(os.path.join(args.pkg, path[5:]))
            root = os.path.realpath(args.pkg)
        else:
            p = os.path.realpath(os.path.join(WWW, (path.strip('/') or 'index.html')))
            root = WWW
        if not p.startswith(root + os.sep) or not os.path.isfile(p):
            return self.body(404, 'text/plain', b'not found')
        self.send_file(p, cache=p.endswith('.wasm'))

    def fs(self, rel, query):
        rel = rel.strip('/')
        p = local(rel)
        virt = virtual_dir(rel) if p is None else {}
        if query == 'stat':
            if p is not None and os.path.exists(p):
                return self.json({'dir': os.path.isdir(p), 'size': os.path.getsize(p)})
            if virt or rel == '':
                return self.json({'dir': True, 'size': 0})
            return self.json({}, 404)
        if query == 'ls':
            if p is not None and os.path.isdir(p):
                out = []
                with os.scandir(p) as it:
                    for e in it:
                        try:
                            out.append([e.name, e.is_dir(), 0 if e.is_dir() else e.stat().st_size])
                        except OSError:
                            pass
                return self.json(out)
            if virt or rel == '':
                return self.json([[k, d, s] for k, (d, s) in virtual_dir(rel).items()])
            return self.json([], 404)
        if p is not None and os.path.isfile(p):
            return self.send_file(p)
        self.body(404, 'text/plain', b'not found')


if args.export:
    export(args.export)
    sys.exit()
srv = http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
srv.daemon_threads = True
url = f'http://127.0.0.1:{args.port}/'
print(url, flush=True)
if args.open:
    webbrowser.open(url)
srv.serve_forever()
