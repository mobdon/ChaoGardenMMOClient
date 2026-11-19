import { Engine, Matrix, Quaternion, Ray, Vector3 } from '@babylonjs/core';

// 005/012 are static leg pivots. Native Walk/Run articulate their children
// 006/013; footwear must follow those actual animated feet.
// 003/010 own the visible arm/palm mesh. 004/011 are distal bind markers;
// they rotate independently during skating and must not parent the gloves.
export const CHAO_GEAR_JOINTS = [
  ['right', '003', '004', '006', '006'],
  ['left', '010', '011', '013', '013'],
];

// Compact rigid shells around the native feet; preserve uniform prop scale.
export const CHAO_SHOE_LENGTH_RATIO = .90;

// One original native right boot is used twice. Reflecting the complete rigid
// prop gives exact rest symmetry despite differences between SA2's L/R meshes.
export const CHAO_SHOE_REFERENCE_SIDE = 'right';

export function shadowShoeMountRotation() {
  // Original right heel13 up and projected heel-to-toe direction, expressed
  // in the centered GLB's coordinates. The glTF root mirrors source X.
  const up = new Vector3(.8025777, -.5964206, .0123086).normalize();
  const forward = new Vector3(-.5965178011, -.8025729883, .0065659268).normalize();
  const basis = Matrix.Identity();
  Matrix.FromXYZAxesToRef(Vector3.Cross(up, forward).normalize(), up, forward, basis);
  return Quaternion.FromRotationMatrix(basis).conjugate().normalize();
}

// sourceBounds are measured after shadowShoeMountRotation, excluding native
// jets. referenceFootBounds are the captured right foot in actor-local rest.
export function mirroredShadowShoeRest(sourceBounds, referenceFootBounds) {
  const scale = referenceFootBounds.size.z * CHAO_SHOE_LENGTH_RATIO / sourceBounds.size.z;
  const position = referenceFootBounds.center.subtract(sourceBounds.center.scale(scale));
  position.y = referenceFootBounds.min.y - sourceBounds.min.y * scale;
  const right = Matrix.Compose(new Vector3(scale, scale, scale), shadowShoeMountRotation(), position);
  return { right, left: right.multiply(Matrix.Scaling(-1, 1, 1)), scale };
}

export function attachRigidGearAtRest(root, fit, desired) {
  // Convert the desired actor-local rest placement into the untouched native
  // socket. There is no per-frame alignment: each original joint carries its
  // own boot/glove through the native animation, preserving alternating gait.
  const local = desired.multiply(Matrix.Invert(fit.restToRoot));
  local.decompose(root.scaling, root.rotationQuaternion, root.position);
  const uniform = (Math.abs(root.scaling.x) + Math.abs(root.scaling.y) + Math.abs(root.scaling.z)) / 3;
  root.scaling.set(Math.sign(root.scaling.x) * uniform, Math.sign(root.scaling.y) * uniform, Math.sign(root.scaling.z) * uniform);
  root.parent = fit.socket;
  return local;
}

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

export function nativeShoeEffect(mesh) {
  const match = /native-shadow-jet-(9[2-9])-(left|right)-(toe|heel)-(narrow|wide)/.exec(mesh.name);
  if (!match) return null;
  const [, id, side, part, variant] = match;
  return { mesh, modelId: Number(id), side, part, variant };
}

// The aliases bundle original effect geometry beside the original footwear.
// Separate them before measuring the boot, so a flame cannot inflate fitting
// or ground contact. Older aliases simply have no effect; never invent a sprite.
export function collectNativeShoeParts(root) {
  const meshes = [], effects = [];
  for (const mesh of root.getChildMeshes().filter(mesh => mesh.getTotalVertices())) {
    const effect = nativeShoeEffect(mesh);
    if (!effect) { meshes.push(mesh); continue; }
    mesh.useVertexColors = true;
    mesh.hasVertexAlpha = true;
    mesh.receiveShadows = false;
    mesh.isPickable = false;
    if (mesh.material) {
      mesh.material.unlit = true;
      mesh.material.alphaMode = Engine.ALPHA_COMBINE;
      mesh.material.transparencyMode = 2;
      mesh.material.backFaceCulling = false;
      mesh.material.disableDepthWrite = true;
    }
    mesh.setEnabled(false);
    effects.push(effect);
  }
  return { meshes, effects };
}

export function updateNativeShoeEffects(effects, blend, speed) {
  blend = Number.isFinite(blend) ? Math.max(0, Math.min(1, blend)) : 0;
  // Original narrow/wide models are selected using this game's existing skate
  // speed band. The retail effect timing/selection code has not been traced.
  const variant = speed >= 1.8 ? 'wide' : 'narrow';
  for (const effect of effects || []) {
    effect.mesh.setEnabled(blend > 0 && effect.variant === variant);
    effect.mesh.visibility = blend;
  }
}
