import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readWidgetSession, widgetSnapshotView } from '../src/widget-state.js';

test('disconnected and partial snapshots never manufacture needs, levels or AFK state', () => {
  const empty = widgetSnapshotView(null);
  assert.equal(empty.rings, null);
  assert.deepEqual(Object.values(empty.needs), [null, null, null]);
  assert.equal(empty.stats.length, 7);
  assert.ok(empty.stats.every(stat => stat.level === null && stat.points === null && stat.grade === null));
  assert.equal(widgetSnapshotView({ training: { status: 'none' } }).training, 'AFK status unavailable');
  const partial = widgetSnapshotView({ energy: 0, hunger: 101, happiness: '50', rings: 0, stats: { run: { level: 8, points: 903, grade: 5 } } });
  assert.equal(partial.needs.energy, 0);
  assert.equal(partial.needs.hunger, null);
  assert.equal(partial.needs.happiness, null);
  assert.equal(partial.rings, 0);
  assert.deepEqual(partial.stats.find(stat => stat.key === 'run'), { key: 'run', label: 'Run', level: 8, points: 903, grade: 5 });
  assert.equal(partial.stats.find(stat => stat.key === 'luck').level, null);
});

test('display preserves all seven canonical stat rows and reported garden activity', () => {
  const keys = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
  const stats = Object.fromEntries(keys.map((key, i) => [key, { level: i + 1, points: i * 100, grade: i }]));
  const profile = { rings: 1234, energy: 87.4, hunger: 31, happiness: 99, watch: { activity: 'rest' }, stats };
  const before = structuredClone(profile);
  const view = widgetSnapshotView(profile);
  assert.deepEqual(view.stats.map(stat => stat.key), keys);
  assert.deepEqual(view.stats.map(stat => stat.level), [1, 2, 3, 4, 5, 6, 7]);
  assert.equal(view.needs.energy, 87.4);
  assert.equal(view.routine, 'Resting in the garden');
  assert.deepEqual(profile, before);
  assert.equal(widgetSnapshotView({ watch: null }).routine, 'Garden routine is off');
  assert.equal(widgetSnapshotView({}).routine, 'Garden activity not reported');
});

test('resume sends only the existing resume-only contract and requires canonical identity', async () => {
  let sent;
  const data = await readWidgetSession(async (path, options) => {
    sent = { path, options };
    return { ok: true, status: 200, json: async () => ({ token: 'existing-token', player: { id: 'canonical-chao' } }) };
  }, 'existing-token');
  assert.equal(sent.path, '/api/session');
  assert.deepEqual(JSON.parse(sent.options.body), { token: 'existing-token', resumeOnly: true });
  assert.equal(sent.options.cache, 'no-store');
  assert.equal(data.player.id, 'canonical-chao');
  await assert.rejects(readWidgetSession(async () => ({ ok: true, status: 200,
    json: async () => ({ token: 'existing-token', player: { name: 'No identity' } }) }), 'existing-token'), /could not confirm/);
});

test('a stalled response body times out and aborts without submitting another request', async () => {
  let signal, calls = 0;
  await assert.rejects(readWidgetSession(async (_, options) => {
    calls++; signal = options.signal;
    return { ok: true, status: 200, json: () => new Promise(() => {}) };
  }, 'existing-token', { timeoutMs: 15 }), { name: 'TimeoutError' });
  assert.equal(signal.aborted, true);
  assert.equal(calls, 1);
});

test('a missing saved identity stops automatic retry while service failures may retry', async () => {
  const failure = status => async () => ({ ok: false, status, json: async () => ({ error: 'server detail' }) });
  await assert.rejects(readWidgetSession(failure(404), 'existing-token'), error => error.retry === false && !error.message.includes('server detail'));
  await assert.rejects(readWidgetSession(failure(503), 'existing-token'), error => error.retry === true);
});

test('a real 307 cannot forward the resume credential body to another origin', async () => {
  let received = 0;
  const target = createServer((request, response) => { received++; response.writeHead(200).end('{}'); });
  const redirect = createServer((request, response) => {
    request.resume();
    response.writeHead(307, { Location: `http://127.0.0.1:${target.address().port}/receive` }).end();
  });
  try {
    await new Promise(resolve => target.listen(0, '127.0.0.1', resolve));
    await new Promise(resolve => redirect.listen(0, '127.0.0.1', resolve));
    const origin = `http://127.0.0.1:${redirect.address().port}`;
    await assert.rejects(readWidgetSession((path, options) => fetch(new URL(path, origin), options), 'synthetic-redirect-test-token'));
    assert.equal(received, 0);
  } finally {
    for (const server of [redirect, target]) {
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  }
});
