import test from 'node:test';
import assert from 'node:assert/strict';
import { flameEmissionCoverage } from '../src/tiny-flame.js';

test('native dim flame radiance becomes transparent coverage without losing emitted RGB', () => {
  const source = new Uint8ClampedArray([0, 0, 88, 255, 24, 61, 107, 255, 0, 0, 0, 255, 10, 20, 30, 0]);
  const original = source.slice();
  const converted = flameEmissionCoverage(source);
  assert.deepEqual([...converted.slice(0, 4)], [0, 0, 255, 88]);
  assert.deepEqual([...converted.slice(8)], [0, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(source, original);
  for (let i = 0; i < source.length; i += 4) for (let channel = 0; channel < 3; channel++) {
    const before = source[i + channel] * source[i + 3] / 255;
    const after = converted[i + channel] * converted[i + 3] / 255;
    assert.ok(Math.abs(before - after) <= .51);
  }
});

test('partial original alpha scales coverage and retained radiance across the byte range', () => {
  const pixels = new Uint8Array(256 * 4);
  for (let value = 0; value < 256; value++) pixels.set([value, Math.floor(value * .63), Math.floor(value * .21), 129], value * 4);
  const converted = flameEmissionCoverage(pixels);
  for (let i = 0; i < pixels.length; i += 4) {
    assert.equal(converted[i + 3], Math.round(pixels[i] * 129 / 255));
    for (let channel = 0; channel < 3; channel++) {
      assert.ok(Math.abs(converted[i + channel] * converted[i + 3] / 255 - pixels[i + channel] * 129 / 255) <= 1);
    }
  }
  assert.throws(() => flameEmissionCoverage(new Uint8Array(3)), TypeError);
  assert.throws(() => flameEmissionCoverage([0, 0, 0, 0]), TypeError);
});
