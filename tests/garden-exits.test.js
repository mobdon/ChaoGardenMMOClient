import test from 'node:test';
import assert from 'node:assert/strict';
import { attachNativeGardenExit, nativeGardenExitAction } from '../src/garden-exits.js';
import { NATIVE_GARDENS } from '../src/world-layout.js';

function mesh(zone, changes = {}) {
  const exit = NATIVE_GARDENS[zone].exit;
  const vector = values => Object.fromEntries(['x', 'y', 'z'].map((axis, i) => [axis, values[i]]));
  return { name: `native-${zone}-${exit.meshes[0]}`, metadata: { nativeTerrain: true },
    isEnabled: () => true, getTotalVertices: () => 12, computeWorldMatrix() {},
    getBoundingInfo: () => ({ boundingBox: { minimumWorld: vector(exit.bounds.min), maximumWorld: vector(exit.bounds.max) } }),
    ...changes };
}
function point(zone) {
  const { min, max } = NATIVE_GARDENS[zone].exit.bounds;
  return Object.fromEntries(['x', 'y', 'z'].map((axis, i) => [axis, (min[i] + max[i]) / 2]));
}

test('each verified native recess submits the existing hub intent; Hero retains its reachability limitation', () => {
  for (const zone of ['garden', 'hero', 'dark']) {
    const source = mesh(zone), result = attachNativeGardenExit(zone, [source]);
    assert.equal(result.ready, true);
    assert.equal(result.reachable, zone !== 'hero');
    assert.equal(source.metadata.nativeTerrain, true);
    assert.equal(nativeGardenExitAction(zone, { hit: true, pickedMesh: source, pickedPoint: point(zone) }), 'hub');
  }
});

test('a click elsewhere on the same native cliff, a wrong zone or a nonfinite point cannot activate the recess', () => {
  const source = mesh('garden'); attachNativeGardenExit('garden', [source]);
  const pick = { hit: true, pickedMesh: source, pickedPoint: point('garden') };
  assert.equal(nativeGardenExitAction('garden', { ...pick, pickedPoint: { ...pick.pickedPoint, x: 0 } }), null);
  assert.equal(nativeGardenExitAction('garden', { ...pick, pickedPoint: { ...pick.pickedPoint, y: NaN } }), null);
  assert.equal(nativeGardenExitAction('dark', pick), null);
  assert.equal(nativeGardenExitAction('garden', { ...pick, hit: false }), null);
  assert.equal(nativeGardenExitAction('garden', undefined), null);
  source.isEnabled = () => false;
  assert.equal(nativeGardenExitAction('garden', pick), null);
});

test('missing, renamed, disabled or displaced native geometry preserves the procedural fallback', () => {
  const displaced = mesh('garden', { getBoundingInfo: () => ({ boundingBox: {
    minimumWorld: { x: 0, y: 0, z: 0 }, maximumWorld: { x: 1, y: 1, z: 1 },
  } }) });
  for (const candidates of [[], [mesh('garden', { name: 'invented-door' })],
    [mesh('garden', { isEnabled: () => false })], [displaced]]) {
    assert.equal(attachNativeGardenExit('garden', candidates).ready, false);
    for (const candidate of candidates) assert.equal(candidate.metadata.action, undefined);
  }
  assert.equal(attachNativeGardenExit('hub', []).ready, false);
});
