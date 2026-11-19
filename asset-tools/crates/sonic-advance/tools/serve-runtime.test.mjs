import assert from 'node:assert/strict';
import { request } from 'node:http';
import test from 'node:test';
import { createPreviewServer } from './serve-runtime.mjs';

function get(port, path, options = {}) {
  return new Promise((resolve, reject) => {
    const req = request({ hostname: '127.0.0.1', port, path, method: options.method || 'GET', headers: options.headers || {} }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.once('end', () => resolve({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks) }));
    });
    req.once('error', reject);
    req.end(options.body);
  });
}

test('loopback preview serves exactly four routes and never accepts source uploads', async t => {
  const server = createPreviewServer();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  assert.equal(server.address().address, '127.0.0.1');
  const port = server.address().port;
  for (const [path, type] of [['/', 'text/html'], ['/runtime.js', 'text/javascript'], ['/runtime.css', 'text/css']]) {
    const result = await get(port, path);
    assert.equal(result.status, 200);
    assert.ok(result.headers['content-type'].startsWith(type));
    assert.equal(result.headers['cache-control'], 'no-store');
    assert.equal(result.headers['access-control-allow-origin'], undefined);
    assert.equal(result.headers['cross-origin-resource-policy'], 'same-origin');
    assert.ok(result.body.length > 0);
    const head = await get(port, path, { method: 'HEAD' });
    assert.equal(head.status, 200);
    assert.equal(head.body.length, 0);
    assert.equal(Number(head.headers['content-length']), result.body.length);
  }
  const wasm = await get(port, '/runtime.wasm');
  assert.ok([200, 503].includes(wasm.status));
  if (wasm.status === 200) {
    assert.equal(wasm.headers['content-type'], 'application/wasm');
    assert.deepEqual(wasm.body.subarray(0, 4), Buffer.from([0, 97, 115, 109]));
  }
  for (const path of ['/favicon.ico', '/runtime.js?query=1', '/runtime.js.map', '/.local/gba/source.gba', '/.local/profiles.json', '/server/', '/catalog.json', '/preview/', '/%2e%2e/.local/', '/runtime.wasm/extra']) {
    assert.equal((await get(port, path)).status, 404, path);
  }
  for (const method of ['POST', 'PUT', 'DELETE', 'OPTIONS']) {
    const result = await get(port, '/', { method, body: 'never stored' });
    assert.equal(result.status, 405, method);
  }
  assert.equal((await get(port, '/', { headers: { Origin: 'https://example.test' } })).status, 403);
  assert.equal((await get(port, '/', { headers: { Host: 'example.test' } })).status, 403);
  assert.equal((await get(port, '/', { headers: { 'Sec-Fetch-Site': 'cross-site' } })).status, 403);
  const navigation = { 'Sec-Fetch-Site': 'cross-site', 'Sec-Fetch-Mode': 'navigate', 'Sec-Fetch-Dest': 'document' };
  assert.equal((await get(port, '/', { headers: navigation })).status, 200);
  assert.equal((await get(port, '/runtime.js', { headers: navigation })).status, 403);
  assert.equal((await get(port, '/', { headers: { ...navigation, Origin: 'https://example.test' } })).status, 403);
  assert.equal((await get(port, '/', { method: 'GET', headers: { 'Content-Length': '1' }, body: 'x' })).status, 400);
});
