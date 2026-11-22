import test from 'node:test';
import assert from 'node:assert/strict';
import { tinyLogin } from '../src/tiny-login.js';

// Synthetic account: two Chao, only the second has a model stored by the PC.
const chao = [{ token: 'a'.repeat(64), player: { id: 'synthetic-one' } }, { token: 'b'.repeat(64), player: { id: 'synthetic-two' } }];
function server(status, body, stored = ['b'.repeat(64)]) {
  const calls = [];
  return { calls, fetch: async (path, options) => {
    calls.push({ path, options });
    if (path === '/api/tiny-chao') return { ok: stored.includes(options.headers.Authorization.slice(7)) };
    return { ok: status === 200, status, json: async () => body };
  } };
}

test('a password login returns the Chao whose model the PC stored', async () => {
  const s = server(200, { card: 'c'.repeat(64), chao });
  assert.deepEqual(await tinyLogin(s.fetch, { login: 'tester', password: 'synthetic-pass' }), chao[1]);
  assert.equal(s.calls[0].path, '/api/account/login');
  assert.deepEqual(JSON.parse(s.calls[0].options.body), { login: 'tester', password: 'synthetic-pass' });
});
test('a memory card code is tidied and sent as a bearer', async () => {
  const s = server(200, { chao });
  await tinyLogin(s.fetch, { code: ` ${'AB12 '.repeat(16)}` });
  assert.equal(s.calls[0].path, '/api/card');
  assert.equal(s.calls[0].options.headers.Authorization, `Bearer ${'ab12'.repeat(16)}`);
});
test('an account that has not played on its PC is told to', async () => {
  await assert.rejects(tinyLogin(server(200, { chao }, []).fetch, { login: 'tester', password: 'x' }), /Play once on your PC first/);
});
test('a short card code never reaches the server, and a refusal carries the server message', async () => {
  const s = server(401, { error: 'Wrong username or password.' });
  await assert.rejects(tinyLogin(s.fetch, { code: 'abc' }), /isn.t complete/);
  assert.equal(s.calls.length, 0);
  await assert.rejects(tinyLogin(s.fetch, { login: 'tester', password: 'x' }), /Wrong username or password/);
});
