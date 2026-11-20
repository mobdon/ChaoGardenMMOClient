import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';
import { supportsTinyAccounts } from '../src/tiny-account.js';

const source = await readFile(new URL('../src/tiny.js', import.meta.url), 'utf8');
// Exercise the actual resume path at its HTTP/socket boundary, without a
// browser, live credential, service or invented account authority.
const resumeSource = source.slice(source.indexOf('function selectAccountProtocol(status)'),
  source.indexOf("document.querySelector('.tiny-screen-nav').addEventListener"));
const v2 = { ok: true, accounts: { version: 2, activityLeases: true, intentSequence: true,
  authentication: ['websocket-first-frame'] } };
const credential = 'a'.repeat(64);

function harness(statuses) {
  const requests = [], sockets = [], accountReads = [], notices = [], nodes = new Map();
  let retries = 0;
  const context = vm.createContext({ supportsTinyAccounts, AbortController,
    connectionGeneration: 0, reconnectTimer: null, resumeAbort: null, saveTimer: null,
    pendingSave: false, returnAfterSave: false, socket: null, connected: false, connecting: false,
    tinyGame: null, controlsSession: '', pendingStart: false, pendingStop: false,
    token: null, player: null, careState: null, lastSyncedAt: null, lastCachedAt: 0, lastSavedAt: null, lastResultId: null,
    savedSlots: [], storage: {}, secureTransport: true, loopback: true,
    accountV2: false, knownAccountV2: false, capabilities: {},
    setTimeout: () => 1, clearTimeout() {}, resetMovement() {}, renderPlayer() {}, renderGame() {},
    connectionLabel() {}, rememberPlayer() {}, renderSlots() {}, readTinyCache: () => null,
    notice: text => notices.push(text), scheduleReconnect() { retries++; },
    $(selector) { if (!nodes.has(selector)) nodes.set(selector, {}); return nodes.get(selector); },
    account: { disconnect() {}, async connect(input) { accountReads.push(input); return { snapshot: { self: { id: 'canonical-chao' } } }; } },
    openSocket(token, generation) { sockets.push({ token, generation, v2: context.accountV2 }); },
    async fetch(path, options) {
      requests.push({ path, options });
      if (path === '/api/status') return { ok: true, json: async () => statuses.shift() };
      if (path === '/api/session') return { ok: true, json: async () => ({ token: credential, player: { id: 'canonical-chao' } }) };
      throw new Error(`Unexpected request ${path}`);
    } });
  vm.runInContext(resumeSource, context);
  return { context, requests, sockets, accountReads, notices, retries: () => retries };
}

test('known v2 reconnect cannot send its credential to legacy session or socket paths', async () => {
  const h = harness([v2, { ok: true }]);
  await h.context.resume(credential);
  assert.equal(h.context.knownAccountV2, true);
  assert.equal(h.accountReads.length, 1);
  assert.equal(h.sockets.length, 1); assert.equal(h.sockets[0].v2, true);
  await h.context.resume(credential);
  assert.deepEqual(h.requests.map(request => request.path), ['/api/status', '/api/status']);
  assert.equal(h.sockets.length, 1); assert.equal(h.accountReads.length, 1);
  assert.equal(h.context.accountV2, true); assert.equal(h.context.connected, false);
  assert.equal(h.context.connecting, false); assert.equal(h.retries(), 0);
  assert.match(h.notices.at(-1), /unsupported account protocol/);
});

test('pre-pairing v2 discovery remains sticky across a different saved credential', async () => {
  const h = harness([{ ok: true }]);
  h.context.selectAccountProtocol(v2);
  await h.context.resume('b'.repeat(64));
  assert.deepEqual(h.requests.map(request => request.path), ['/api/status']);
  assert.equal(h.accountReads.length, 0); assert.equal(h.sockets.length, 0);
  assert.equal(h.context.knownAccountV2, true);
  assert.match(source, /then\(status => \{\s*selectAccountProtocol\(status\);/);
});

test('fresh loopback legacy visits remain supported, but advertised unsupported accounts fail closed', async () => {
  const legacy = harness([{ ok: true }]);
  await legacy.context.resume(credential);
  assert.deepEqual(legacy.requests.map(request => request.path), ['/api/status', '/api/session']);
  assert.deepEqual(JSON.parse(legacy.requests[1].options.body), { token: credential, resumeOnly: true });
  assert.equal(legacy.sockets[0].v2, false);
  assert.equal(legacy.context.knownAccountV2, false);
  const unsupported = harness([{ ok: true, accounts: { version: 1 } }]);
  await unsupported.context.resume(credential);
  assert.deepEqual(unsupported.requests.map(request => request.path), ['/api/status']);
  assert.equal(unsupported.sockets.length, 0);
});

test('a rejected downgrade does not prevent a later confirmed v2 reconnect', async () => {
  const h = harness([v2, { ok: true, accounts: { version: 1 } }, v2]);
  await h.context.resume(credential); await h.context.resume(credential); await h.context.resume(credential);
  assert.equal(h.requests.every(request => request.path === '/api/status'), true);
  assert.equal(h.accountReads.length, 2); assert.equal(h.sockets.length, 2);
  assert.equal(h.sockets.every(socket => socket.v2), true);
  assert.equal(h.context.knownAccountV2, true);
});
