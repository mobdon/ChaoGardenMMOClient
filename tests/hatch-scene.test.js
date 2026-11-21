import test from 'node:test';
import assert from 'node:assert/strict';
import { createHatch } from '../src/hatch-scene.js';

const seq = (...v) => () => v.shift();
const shakeSteps = n => Math.ceil(n * 16 / 0.16);

test('Pon: the egg plays 27 frames at 0.16 and the newborn plays motion 206 to frame 19 at 0.2', () => {
  const h = createHatch(seq(0.2, 0.2));
  assert.equal(h.kind, 'Pon'); assert.equal(h.chaoState, 206);
  let n = 0; while (!h.done && n++ < 1000) h.step();
  assert.equal(h.shakes, 2);
  assert.ok(Math.abs(n - (shakeSteps(2) + Math.ceil(27 / 0.16))) <= 1, String(n));
  assert.equal(h.chaoFrame, 19);
});

test('Kyoro: 20 egg frames, newborn motion 205', () => {
  const h = createHatch(seq(0.7, 0.7));
  assert.equal(h.kind, 'Kyoro'); assert.equal(h.chaoState, 205);
  let n = 0; while (!h.done && n++ < 1000) h.step();
  assert.equal(h.shakes, 3);
  assert.ok(Math.abs(n - (shakeSteps(3) + Math.ceil(20 / 0.16))) <= 1, String(n));
});
