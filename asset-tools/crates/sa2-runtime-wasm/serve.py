"""Local development server for the Iron Gate page.

Serves www/ at / and, read-only, the Steam game folder given as the first
argument at /game/ (for index.html?from=/game/). Binds to 127.0.0.1 only.

    python -I serve.py "C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2" [port]
"""
import functools
import http.server
import os
import sys

WWW = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'www')
GAME = os.path.abspath(sys.argv[1])
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8411
ALLOWED = {'sonic2app.exe', 'resource/gd_PC/stg11D.rel', 'resource/gd_PC/set0011_s.bin',
           'resource/gd_PC/set0011_u.bin'}


class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        path = path.split('?', 1)[0].split('#', 1)[0]
        if path.startswith('/game/'):
            rel = path[len('/game/'):]
            # Only the files the page asks for; nothing else of the install.
            return os.path.join(GAME, *rel.split('/')) if rel in ALLOWED else os.devnull + '.missing'
        return super().translate_path(path)

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()


http.server.SimpleHTTPRequestHandler.extensions_map['.wasm'] = 'application/wasm'
http.server.SimpleHTTPRequestHandler.extensions_map['.js'] = 'text/javascript'
srv = http.server.ThreadingHTTPServer(('127.0.0.1', PORT), functools.partial(Handler, directory=WWW))
print(f'http://127.0.0.1:{PORT}/', flush=True)
srv.serve_forever()
