import test from 'node:test';
import assert from 'node:assert/strict';
import { headBallColour } from '../src/chao-head-ball.js';

// Made-up tables: every colour of table t, row r, column c is (t*100 + r*10 + c) in all three channels.
const tables = [0, 1, 2].map(t => Uint8Array.from({ length: 108 }, (_, i) => t * 100 + Math.floor(i / 18) * 10 + Math.floor(i % 18 / 3)));
const look = (type, hPos = 0, vPos = 0, aPos = 0, growth = 0) => ({ type, hPos, vPos, aPos, growth });

test('a fresh child shows the neutral zero colour, and alignment moves it to the hero or dark one', () => {
  assert.deepEqual(headBallColour(tables, look(2)), [0, 0, 0]);
  assert.deepEqual(headBallColour(tables, look(2, 0, 0, 1)), [100, 100, 100]);
  assert.deepEqual(headBallColour(tables, look(2, 0, 0, -1)), [200, 200, 200]);
});

test('a fully grown power child lands one under the power colour, as the game truncates', () => {
  // The zero swim/fly value counts as 1e-6, so the weight of the power colour is just under 1.
  assert.deepEqual(headBallColour(tables, look(2, 1, 0, 0, 1)), [4, 4, 4]);
  assert.deepEqual(headBallColour(tables, look(2, -1, 0, 0, 1)), [3, 3, 3]);
});

test('an adult reads its family table and second-evolution row', () => {
  assert.deepEqual(headBallColour(tables, look(9, 0, 1, 0, 1)), [123, 123, 123]); // hero swim, fully fly
  assert.deepEqual(headBallColour(tables, look(19)), [250, 250, 250]); // dark power, not grown
});

test('Chaos and character Chao have fixed colours', () => {
  assert.deepEqual(headBallColour(tables, look(20)), [255, 255, 255]);
  assert.deepEqual(headBallColour(tables, look(25)), [255, 0, 0]);
  assert.equal(headBallColour(tables, look(1)), null);
});
