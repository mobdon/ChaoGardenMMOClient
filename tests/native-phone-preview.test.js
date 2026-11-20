import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { nativePhonePreview } from '../scripts/native-phone-preview.mjs';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');

async function fixture(t) {
  // Every manifest and mutation in this suite belongs to a disposable root,
  // never to the running preview or its canonical save/credentials.
  const root = await mkdtemp(join(tmpdir(), 'chao-native-phone-preview-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const directory = join(root, '.local', 'native-phone');
  await mkdir(join(directory, 'snapshot-test', 'assets'), { recursive: true });
  const body = Buffer.from('<h1>Tiny Garden · 宝</h1>');
  const file = join(directory, 'snapshot-test', 'tiny.html');
  await writeFile(file, body);
  const manifest = { sourceCommit: 'isolated-fixture', routes: {
    '/phone-tiny/tiny.html': { file: 'snapshot-test/tiny.html', sha256: digest(body), mime: 'text/html; charset=utf-8' },
  } };
  const manifestFile = join(directory, 'manifest.json');
  const publish = () => writeFile(manifestFile, JSON.stringify(manifest));
  await publish();
  let handler;
  nativePhonePreview(root).configureServer({ middlewares: { use(value) { handler = value; } } });
  assert.equal(typeof handler, 'function');
  async function request(url, method = 'GET') {
    const result = { status: null, headers: {}, body: undefined, ended: false, fellThrough: false };
    const response = {
      writeHead(status, headers = {}) { result.status = status; result.headers = headers; return this; },
      end(value) { result.body = value; result.ended = true; return this; },
    };
    await handler({ url, method }, response, () => { result.fellThrough = true; });
    return result;
  }
  return { root, directory, body, file, manifest, manifestFile, publish, request };
}

test('native phone serves only the exact GET entry, with byte length and no-store headers', async t => {
  const f = await fixture(t);
  for (const path of ['/phone-tiny/tiny.html', '/phone-tiny/tiny.html?view=phone']) {
    const result = await f.request(path);
    assert.equal(result.status, 200);
    assert.equal(result.ended, true);
    assert.equal(result.fellThrough, false);
    assert.deepEqual(result.body, f.body);
    assert.deepEqual(result.headers, {
      'Content-Type': 'text/html; charset=utf-8', 'Content-Length': f.body.length,
      'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff',
    });
  }
});

test('native phone HEAD returns GET metadata without presentation bytes', async t => {
  const f = await fixture(t);
  const get = await f.request('/phone-tiny/tiny.html');
  const head = await f.request('/phone-tiny/tiny.html', 'HEAD');
  assert.equal(head.status, 200);
  assert.deepEqual(head.headers, get.headers);
  assert.equal(head.body, undefined);
  assert.equal(head.ended, true);
  assert.equal(head.fellThrough, false);
});

test('native phone leaves Rust APIs, sockets, private paths and other pages to their existing handlers', async t => {
  const f = await fixture(t);
  for (const path of ['/api/session', '/api/assets/tiny-dark-chaos.png', '/ws?token=not-a-real-credential',
    '/tiny.html', '/widget.html', '/.local/native-phone/manifest.json', '/phone-tiny-other/tiny.html']) {
    const result = await f.request(path);
    assert.equal(result.fellThrough, true, path);
    assert.equal(result.ended, false, path);
    assert.equal(result.status, null, path);
    assert.equal(result.body, undefined, path);
  }
});

test('native phone rejects unknown, alias and traversal requests without an SPA fallback', async t => {
  const f = await fixture(t);
  for (const path of ['/phone-tiny/', '/phone-tiny/widget.html', '/phone-tiny/manifest.json',
    '/phone-tiny/toString', '/phone-tiny/%74iny.html', '/phone-tiny/./tiny.html',
    '/phone-tiny/../tiny.html', '/phone-tiny/%2e%2e/tiny.html', '/phone-tiny/%252e%252e/tiny.html',
    '/phone-tiny/..\\tiny.html', '/phone-tiny/assets/../../tiny.html']) {
    const result = await f.request(path);
    assert.equal(result.status, 404, path);
    assert.equal(result.fellThrough, false, path);
    assert.equal(result.ended, true, path);
    assert.notDeepEqual(result.body, f.body, path);
  }
});

test('native phone rejects mutation methods and does not change the isolated snapshot', async t => {
  const f = await fixture(t);
  const beforeManifest = await readFile(f.manifestFile);
  for (const method of ['POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'CONNECT']) {
    for (const path of ['/phone-tiny/tiny.html', '/phone-tiny/unknown']) {
      const result = await f.request(path, method);
      assert.equal(result.status, 405, `${method} ${path}`);
      assert.equal(result.fellThrough, false);
      assert.equal(result.body, undefined);
    }
  }
  assert.deepEqual(await readFile(f.file), f.body);
  assert.deepEqual(await readFile(f.manifestFile), beforeManifest);
});

test('native phone rejects manifest entries that point outside its snapshot boundary', async t => {
  const f = await fixture(t);
  const secret = Buffer.from('isolated outside file must never be served');
  const outside = join(f.root, 'private.txt');
  await writeFile(outside, secret);
  for (const file of [relative(f.directory, outside), outside]) {
    f.manifest.routes['/phone-tiny/escape.txt'] = { file, sha256: digest(secret), mime: 'text/plain' };
    await f.publish();
    const result = await f.request('/phone-tiny/escape.txt');
    assert.equal(result.status, 404);
    assert.equal(result.fellThrough, false);
    assert.equal(Buffer.from(result.body).includes(secret), false);
  }
});

test('native phone fails closed if an allowlisted file changes or disappears, including HEAD', async t => {
  const f = await fixture(t);
  await writeFile(f.file, '<h1>Unverified replacement</h1>');
  for (const method of ['GET', 'HEAD']) {
    const result = await f.request('/phone-tiny/tiny.html', method);
    assert.equal(result.status, 404);
    assert.equal(result.fellThrough, false);
    assert.equal(Buffer.from(result.body).includes(Buffer.from('Unverified replacement')), false);
  }
  await rm(f.file);
  assert.equal((await f.request('/phone-tiny/tiny.html')).status, 404);
});

test('native phone fails closed for missing, corrupt or incomplete manifests and wrong digests', async t => {
  const f = await fixture(t);
  const malformed = ['{', '{}', '{"routes":null}', '{"routes":{"/phone-tiny/tiny.html":{}}}'];
  for (const contents of malformed) {
    await writeFile(f.manifestFile, contents);
    const result = await f.request('/phone-tiny/tiny.html');
    assert.equal(result.status, 404, contents);
    assert.equal(result.fellThrough, false);
    assert.notDeepEqual(result.body, f.body);
  }
  f.manifest.routes['/phone-tiny/tiny.html'].sha256 = '0'.repeat(64);
  await f.publish();
  assert.equal((await f.request('/phone-tiny/tiny.html')).status, 404);
  await rm(f.manifestFile);
  assert.equal((await f.request('/phone-tiny/tiny.html')).status, 404);
});
