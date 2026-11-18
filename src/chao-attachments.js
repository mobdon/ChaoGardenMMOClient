import { Engine, Ray, Vector3 } from '@babylonjs/core';

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
