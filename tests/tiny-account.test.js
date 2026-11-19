import test from 'node:test';
import assert from 'node:assert/strict';
import { createTinyAccount, normalizeAccountIdentity, supportsTinyAccounts, tinyIntentActivity } from '../src/tiny-account.js';

const status = { accounts: { version: 2, activityLeases: true, intentSequence: true, authentication: ['bearer-header', 'websocket-first-frame'] } };
const baseIdentity = () => ({ schemaVersion: 2, accountId: 'account-a', chaoId: 'chao-a', deviceId: 'device-a', revision: 7, devices: [], lease: null });
const lease = (activity = 'tiny', extra = {}) => ({ id: 'lease-a', deviceId: 'device-a', activity, expiresAt: 30000, lastSequence: 4, ...extra });
const response = (value, ok = true) => ({ ok, json: async () => value });
const tick = () => new Promise(resolve => setTimeout(resolve, 0));

function fixture({ identity: initial = baseIdentity(), now = () => 0 } = {}) {
  let identity = structuredClone(initial);
  const calls = [], wires = [], notices = [], states = [];
  let leaseError = null;
  const snapshot = () => ({ type: 'snapshot', protocolVersion: 2, self: { id: identity.chaoId, rings: 12 }, identity: structuredClone(identity) });
  const fetch = async (path, options) => {
    calls.push({ path, ...options });
    if (path === '/api/account') return response({ identity: structuredClone(identity), snapshot: snapshot(), capabilities: { ...status.accounts, durableTraining: false } });
    if (path === '/api/lease') {
      const body = JSON.parse(options.body);
      if (leaseError) { const error = leaseError; leaseError = null; return response({ error, protocolVersion: 2 }, false); }
      assert.equal(body.expectedRevision, identity.revision);
      identity.revision++;
      if (body.operation === 'acquire') identity.lease = lease(body.activity, { id: 'lease-new', lastSequence: 0 });
      if (body.operation === 'release') identity.lease = null;
      return response({ identity: structuredClone(identity) });
    }
    throw new Error(`Unexpected path ${path}`);
  };
  const account = createTinyAccount({ fetch, transmit: message => { wires.push(message); return true; },
    onState: state => states.push(state), onError: (text, code) => notices.push({ text, code }), now });
  const connect = () => account.connect({ token: 'private-test-credential', status });
  function acknowledge({ kind = 'intent-accepted', sequence = wires.at(-1)?.sequence, ...extra } = {}) {
    identity.revision++; identity.lease.lastSequence = sequence;
    account.consume({ type: 'event', kind, sequence, identity: structuredClone(identity), ...extra });
  }
  return { account, calls, wires, notices, states, connect, acknowledge, snapshot,
    get identity() { return identity; }, set leaseError(value) { leaseError = value; } };
}

test('v2 account discovery and identity validation fail closed', () => {
  assert.equal(supportsTinyAccounts(status), true);
  for (const value of [{}, { accounts: { ...status.accounts, version: 3 } }, { accounts: { ...status.accounts, intentSequence: false } }]) assert.equal(supportsTinyAccounts(value), false);
  assert.equal(normalizeAccountIdentity(baseIdentity()).revision, 7);
  for (const value of [{ ...baseIdentity(), schemaVersion: 1 }, { ...baseIdentity(), lease: {} }, { ...baseIdentity(), revision: NaN }, { ...baseIdentity(), lease: lease('unknown') }, { ...baseIdentity(), devices: [null] }]) assert.equal(normalizeAccountIdentity(value), null);
});

test('credentials appear only in bearer headers and are never exposed in callback state', async () => {
  const f = fixture(); const result = await f.connect();
  assert.equal(result.snapshot.self.id, 'chao-a'); assert.equal(f.account.ready, true);
  assert.equal(f.calls[0].path, '/api/account');
  assert.equal(f.calls[0].headers.Authorization, 'Bearer private-test-credential');
  assert.equal(f.calls[0].cache, 'no-store');
  assert.doesNotMatch(JSON.stringify(f.states), /private-test-credential/);
});

test('first input acquires its exact activity and submits the latest revision and sequence', async () => {
  const f = fixture(); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'tiny-start', gameId: 'memory' }); await tick();
  assert.deepEqual(JSON.parse(f.calls[1].body), { operation: 'acquire', expectedRevision: 7, activity: 'tiny' });
  assert.deepEqual(f.wires[0], { type: 'action', action: 'tiny-start', gameId: 'memory', leaseId: 'lease-new', expectedRevision: 8, sequence: 1 });
  f.acknowledge(); assert.equal(await sending, true);
});

test('intents serialize through durable acknowledgements and concurrent inputs are dropped', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease() } }); await f.connect();
  const sending = f.account.send({ type: 'tiny-input', sessionId: 'game-a', input: { kind: 'move', x: 1, y: 0 } }); await tick();
  assert.equal(await f.account.send({ type: 'action', action: 'watch-care', kind: 'pet' }), false);
  assert.equal(f.wires.length, 1); assert.equal(f.wires[0].sequence, 5);
  // A stale acknowledgement and an ordinary snapshot do not release the lock.
  f.account.consume({ type: 'event', kind: 'intent-accepted', sequence: 4, identity: { ...f.identity } });
  f.account.consume(f.snapshot()); assert.equal(f.account.pending, true);
  f.acknowledge(); assert.equal(await sending, true);
  const second = f.account.send({ type: 'tiny-input', sessionId: 'game-a', input: { kind: 'move', x: 0, y: 0 } }); await tick();
  assert.equal(f.wires[1].sequence, 6); f.acknowledge(); await second;
});

test('watch saved accepted:false still confirms durable transport and does not imply a care gain', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('watch') } }); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'watch-care', sessionId: 'watch-a', kind: 'pet' }); await tick();
  f.acknowledge({ kind: 'watch-saved', accepted: false }); assert.equal(await sending, true);
  assert.equal(f.account.pending, false); assert.equal(f.wires.length, 1);
});

test('changing activity explicitly releases the owned lease before acquiring another', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('watch') } }); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'tiny-start', gameId: 'ball' }); await tick();
  const operations = f.calls.filter(call => call.path === '/api/lease').map(call => JSON.parse(call.body));
  assert.deepEqual(operations.map(body => body.operation), ['renew', 'release', 'acquire']);
  assert.equal(operations.at(-1).activity, 'tiny');
  f.acknowledge(); assert.equal(await sending, true);
});

test('reconnect renews the existing lease without resetting its sequence counter', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('watch', { lastSequence: 19 }) } }); await f.connect();
  assert.deepEqual(JSON.parse(f.calls[1].body), { operation: 'renew', expectedRevision: 7, leaseId: 'lease-a' });
  const sending = f.account.send({ type: 'action', action: 'watch-stop', sessionId: 'watch-a' }); await tick();
  assert.equal(f.wires[0].sequence, 20); f.acknowledge({ kind: 'watch-saved' }); await sending;
});

test('expired reconnect lease re-reads canonical ownership and acquires a fresh generation', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease() } }); f.leaseError = 'lease_expired';
  await f.connect();
  assert.deepEqual(f.calls.map(call => call.path), ['/api/account', '/api/lease', '/api/account', '/api/lease']);
  assert.equal(f.account.identity.lease.id, 'lease-new'); assert.equal(f.account.identity.lease.lastSequence, 0);
  assert.equal(f.wires.length, 0);
});

test('another device lease never gets released or used by this client', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('tiny', { deviceId: 'other-device' }) } }); await f.connect();
  assert.equal(await f.account.send({ type: 'action', action: 'tiny-start', gameId: 'memory' }), false);
  assert.equal(f.calls.some(call => call.path === '/api/lease'), false); assert.equal(f.wires.length, 0);
  assert.equal(f.notices.at(-1).code, 'activity_conflict');
});

test('fencing rejection reconciles canonical state without replaying the mutation', async () => {
  const f = fixture(); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'tiny-start', gameId: 'search' }); await tick();
  f.account.consume({ type: 'error', error: 'stale_revision', protocolVersion: 2 });
  assert.equal(await sending, false); await tick();
  assert.equal(f.account.ready, true); assert.equal(f.wires.length, 1);
  assert.equal(f.calls.filter(call => call.path === '/api/account').length, 2);
  assert.equal(f.notices.at(-1).code, 'stale_revision');
});

test('save remains lease exempt and sends no client profile or sequence', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('tiny', { deviceId: 'other-device' }) } }); await f.connect();
  assert.equal(await f.account.send({ type: 'action', action: 'save', rings: 999 }), true);
  assert.deepEqual(f.wires[0], { type: 'action', action: 'save' });
  assert.equal(f.calls.length, 1);
});

test('disconnect aborts pending confirmation and never sends a queued action', async () => {
  const f = fixture(); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'tiny-stop', sessionId: 'game-a' }); await tick();
  f.account.disconnect(); assert.equal(await sending, false); assert.equal(f.account.ready, false);
  assert.equal(await f.account.send({ type: 'action', action: 'tiny-start', gameId: 'ball' }), false);
  assert.equal(f.wires.length, 1);
});

test('old snapshots never roll back a newer canonical revision', async () => {
  const f = fixture(); await f.connect();
  f.account.consume({ type: 'snapshot', protocolVersion: 2, self: { id: 'chao-a', rings: 20 }, identity: { ...baseIdentity(), revision: 10 } });
  assert.equal(f.account.consume({ type: 'snapshot', protocolVersion: 2, self: { id: 'chao-a', rings: 12 }, identity: baseIdentity() }), false);
  assert.equal(f.account.identity.revision, 10);
  assert.equal(f.account.snapshot.self.rings, 20);
});

test('intent activity mapping matches Tiny, Watch and current direct host actions', () => {
  assert.equal(tinyIntentActivity({ type: 'tiny-input' }), 'tiny');
  assert.equal(tinyIntentActivity({ action: 'tiny-start' }), 'tiny');
  assert.equal(tinyIntentActivity({ action: 'watch-care' }), 'watch');
  assert.equal(tinyIntentActivity({ action: 'buy' }), 'direct');
});

test('a response from a disconnected generation cannot restore account controls', async () => {
  let release;
  const responsePending = new Promise(resolve => { release = resolve; });
  const account = createTinyAccount({ fetch: () => responsePending, transmit: () => true });
  const connecting = account.connect({ token: 'credential-a', status });
  account.disconnect();
  release(response({ identity: baseIdentity(), capabilities: status.accounts,
    snapshot: { protocolVersion: 2, self: { id: 'chao-a' } } }));
  await assert.rejects(connecting, /Connection interrupted/);
  assert.equal(account.ready, false); assert.equal(account.identity, null);
});

test('explicit take-control asks the server about a retained foreign lease without releasing it', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('tiny', { deviceId: 'other-device' }) } }); await f.connect();
  assert.equal(await f.account.takeControl('tiny'), true);
  assert.deepEqual(JSON.parse(f.calls[1].body), { operation: 'acquire', activity: 'tiny', expectedRevision: 7 });
  assert.equal(f.account.identity.lease.deviceId, 'device-a');
  assert.equal(f.wires.length, 0);
});

test('explicit take-control preserves a foreign active lease when the server rejects acquisition', async () => {
  const f = fixture({ identity: { ...baseIdentity(), lease: lease('watch', { deviceId: 'other-device' }) } }); await f.connect();
  f.leaseError = 'activity_conflict'; assert.equal(await f.account.takeControl('tiny'), false);
  assert.equal(f.account.identity.lease.deviceId, 'other-device');
  assert.equal(f.calls.filter(call => call.path === '/api/lease').length, 1);
  assert.equal(JSON.parse(f.calls[1].body).operation, 'acquire');
});

test('account requests time out and abort without enabling controls', async () => {
  let signal;
  const account = createTinyAccount({ fetch: (_path, options) => { signal = options.signal; return new Promise(() => {}); },
    transmit: () => true, requestTimeoutMs: 15 });
  await assert.rejects(account.connect({ token: 'credential-a', status }), /did not respond in time/);
  assert.equal(signal.aborted, true); assert.equal(account.ready, false);
});

test('whenIdle waits for the current durable transport acknowledgement and sends nothing itself', async () => {
  const f = fixture(); await f.connect();
  const sending = f.account.send({ type: 'action', action: 'tiny-start', gameId: 'memory' });
  const waiting = f.account.whenIdle(); let resolved = false; void waiting.then(() => { resolved = true; });
  await tick(); assert.equal(resolved, false);
  f.acknowledge(); assert.equal(await sending, true); assert.equal(await waiting, true);
  assert.equal(f.wires.length, 1);
  f.account.disconnect(); assert.equal(await f.account.whenIdle(), false);
});

test('a failed request from an old generation cannot pause or reconcile a newly connected account', async () => {
  let completeOldLease; let accounts = 0; const notices = [];
  const firstIdentity = baseIdentity();
  const secondIdentity = { ...baseIdentity(), accountId: 'account-b', chaoId: 'chao-b', deviceId: 'device-b' };
  const account = createTinyAccount({
    fetch: async (path, options) => {
      if (path === '/api/account') {
        accounts++; const identity = options.headers.Authorization.endsWith('credential-b') ? secondIdentity : firstIdentity;
        return response({ identity, snapshot: { type: 'snapshot', protocolVersion: 2, self: { id: identity.chaoId }, identity }, capabilities: status.accounts });
      }
      return new Promise(resolve => { completeOldLease = resolve; });
    }, transmit: () => true, onError: text => notices.push(text),
  });
  await account.connect({ token: 'credential-a', status });
  const oldSending = account.send({ type: 'action', action: 'tiny-start', gameId: 'memory' }); await tick();
  account.disconnect(); await account.connect({ token: 'credential-b', status });
  completeOldLease(response({ identity: { ...firstIdentity, revision: 8, lease: lease() } }));
  assert.equal(await oldSending, false); assert.equal(account.ready, true);
  assert.equal(account.identity.chaoId, 'chao-b'); assert.equal(accounts, 2); assert.equal(notices.length, 0);
});

test('an older HTTP account response cannot overwrite a newer socket snapshot', async () => {
  let release; let requests = 0; const snapshots = [];
  const initial = { identity: baseIdentity(), capabilities: status.accounts,
    snapshot: { type: 'snapshot', protocolVersion: 2, self: { id: 'chao-a', rings: 12 }, identity: baseIdentity() } };
  const account = createTinyAccount({ fetch: async () => ++requests === 1 ? response(initial) : new Promise(resolve => { release = resolve; }),
    transmit: () => true, onSnapshot: snapshot => snapshots.push(snapshot) });
  await account.connect({ token: 'credential-a', status });
  const refreshing = account.reconcile(); await tick();
  account.consume({ type: 'snapshot', protocolVersion: 2, self: { id: 'chao-a', rings: 20 }, identity: { ...baseIdentity(), revision: 10 } });
  release(response(initial)); await refreshing;
  assert.equal(account.identity.revision, 10); assert.equal(account.snapshot.self.rings, 20);
  assert.equal(snapshots.length, 1);
});
