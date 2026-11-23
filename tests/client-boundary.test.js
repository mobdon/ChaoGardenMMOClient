import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';

function request(port, path) {
  return new Promise((resolve, reject) => {
    const pending = http.get({ hostname: '127.0.0.1', port, path }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode, body: Buffer.concat(chunks).toString('utf8') }));
    });
    pending.on('error', reject);
  });
}

test('standalone client serves public modules while private directories stay inaccessible', { timeout: 15000 }, async () => {
  const root = fileURLToPath(new URL('../', import.meta.url));
  const frontend = await createServer({ root, logLevel: 'silent', server: { port: 0, strictPort: false, open: false } });
  try {
    await frontend.listen();
    const port = frontend.httpServer.address().port;
    const main = await request(port, '/src/main.js');
    assert.equal(main.status, 200);
    assert.match(main.body, /save-slots\.js/);
    assert.doesNotMatch(main.body, /from\s+['"][^'"]*server\//);
    assert.equal((await request(port, '/shared/constants.js')).status, 200);
    assert.equal((await request(port, '/')).status, 200);
    for (const path of [
      '/.local/profiles.json', '/%2elocal/profiles.json', '/%252elocal/profiles.json',
      '/.codex-attachments/private.zip', '/%2ecodex-attachments/private.zip', '/%252ecodex-attachments/private.zip',
      '/server/src/main.rs', '/server/game.js', '/scripts/game-files.js', '/tests/client-boundary.test.js',
      `/@fs/${root.replaceAll('\\', '/')}.local/profiles.json`,
    ]) {
      const result = await request(port, path);
      assert.equal(result.status, 403, path);
      assert.doesNotMatch(result.body, /tokenHash|BEGIN.*PRIVATE KEY/);
    }
  } finally { await frontend.close(); }
});
