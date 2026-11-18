import { Matrix, MeshBuilder, Quaternion, Ray, TransformNode, Vector3 } from '@babylonjs/core';

// 005/012 are static leg pivots. Native Walk/Run articulate their children
// 006/013; footwear must follow those actual animated feet.
export const CHAO_GEAR_JOINTS = [
  ['right', '003', '004', '006', '006'],
  ['left', '010', '011', '013', '013'],
];

// Capture the skin surface in the bind pose, before animation/async loading.
// Centering the emerald here embeds half its depth instead of floating it
// beyond the torso's maximum bounds (which include its wider lower belly).
export function chestSurfacePoint(meshes, toRoot, bounds, heightFraction = .62) {
  const point = new Vector3(bounds.center.x, bounds.min.y + bounds.size.y * heightFraction, bounds.max.z);
  const origin = point.add(new Vector3(0, 0, Math.max(1, bounds.size.z)));
  const ray = new Ray(origin, new Vector3(0, 0, -1));
  let nearest = Infinity;
  for (const mesh of meshes) {
    const positions = mesh.getVerticesData('position'), indices = mesh.getIndices();
    if (!positions || !indices) continue;
    mesh.computeWorldMatrix(true);
    const transform = mesh.getWorldMatrix().multiply(toRoot);
    const vertex = index => Vector3.TransformCoordinates(Vector3.FromArray(positions, index * 3), transform);
    for (let i = 0; i < indices.length; i += 3) {
      const hit = ray.intersectsTriangle(vertex(indices[i]), vertex(indices[i + 1]), vertex(indices[i + 2]));
      if (hit && hit.distance >= 0) nearest = Math.min(nearest, hit.distance);
    }
  }
  if (Number.isFinite(nearest)) point.z = origin.z - nearest;
  return point;
}

export function createShoeExhaust(scene, material, { root, fit }) {
  const jet = new TransformNode(`${root.name}-heel-exhaust`, scene);
  const length = fit.bounds.size.z * 1.15;
  const direction = new Vector3(0, -.25, -1).normalize();
  const heel = new Vector3(fit.bounds.center.x, fit.bounds.min.y + fit.bounds.size.y * .3, fit.bounds.min.z);
  const rotation = Quaternion.RotationAxis(Vector3.Right(), -Math.PI / 2 - Math.atan(.25));
  const desired = Matrix.Compose(Vector3.One(), rotation, heel.add(direction.scale(length / 2)));
  jet.rotationQuaternion = Quaternion.Identity();
  desired.multiply(Matrix.Invert(fit.restToRoot)).decompose(jet.scaling, jet.rotationQuaternion, jet.position);
  jet.parent = fit.socket;
  // Original Chao flame atlas adapted as a heel jet, not an extracted
  // Shadow exhaust mesh. Crossed planes retain volume as the foot swings.
  for (let i = 0; i < 2; i++) {
    const plane = MeshBuilder.CreatePlane(`${jet.name}-${i}`, { width: fit.bounds.size.x * .6, height: length }, scene);
    plane.parent = jet; plane.rotation.y = i * Math.PI / 2;
    plane.material = material; plane.isPickable = false;
  }
  jet.setEnabled(false);
  // The socket owns animated placement; the gear owns its lifecycle. Shared
  // material/atlas lifetime belongs to the scene, including the head flame.
  root.onDisposeObservable.add(() => jet.dispose());
  return jet;
}
