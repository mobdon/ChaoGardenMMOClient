import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeTrainingSnapshot, trainingView, trainingIntent, confirmsTrainingOperation, trainingDuration } from '../src/tiny-training.js';

const options = [{ id: 'run-short', stat: 'run', durationMs: 60000, costRings: 3, maxPoints: 1 }];
const job = { id: 'job-a', stat: 'run', phase: 'active', startedAt: 100000, endsAt: 160000, costRings: 3 };
const profile = (training = {}) => ({ id: 'chao-a', rings: 10, training: { revision: 2, rulesVersion: 'mmo-1', options, job: null, ...training } });
const ready = { player: profile(), capabilities: { training: { protocolVersion: 1 } }, connected: true, reconciled: true, serverTime: 110000 };

test('legacy and unknown protocol servers never enable training', () => {
  for (const capabilities of [undefined, {}, { training: true }, { training: { protocolVersion: 2 } }]) {
    const view = trainingView({ ...ready, capabilities });
    assert.equal(view.supported, false); assert.equal(view.canStart, false); assert.equal(view.remainingMs, null);
    assert.equal(trainingIntent(view, 'start', 'op-a'), null);
  }
});

test('training options retain only server-provided valid choices and all seven known stats', () => {
  const everyStat = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
  const raw = { revision: 3, rulesVersion: 'rules-a', job: null, options: everyStat.map((stat, i) => ({ ...options[0], id: `option-${i}`, stat })) };
  raw.options.push({ ...options[0], id: 'negative', costRings: -1 }, { ...options[0], id: 'unknown', stat: 'attack' }, raw.options[0]);
  const snapshot = normalizeTrainingSnapshot(raw, 100);
  assert.equal(snapshot.options.length, 7); assert.deepEqual(snapshot.options.map(item => item.stat), everyStat);
  assert.deepEqual(raw.options[0], { ...options[0], id: 'option-0', stat: 'swim' });
});

test('malformed training state cannot masquerade as an idle job', () => {
  for (const change of [{ revision: -1 }, { rulesVersion: '' }, { options: null }, { job: undefined }, { job: { ...job, endsAt: job.startedAt } }, { job: { ...job, phase: 'complete' } }]) {
    const view = trainingView({ ...ready, player: profile(change) });
    assert.equal(view.snapshot, null); assert.equal(view.canStart, false);
    assert.equal(view.blocksActivity, true);
  }
});

test('start submits server option and revision only, with no client reward or clock', () => {
  const view = trainingView(ready);
  assert.deepEqual(trainingIntent(view, 'start', 'op-a'), {
    type: 'action', action: 'training-start', operationId: 'op-a', optionId: 'run-short', revision: 2,
  });
  assert.equal(trainingIntent(view, 'start', ''), null);
});

test('offline, unreconciled, busy, pending and unaffordable states block starts', () => {
  for (const change of [{ connected: false }, { reconciled: false }, { busy: true }, { pending: { operationId: 'op-a' } }, { player: { ...profile(), rings: 2 } }]) {
    assert.equal(trainingView({ ...ready, ...change }).canStart, false);
  }
  assert.equal(trainingView({ ...ready, player: { ...profile(), rings: undefined } }).canStart, false);
});

test('server timestamps are the only countdown source and never imply readiness', () => {
  const player = profile({ job });
  assert.equal(trainingView({ ...ready, player, serverTime: 120000 }).remainingMs, 40000);
  const expired = trainingView({ ...ready, player, serverTime: 9999999999999 });
  assert.equal(expired.remainingMs, 0); assert.equal(expired.canClaim, false);
  assert.equal(expired.status, 'Waiting for server completion');
  for (const serverTime of [undefined, NaN, -1, '120000', 99999]) {
    assert.equal(trainingView({ ...ready, player, serverTime }).remainingMs, null);
  }
});

test('local wall clock changes cannot change the countdown or submit readiness', () => {
  const before = Date.now;
  try {
    Date.now = () => Number.MAX_SAFE_INTEGER;
    assert.equal(trainingView({ ...ready, player: profile({ job }) }).remainingMs, 50000);
    assert.equal(trainingView({ ...ready, player: profile({ job }) }).canClaim, false);
  } finally { Date.now = before; }
});

test('cancel and claim bind to the current canonical job and phase', () => {
  const active = trainingView({ ...ready, player: profile({ job }) });
  assert.deepEqual(trainingIntent(active, 'cancel', 'op-cancel'), {
    type: 'action', action: 'training-cancel', operationId: 'op-cancel', jobId: 'job-a', revision: 2,
  });
  assert.equal(trainingIntent(active, 'claim', 'op-claim'), null);
  const completed = trainingView({ ...ready, player: profile({ job: { ...job, phase: 'ready' } }) });
  assert.equal(trainingIntent(completed, 'claim', 'op-claim').jobId, 'job-a');
  assert.equal(trainingIntent(completed, 'cancel', 'op-cancel'), null);
});

test('saving jobs and unsettled results never expose gains or enable another start', () => {
  const result = { id: 'result-a', points: 7, savedAt: 200000, saved: false };
  for (const phase of ['active', 'ready', 'saving']) {
    const view = trainingView({ ...ready, player: profile({ job: { ...job, phase, result } }) });
    assert.equal(view.result, null); assert.equal(view.canStart, false);
  }
  const unsaved = trainingView({ ...ready, player: profile({ job: { ...job, phase: 'settled', result } }) });
  assert.equal(unsaved.result, null); assert.equal(unsaved.status, 'Awaiting saved result');
  assert.equal(unsaved.canStart, false); assert.equal(unsaved.blocksActivity, true);
});

test('only an explicitly durable server result is presented as saved', () => {
  const view = trainingView({ ...ready, player: profile({ job: { ...job, phase: 'settled', result: { id: 'result-a', points: 7, savedAt: 200000, saved: true } } }) });
  assert.deepEqual(view.result, { id: 'result-a', points: 7, savedAt: 200000 });
});

test('operation confirmation rejects unsaved, stale, cross-profile and cross-job acknowledgements', () => {
  const pending = { operationId: 'op-a', profileId: 'chao-a', jobId: 'job-a', revision: 2 };
  const event = { kind: 'training-saved', saved: true, operationId: 'op-a', profileId: 'chao-a', jobId: 'job-a', revision: 3 };
  assert.equal(confirmsTrainingOperation(pending, event), true);
  for (const change of [{ saved: false }, { revision: 2 }, { revision: NaN }, { profileId: 'chao-b' }, { operationId: 'old-op' }, { jobId: 'old-job' }]) {
    assert.equal(confirmsTrainingOperation(pending, { ...event, ...change }), false);
  }
});

test('durations are explicit and invalid input never produces a fabricated time', () => {
  assert.equal(trainingDuration(0), '0s'); assert.equal(trainingDuration(60000), '1 min');
  assert.equal(trainingDuration(3600000), '1h 0m'); assert.equal(trainingDuration(NaN), 'Time not confirmed');
});
