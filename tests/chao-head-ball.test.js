import test from 'node:test';
import assert from 'node:assert/strict';
import { haloPicture, headBallColour, headIcon } from '../src/chao-head-ball.js';

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

test('the head icon follows the type: ring for hero, spike ball for dark, the three Chaos icons added to the screen', () => {
  assert.deepEqual(headIcon(2), { kind: 'ball', picture: 0, scale: 1, additive: false });
  assert.deepEqual(headIcon(5).kind, 'ball');
  assert.deepEqual(headIcon(9), { kind: 'halo', scale: 2, additive: false });
  assert.deepEqual(headIcon(21), { kind: 'halo', scale: 2, additive: true });
  assert.deepEqual(headIcon(19), { kind: 'spike', picture: 5, scale: 1.5, additive: false });
  assert.deepEqual(headIcon(22), { kind: 'flame', scale: 1.5, additive: true });
  assert.deepEqual(headIcon(20), { kind: 'glow', picture: 6, scale: 1.5, additive: true });
  assert.equal(headIcon(24).kind, 'ball'); // Knuckles is not a dark type
});

test('the ring picture is the table entry of the whole-degree lean, plus 6', () => {
  const table = Array.from({ length: 181 }, (_, degree) => degree); // made-up: entry = its own degree
  assert.equal(haloPicture(table, 1), 6);
  assert.equal(haloPicture(table, 0), 96);
  assert.equal(haloPicture(table, -1), 186);
  assert.equal(haloPicture(table, Math.cos(85.6 * Math.PI / 180)), 92); // rounds to 86 degrees
  assert.equal(haloPicture(table, 2), 6); // out-of-range input is held
});
