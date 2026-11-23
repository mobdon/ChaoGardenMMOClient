import assert from 'node:assert/strict';
import test from 'node:test';
import { CELL, GRID, sheetFrame } from '../src/tiny-sheet.js';

test('the sheet is 768 px of 96 px cells and each clip gives four evenly spread frames', () => {
  assert.equal(CELL * GRID, 768);
  assert.deepEqual([0, 1, 2, 3].map(c => sheetFrame(0, 40, c)), [0, 10, 20, 30]);
  assert.deepEqual([4, 5, 6, 7].map(c => sheetFrame(8, 24, c)), [8, 12, 16, 20]);
});
