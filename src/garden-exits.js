import { NATIVE_GARDENS } from './world-layout.js';

// Source bounds are recorded to four decimals. Permit only a small rounding
// margin; never make the whole cliff primitive a return-to-hub click target.
const ROUNDING_MARGIN = .002;
const axes = ['x', 'y', 'z'];

function contains(bounds, point) {
  return axes.every((axis, i) => Number.isFinite(point?.[axis])
    && point[axis] >= bounds.min[i] - ROUNDING_MARGIN
    && point[axis] <= bounds.max[i] + ROUNDING_MARGIN);
}

function nativeNameMatches(name, expected) {
  return typeof name === 'string' && (name === expected || name.endsWith(`-${expected}`));
}

export function attachNativeGardenExit(zone, meshes) {
  const exit = NATIVE_GARDENS[zone]?.exit;
  const attached = [];
  for (const mesh of exit ? meshes : []) {
    if (!exit.meshes.some(name => nativeNameMatches(mesh.name, name))
      || !mesh.isEnabled() || !mesh.getTotalVertices()) continue;
    mesh.computeWorldMatrix(true);
    const box = mesh.getBoundingInfo().boundingBox;
    // Reject stale/wrong geometry instead of silently placing an arbitrary
    // invisible trigger. Native source geometry must contain its recess.
    if (!axes.every((axis, i) => Number.isFinite(box.minimumWorld[axis]) && Number.isFinite(box.maximumWorld[axis])
      && exit.bounds.min[i] >= box.minimumWorld[axis] - ROUNDING_MARGIN
      && exit.bounds.max[i] <= box.maximumWorld[axis] + ROUNDING_MARGIN)) continue;
    mesh.metadata = { ...mesh.metadata, action: 'hub', nativeGardenExit: zone };
    attached.push(mesh.name);
  }
  return { ready: attached.length > 0, meshes: attached, action: 'hub', reachable: !!exit?.reachable,
    trigger: 'adapted-native-recess-click' };
}

export function nativeGardenExitAction(zone, pick) {
  const exit = NATIVE_GARDENS[zone]?.exit;
  const mesh = pick?.pickedMesh;
  if (!exit || !pick?.hit || !mesh || !mesh.isEnabled()
    || mesh.metadata?.nativeGardenExit !== zone || mesh.metadata.action !== 'hub'
    || !exit.meshes.some(name => nativeNameMatches(mesh.name, name))) return null;
  return contains(exit.bounds, pick.pickedPoint) ? 'hub' : null;
}
