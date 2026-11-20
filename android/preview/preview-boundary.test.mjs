import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdir, mkdtemp, writeFile, unlink, rmdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { createServer } from 'vite';

const root = fileURLToPath(new URL('../../', import.meta.url));
function request(port, path) {
  return new Promise((resolve, reject) => {
    http.get({ hostname: '127.0.0.1', port, path }, response => {
      const chunks = [];
      response.on('data', bytes => chunks.push(bytes));
      response.on('end', () => resolve({ status: response.statusCode, body: Buffer.concat(chunks), type: response.headers['content-type'] }));
    }).on('error', reject);
  });
}

test('original-art preview refuses unverified files and retains the private source boundary', async () => {
  await mkdir(resolve(root, '.local'), { recursive: true });
  const privateArt = await mkdtemp(resolve(root, '.local/widget-art-test-'));
  const old = process.env.TINY_GARDEN_ART_DIR;
  process.env.TINY_GARDEN_ART_DIR = privateArt;
  await writeFile(resolve(privateArt, 'tiny-garden-a2ne.png'), 'not verified native art');
  await writeFile(resolve(privateArt, 'tiny-dark-chaos.png'), 'not verified Chao art');
  await writeFile(resolve(privateArt, 'tiny-garden-sidebar-1-a2ne.png'), 'not verified menu art');
  const server = await createServer({ root, configFile: resolve(root, 'android/preview/vite.config.mjs'), logLevel: 'silent', server: { port: 0, strictPort: false } });
  try {
    await server.listen();
    const port = server.httpServer.address().port;
    assert.equal((await request(port, '/widget.html')).status, 200);
    assert.equal((await request(port, '/src/widget.js')).status, 200);
    const art = await request(port, '/__widget-art/tiny-garden-a2ne.png');
    assert.equal(art.status, 404);
    assert.doesNotMatch(art.body.toString(), /not verified native art/);
    const chao = await request(port, '/api/assets/tiny-dark-chaos.png');
    assert.equal(chao.status, 404);
    assert.doesNotMatch(chao.body.toString(), /not verified Chao art/);
    const menu = await request(port, '/__widget-art/tiny-garden-a2ne-sidebar-1.png');
    assert.equal(menu.status, 404);
    assert.doesNotMatch(menu.body.toString(), /not verified menu art/);
    for (const path of ['/.local/profiles.json', '/server/src/service.rs', '/__widget-art/%252e%252e/.local/profiles.json']) {
      assert.equal((await request(port, path)).status, 403);
    }
  } finally {
    await server.close();
    if (old === undefined) delete process.env.TINY_GARDEN_ART_DIR; else process.env.TINY_GARDEN_ART_DIR = old;
    await unlink(resolve(privateArt, 'tiny-garden-a2ne.png'));
    await unlink(resolve(privateArt, 'tiny-dark-chaos.png'));
    await unlink(resolve(privateArt, 'tiny-garden-sidebar-1-a2ne.png'));
    await rmdir(privateArt);
  }
});
