import test from 'node:test';
import assert from 'node:assert/strict';
import { cachedProfile, durableTinyResult, readTinyCache, writeTinyCache, TINY_CACHE_KEY } from '../src/tiny-state.js';

test('offline view retains only public presentation, never credentials, jobs or leases', () => {
  const view = cachedProfile({ id: 'same-chao', name: 'A', token: 'secret', lease: { id: 'secret' }, training: { job: {} }, watch: { id: 'expired' }, rings: 14, stats: { run: { level: 8, points: 100, grade: 5, outcome: 90 }, fly: { level: 3 } }, ownedCosmetics: ['cap', null] });
  assert.deepEqual(view, { id: 'same-chao', name: 'A', rings: 14, stats: { run: { level: 8, points: 100, grade: 5 } }, ownedCosmetics: ['cap'] });
});

test('offline view is isolated by canonical profile, versioned and tolerant of unavailable storage', () => {
  const memory = new Map();
  const storage = { getItem: key => memory.get(key), setItem: (key, value) => memory.set(key, value) };
  writeTinyCache(storage, { id: 'a', rings: 3 }, 100);
  assert.equal(readTinyCache(storage, 'b'), null);
  assert.equal(readTinyCache(storage, 'a').syncedAt, 100);
  memory.set(TINY_CACHE_KEY, '{');
  assert.equal(readTinyCache(storage, 'a'), null);
  assert.doesNotThrow(() => writeTinyCache({ setItem() { throw Error(); } }, { id: 'a' }, 100));
});

test('only explicit durable receipts matching the current round can label a result saved', () => {
  const result = { id: 'run', gameId: 'memory', rings: 18, saved: true };
  assert.equal(durableTinyResult(result, { id: 'run', gameId: 'memory' }), true);
  for (const override of [{ saved: 'true' }, { saved: false }, { id: '' }, { rings: -1 }, { rings: 1.5 }, { gameId: 'unknown' }]) assert.equal(durableTinyResult({ ...result, ...override }), false);
  assert.equal(durableTinyResult(result, { id: 'old', gameId: 'memory' }), false);
  assert.equal(durableTinyResult(result, { id: 'run', gameId: 'ball' }), false);
});
