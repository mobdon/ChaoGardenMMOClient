import test from 'node:test';
import assert from 'node:assert/strict';
import { tinyLogin } from '../src/tiny-login.js';

const chao = [{ token: 'b'.repeat(64), player: { id: 'synthetic-chao', name: 'Test' } }];
const server = (status, body) => { const calls = []; return { calls, fetch: async (path, options) => { calls.push({ path, options }); return { ok: status === 200, status, json: async () => body }; } }; };

test('a password login posts to the account route and returns the Chao', async () => {
  const s = server(200, { card: 'c'.repeat(64), chao });
  assert.deepEqual(await tinyLogin(s.fetch, { login: 'tester', password: 'synthetic-pass' }), chao);
  assert.equal(s.calls[0].path, '/api/account/login');
  assert.deepEqual(JSON.parse(s.calls[0].options.body), { login: 'tester', password: 'synthetic-pass' });
});
test('a memory card code is tidied and sent as a bearer', async () => {
  const s = server(200, { chao });
  await tinyLogin(s.fetch, { code: ` ${'AB12 '.repeat(16)}` });
  assert.equal(s.calls[0].path, '/api/card');
  assert.equal(s.calls[0].options.headers.Authorization, `Bearer ${'ab12'.repeat(16)}`);
});
test('a short card code never reaches the server, and a refusal carries the server message', async () => {
  const s = server(401, { error: 'Wrong username or password.' });
  await assert.rejects(tinyLogin(s.fetch, { code: 'abc' }), /isn.t complete/);
  assert.equal(s.calls.length, 0);
  await assert.rejects(tinyLogin(s.fetch, { login: 'tester', password: 'x' }), /Wrong username or password/);
});
