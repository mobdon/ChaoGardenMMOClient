import { Quaternion, Vector3 } from '@babylonjs/core';

// Presents the server-owned Chao body (ChaoGardenMMOServer src/body.rs):
// which native AL_RootObject family to load and how far each morph target is
// applied. The weights come from the server; this module never decides growth
// or evolution.
//
// Family GLBs (scripts/convert-chao-evolution.py, local only) hold the z model
// with five glTF morph targets n, s, f, r, p and, on the armature root node, an
// extras.chaoMorph record of per-target node rest offsets in glTF node space.

const ALIGN = ['n', 'h', 'd'];
const FAMILY = ['c', 'n', 's', 'f', 'r', 'p'];
const TARGETS = ['n', 's', 'f', 'r', 'p'];

// AL_RootObject base index (multiple of 6, 0..102) -> converted family asset key.
export function familyAssetKey(baseIndex) {
  if (!Number.isInteger(baseIndex) || baseIndex < 0 || baseIndex > 102 || baseIndex % 6) return null;
  const group = baseIndex / 6;
  return `chao-body-al_${ALIGN[group % 3]}${FAMILY[Math.floor(group / 3)]}z`;
}

// Chaos and special Chao keep their dedicated single-model assets.
export function bodyAssetFor(data) {
  const body = data?.body;
  if (!body || typeof body.typeName !== 'string' || body.typeName.endsWith('-chaos')) return null;
  return familyAssetKey(body.modelBaseIndex);
}

export function morphWeights(body) {
  const weights = body?.morph?.weights;
  if (!Array.isArray(weights) || weights.length !== 5) return [0, 0, 0, 0, 0];
  return weights.map(value => (Number.isFinite(value) ? Math.max(-2, Math.min(2, value)) : 0));
}

function morphRecord(root) {
  for (const node of [root, ...root.getDescendants(false)]) {
    const record = node.metadata?.gltf?.extras?.chaoMorph;
    if (record && typeof record === 'object') return record;
  }
  return null;
}

function nodeRest(root) {
  if (root.metadata?.chaoBodyRest) return root.metadata.chaoBodyRest;
  const record = morphRecord(root);
  const rest = new Map();
  if (record?.nodeOffsets) {
    const names = new Set(Object.values(record.nodeOffsets).flatMap(offsets => Object.keys(offsets || {})));
    for (const node of root.getDescendants(false)) {
      const name = names.has(node.name) ? node.name : [...names].find(candidate => node.name.endsWith(`-${candidate}`));
      if (!name || !node.position) continue;
      rest.set(node, {
        name,
        position: node.position.clone(),
        rotation: (node.rotationQuaternion || Quaternion.Identity()).clone(),
        scaling: node.scaling.clone(),
      });
    }
  }
  root.metadata = { ...(root.metadata || {}), chaoBodyRest: rest, chaoMorphRecord: record };
  return rest;
}

// Native node offsets blend linearly with the same weights as the vertices.
// Animated channels overwrite their own properties each frame; call this after
// animation evaluation (onAfterAnimations) so rest offsets ride on top of them.
function applyNodeOffsets(root, weights, animatedOnly) {
  const record = root.metadata.chaoMorphRecord;
  if (!record?.nodeOffsets) return;
  for (const [node, rest] of root.metadata.chaoBodyRest) {
    const t = new Vector3(0, 0, 0), s = new Vector3(1, 1, 1);
    let q = Quaternion.Identity();
    TARGETS.forEach((target, i) => {
      const offset = record.nodeOffsets[target]?.[rest.name];
      const w = weights[i];
      if (!offset || !w) return;
      t.addInPlace(Vector3.FromArray(offset.t).scale(w));
      s.multiplyInPlace(new Vector3(1 + (offset.s[0] - 1) * w, 1 + (offset.s[1] - 1) * w, 1 + (offset.s[2] - 1) * w));
      q = q.multiply(Quaternion.Slerp(Quaternion.Identity(), Quaternion.FromArray(offset.q), w));
    });
    const animated = animatedOnly.get(node) || {};
    // Animated values are re-written by the animation each frame; if an animation
    // is paused the node still holds our last output, so reuse the last base.
    const last = rest.last;
    const base = {
      position: !animated.position ? rest.position : last && node.position.equals(last.position) ? last.base.position : node.position.clone(),
      rotation: !animated.rotation ? rest.rotation : last && node.rotationQuaternion?.equals(last.rotation) ? last.base.rotation : (node.rotationQuaternion || rest.rotation).clone(),
      scaling: !animated.scaling ? rest.scaling : last && node.scaling.equals(last.scaling) ? last.base.scaling : node.scaling.clone(),
    };
    node.position.copyFrom(base.position.add(t));
    node.rotationQuaternion = base.rotation.multiply(q);
    node.scaling.copyFrom(base.scaling.multiply(s));
    rest.last = { base, position: node.position.clone(), rotation: node.rotationQuaternion.clone(), scaling: node.scaling.clone() };
  }
}

function animatedChannels(root, groups = []) {
  const channels = new Map();
  for (const group of groups) for (const { target, animation } of group.targetedAnimations) {
    if (!channels.has(target)) channels.set(target, {});
    const property = animation.targetProperty;
    if (property === 'position') channels.get(target).position = true;
    if (property === 'rotationQuaternion' || property === 'rotation') channels.get(target).rotation = true;
    if (property === 'scaling') channels.get(target).scaling = true;
  }
  return channels;
}

// Apply server body weights to an instantiated family Chao. Returns a disposer.
export function attachChaoBody(root, animationGroups, getBody) {
  nodeRest(root);
  const channels = animatedChannels(root, animationGroups);
  const meshes = root.getChildMeshes(false).filter(mesh => mesh.morphTargetManager);
  const scene = root.getScene();
  const observer = scene.onAfterAnimationsObservable.add(() => {
    const weights = morphWeights(getBody());
    for (const mesh of meshes) {
      const manager = mesh.morphTargetManager;
      for (let i = 0; i < manager.numTargets; i += 1) {
        const target = manager.getTarget(i);
        const index = TARGETS.indexOf(target.name);
        target.influence = index >= 0 ? weights[index] : 0;
      }
    }
    applyNodeOffsets(root, weights, channels);
  });
  return () => scene.onAfterAnimationsObservable.remove(observer);
}
