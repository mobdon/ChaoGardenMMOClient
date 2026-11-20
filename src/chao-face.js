import { Texture } from '@babylonjs/core';

// Native Chao face presentation (AL_FACE_CTRL in X-Hax SA2Structs.h).
// Eyes are texture swaps (ChaoEyes 0..13 -> original al_eye images), the mouth is
// one of the original mouth parts (ChaoMouth 1..15, 0 = none), and the eyelids
// are the original lid overlays on nodes 019/022 shown and rotated by a close
// amount. The face is presentation only: the server publishes behaviour and
// body state, the client picks the face.
//
// The converter (scripts/convert-chao-evolution.py) records the face parts in
// each Chao GLB as glTF extras { chaoFace: 'eye' | 'eyelid' | 'mouth', mouth?: n }
// and writes .local/imports/chao-face.json with the eye texture files. Assets
// stay local; this module only reads what the local asset endpoint serves.

export const CHAO_EYES = Object.freeze({
  Normal: 0, Painful: 1, ClosedStraight: 2, ClosedHappy: 3, ClosedUp: 4, Tiny: 5, Circles: 6,
  ClosedDown: 7, Tiny2: 8, HalfClosed: 9, Mean: 10, GreenChaos: 11, BlueChaos: 12, YellowChaos: 13,
});

export const CHAO_MOUTH = Object.freeze({
  None: 0, ToothySmile: 1, Open: 2, ClosedSmile: 3, ClosedFrown: 4, OpenSmile: 5, OpenFrown: 6,
  ClosedSmile2: 7, Squiggly: 8, ToothyFrown: 9, ClosedFrown2: 10, WideOpen: 11, ClosedFrown3: 12,
  StraightMoustache: 13, SquigglyMoustache1: 14, SquigglyMoustache2: 15,
});

// The open-eyed default the existing pipeline already presents.
export const DEFAULT_FACE = Object.freeze({ eye: CHAO_EYES.Normal, mouth: CHAO_MOUTH.None, eyelid: 0 });

let manifest = null;
let manifestTask = null;
const textureCache = new Map();

export function loadChaoFaceManifest(fetcher = fetch) {
  if (manifestTask) return manifestTask;
  manifestTask = (async () => {
    try {
      const response = await fetcher('/api/assets/chao-face.json', { cache: 'no-store' });
      manifest = response.ok ? await response.json() : null;
    } catch {
      manifest = null;
    }
    return manifest;
  })();
  return manifestTask;
}

function clampEnum(value, max, fallback) {
  return Number.isInteger(value) && value >= 0 && value <= max ? value : fallback;
}

export function normalizeFace(face = {}) {
  const eyelid = Number(face.eyelid);
  return {
    eye: clampEnum(face.eye, 13, DEFAULT_FACE.eye),
    mouth: clampEnum(face.mouth, 15, DEFAULT_FACE.mouth),
    eyelid: Number.isFinite(eyelid) ? Math.min(1, Math.max(0, eyelid)) : 0,
  };
}

// Collect the face parts of an instantiated Chao once and cache them on the root.
export function faceParts(root) {
  if (root.metadata?.chaoFaceParts) return root.metadata.chaoFaceParts;
  const parts = { eyes: [], eyelids: [], mouths: new Map() };
  for (const node of [root, ...root.getDescendants(false)]) {
    const extras = node.metadata?.gltf?.extras;
    if (!extras?.chaoFace) continue;
    if (extras.chaoFace === 'eye') parts.eyes.push(node);
    else if (extras.chaoFace === 'eyelid') parts.eyelids.push({ node, rest: node.rotation?.clone?.() || null });
    else if (extras.chaoFace === 'mouth' && Number.isInteger(extras.mouth)) {
      if (!parts.mouths.has(extras.mouth)) parts.mouths.set(extras.mouth, []);
      parts.mouths.get(extras.mouth).push(node);
    }
  }
  root.metadata = { ...(root.metadata || {}), chaoFaceParts: parts };
  return parts;
}

function eyeTexture(scene, eye) {
  const file = manifest?.eyes?.[eye];
  if (!file) return null;
  if (!textureCache.has(file)) {
    const texture = new Texture(`/api/assets/${file}`, scene, true, false, Texture.NEAREST_SAMPLINGMODE);
    texture.hasAlpha = true;
    textureCache.set(file, texture);
  }
  return textureCache.get(file);
}

// Lids are the original overlay meshes; closing rotates them about their own node
// by up to the native fully closed angle recorded by the converter (radians).
function applyEyelids(parts, amount) {
  const closed = manifest?.eyelidClosedAngle ?? 0;
  for (const { node, rest } of parts.eyelids) {
    node.setEnabled(amount > 0);
    if (rest && node.rotation) node.rotation.x = rest.x + closed * (1 - amount);
  }
}

export function setChaoFace(root, face) {
  if (!root) return false;
  const next = normalizeFace(face);
  const parts = faceParts(root);
  const scene = root.getScene();
  const texture = eyeTexture(scene, next.eye);
  if (texture) {
    for (const mesh of parts.eyes) if (mesh.material && 'albedoTexture' in mesh.material) {
      if (!mesh.metadata?.chaoFaceOwnMaterial) {
        mesh.material = mesh.material.clone(`${mesh.material.name}-face`);
        mesh.metadata = { ...(mesh.metadata || {}), chaoFaceOwnMaterial: true };
        (root.metadata.ownedMaterials ||= []).push(mesh.material);
      }
      mesh.material.albedoTexture = texture;
    }
  }
  for (const [mouth, meshes] of parts.mouths) for (const mesh of meshes) mesh.setEnabled(mouth === next.mouth);
  applyEyelids(parts, next.eyelid);
  root.metadata.chaoFace = next;
  return true;
}

export function resetChaoFace(root) {
  return setChaoFace(root, DEFAULT_FACE);
}
