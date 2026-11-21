import test from 'node:test';
import assert from 'node:assert/strict';
import { canvasSize, measureFont, scale2x, slotAt } from '../src/native-inventory.js';

test('slots run four across at the Chao Select pitch and the window grows a row at a time', () => {
  assert.deepEqual([0, 3, 4].map(slotAt), [{ x: 12, y: 24 }, { x: 180, y: 24 }, { x: 12, y: 80 }]);
  assert.deepEqual(canvasSize(8), { width: 240, height: 234, window: 202 });
  assert.equal(canvasSize(13).window, 24 + 4 * 56 + 16);
});

test('glyph widths come from the font image: opaque, bright columns are ink', () => {
  const width = 23 * 22, height = 22, data = new Uint8ClampedArray(width * height * 4);
  const ink = (x, y, red = 255, alpha = 255) => data.set([red, red, red, alpha], (y * width + x) * 4);
  const zero = '0'.charCodeAt(0) - 33; // cell 15
  ink(zero * 22 + 4, 5); ink(zero * 22 + 9, 7); ink(zero * 22 + 12, 3, 120); ink(zero * 22 + 14, 3, 255, 128);
  assert.deepEqual(measureFont({ data, width })['0'], [4, 6]);
});

test('scale2x smooths a diagonal edge with existing colours only', () => {
  const A = [255, 0, 0, 255], B = [0, 0, 255, 255];
  // A B
  // B B  -> the top-left pixel's lower-right corner takes B, where two B neighbours meet.
  const data = new Uint8ClampedArray([...A, ...B, ...B, ...B]);
  const out = scale2x(data, 2, 2);
  const at = (x, y) => [...out.subarray((y * 4 + x) * 4, (y * 4 + x) * 4 + 4)];
  assert.deepEqual(at(0, 0), A);
  assert.deepEqual(at(1, 1), B);
  assert.equal(out.length, 4 * 4 * 4);
  assert.ok([...Array(16).keys()].every(i => [A, B].some(c => c.every((v, k) => v === out[i * 4 + k]))), 'no new colours');
});
