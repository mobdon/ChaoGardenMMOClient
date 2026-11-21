import test from 'node:test';
import assert from 'node:assert/strict';
import { createHatch } from '../src/hatch-scene.js';

test('Pon: the egg plays 27 frames at 0.16 and the newborn plays motion 206 to frame 19 at 0.2', () => {
  const h = createHatch(() => 0.2);
  assert.equal(h.kind, 'Pon'); assert.equal(h.chaoState, 206);
  let n = 0; while (!h.done && n++ < 1000) h.step();
  assert.equal(n, Math.ceil(27 / 0.16));
  assert.equal(h.chaoFrame, 19);
});

test('Kyoro: 20 egg frames, newborn motion 205', () => {
  const h = createHatch(() => 0.7);
  assert.equal(h.kind, 'Kyoro'); assert.equal(h.chaoState, 205);
  let n = 0; while (!h.done && n++ < 1000) h.step();
  assert.equal(n, Math.ceil(20 / 0.16));
});
