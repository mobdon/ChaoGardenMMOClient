import test from 'node:test';
import assert from 'node:assert/strict';
import { gotoLine } from '../asset-tools/crates/sa2-web/www/staff-goto.js';

test('a staff-goto event becomes the line the game reads', () => {
  assert.equal(gotoLine({ room: 'garden', pose: { x: 1.5, y: 0, z: -20, angle: 3 } }, 7), '7 1 1.5 0 -20');
  assert.equal(gotoLine({ room: 'hub', pose: { x: 1, y: 2, z: 3 } }, 1), '1 0 1 2 3');
  assert.equal(gotoLine({ room: 'chao-karate', pose: null }, 2), '2 10');
});

test('a bad place is a room alone, and a room outside Chao World is nothing', () => {
  assert.equal(gotoLine({ room: 'dark', pose: { x: 1, y: 'up', z: 3 } }, 3), '3 3');
  assert.equal(gotoLine({ room: 'dark', pose: { x: 1, y: Infinity, z: 3 } }, 3), '3 3');
  assert.equal(gotoLine({ room: 'tiny', pose: { x: 1, y: 2, z: 3 } }, 4), null);
  assert.equal(gotoLine({ room: 'constructor' }, 5), null);
  assert.equal(gotoLine({}, 6), null);
});
