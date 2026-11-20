import test from 'node:test';
import assert from 'node:assert/strict';
import { acceptWidgetAccountSnapshot, acceptWidgetIdentity, authenticateWidgetSocket,
  readWidgetObservation, widgetProtocolVersion, widgetSocketUrl } from '../src/widget-observer.js';

const capabilities = { version: 2, authentication: ['bearer-header', 'websocket-first-frame'], durableTraining: false };
const identity = (revision = 4) => ({ schemaVersion: 2, accountId: 'account-a', chaoId: 'chao-a', deviceId: 'device-a',
  revision, lease: null, devices: [{ id: 'device-a', kind: 'desktop', revoked: false }] });
const snapshot = (revision = 4) => ({ type: 'snapshot', protocolVersion: 2, identity: identity(revision), selfId: 'chao-a',
  self: { id: 'chao-a', name: 'Canonical', rings: 42 }, players: [{ id: 'another-chao' }, { id: 'chao-a', name: 'Canonical', rings: 42 }] });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => structuredClone(data) });

test('v2 observation resumes the existing Chao and reads bearer account state without taking a lease', async () => {
  const calls = [], protocols = [];
  const result = await readWidgetObservation(async (path, options) => {
    calls.push({ path, options });
    if (path === '/api/status') return response({ ok: true, accounts: capabilities });
    if (path === '/api/session') return response({ token: 'opaque-test-credential', player: { id: 'chao-a' }, identity: identity() });
    if (path === '/api/account') return response({ capabilities, identity: identity(), snapshot: snapshot() });
    throw new Error('Unexpected request');
  }, 'opaque-test-credential', { onProtocol: version => protocols.push(version) });
  assert.deepEqual(calls.map(call => call.path), ['/api/status', '/api/session', '/api/account']);
  assert.deepEqual(JSON.parse(calls[1].options.body), { token: 'opaque-test-credential', resumeOnly: true });
  assert.equal(calls[2].options.method, 'GET');
  assert.equal(calls[2].options.headers.Authorization, 'Bearer opaque-test-credential');
  assert.equal(calls[2].options.credentials, 'omit');
  assert.ok(calls.every(call => !call.path.includes('opaque-test-credential')));
  assert.deepEqual(protocols, [2]);
  assert.equal(result.player.id, 'chao-a');
  assert.equal(result.player.rings, 42);
  assert.equal(result.identity.revision, 4);
});

test('a public list row cannot substitute for the private canonical self', () => {
  const source = snapshot();
  source.players[1].rings = 9999;
  assert.equal(acceptWidgetAccountSnapshot(source, identity()).player.rings, 42);
  for (const malformed of [
    { ...snapshot(), self: { id: 'another-chao', rings: 9999 } },
    { ...snapshot(), self: undefined },
    { ...snapshot(), selfId: 'another-chao' },
    { ...snapshot(), players: [{ id: 'chao-a' }, { id: 'chao-a' }] },
    { ...snapshot(), identity: { ...identity(), accountId: 'another-account' } },
    { ...snapshot(), identity: { ...identity(), deviceId: 'another-device' } },
  ]) assert.throws(() => acceptWidgetAccountSnapshot(malformed, identity()), /same canonical Chao/);
});

test('newer private revisions fence stale observations without rolling back canonical identity', () => {
  const latest = acceptWidgetIdentity(identity(7), identity(4));
  assert.equal(latest.revision, 7);
  assert.equal(acceptWidgetIdentity(identity(4), latest).revision, 7);
  assert.equal(acceptWidgetAccountSnapshot(snapshot(6), latest), null);
  assert.equal(acceptWidgetAccountSnapshot(snapshot(7), latest).identity.revision, 7);
});

test('v2 websocket keeps the credential out of its URL and sends only the authentication first frame', () => {
  const sent = [];
  const location = { protocol: 'https:', host: 'garden.example' };
  assert.equal(widgetSocketUrl(location, 2, 'opaque-test-credential'), 'wss://garden.example/ws');
  authenticateWidgetSocket({ send: value => sent.push(JSON.parse(value)) }, 2, 'opaque-test-credential');
  assert.deepEqual(sent, [{ type: 'authenticate', token: 'opaque-test-credential' }]);
  authenticateWidgetSocket({ send: () => assert.fail('Legacy must not send a v2 authentication frame') }, 1, 'opaque-test-credential');
  assert.throws(() => widgetSocketUrl(location, 3, 'opaque-test-credential'), /protocol/);
});

test('failed discovery pauses before any credential request and a known v2 service cannot downgrade', async () => {
  const calls = [];
  await assert.rejects(readWidgetObservation(async path => {
    calls.push(path); return response({ error: 'unavailable' }, 503);
  }, 'opaque-test-credential', { knownV2: true }), /capabilities could not be read/);
  assert.deepEqual(calls, ['/api/status']);
  await assert.rejects(readWidgetObservation(async path => {
    assert.equal(path, '/api/status'); return response({ ok: true });
  }, 'opaque-test-credential', { knownV2: true }), /protocol/);
  assert.throws(() => widgetProtocolVersion({ ok: true, accounts: { version: 3 } }), /protocol/);
  assert.throws(() => widgetProtocolVersion({ ok: true, accounts: { version: 2, authentication: ['bearer-header'] } }), /protocol/);
});

test('legacy discovery retains the existing local resume-only read contract', async () => {
  const calls = [];
  const result = await readWidgetObservation(async (path, options) => {
    calls.push(path);
    if (path === '/api/status') return response({ ok: true });
    assert.deepEqual(JSON.parse(options.body), { token: 'opaque-test-credential', resumeOnly: true });
    return response({ token: 'opaque-test-credential', player: { id: 'chao-a' } });
  }, 'opaque-test-credential');
  assert.equal(result.version, 1);
  assert.equal(result.player.id, 'chao-a');
  assert.deepEqual(calls, ['/api/status', '/api/session']);
});

test('HTTP account state cannot switch the resumed Chao or return an older identity', async () => {
  for (const account of [
    { capabilities, identity: { ...identity(), chaoId: 'another-chao' }, snapshot: snapshot() },
    { capabilities, identity: identity(3), snapshot: snapshot(3) },
  ]) {
    await assert.rejects(readWidgetObservation(async path => {
      if (path === '/api/status') return response({ ok: true, accounts: capabilities });
      if (path === '/api/session') return response({ token: 'opaque-test-credential', player: { id: 'chao-a' }, identity: identity() });
      return response(account);
    }, 'opaque-test-credential'), /same canonical Chao/);
  }
});
