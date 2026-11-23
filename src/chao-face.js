import { Quaternion, Texture, Vector3 } from '@babylonjs/core';

// Native Chao face presentation, ported from SA2B GC ChaoMain.rel (GSNE8P):
// AL_FaceSetEye (0xAAC0) and the AL_FACE_CTRL eyelid/blink state machine.
// The face is presentation only: the server publishes behaviour and body
// state, the client picks the face from native call sites.
//
// Local assets (scripts/convert-chao-evolution.py, never in Git): the face
// parts are glTF extras { chaoFace: 'eye' | 'eyelid' | 'mouth' } on each Chao
// body GLB, and .local/imports/chao-face.json lists the al_eye files in archive
// order plus the traced lid rotation mapping.

export const CHAO_EYES = Object.freeze({
  Normal: 0, Painful: 1, ClosedStraight: 2, ClosedHappy: 3, ClosedUp: 4, Tiny: 5, Circles: 6,
  ClosedDown: 7, Tiny2: 8, HalfClosed: 9, Mean: 10, GreenChaos: 11, BlueChaos: 12, YellowChaos: 13,
});

export const CHAO_MOUTH = Object.freeze({
  None: 0, ToothySmile: 1, Open: 2, ClosedSmile: 3, ClosedFrown: 4, OpenSmile: 5, OpenFrown: 6,
  ClosedSmile2: 7, Squiggly: 8, ToothyFrown: 9, ClosedFrown2: 10, WideOpen: 11, ClosedFrown3: 12,
});

// Native eye -> al_eye archive index (al_eye01..08, al_eye10, al_eye11, ncha, hcha, dcha).
export const EYE_TEXTURE = Object.freeze([0, 1, 2, 3, 4, 5, 6, 7, 5, 0, 0, 10, 11, 12]);
// Eyes 0/9/10 draw the body type's eye-colour texture instead: normal al_eye01,
// hero al_eye11, dark al_eye10.
const EYE_COLOUR = Object.freeze({ normal: 0, hero: 9, dark: 8 });
const BAMS = 0x10000;
const LID_OPEN = 0;
const LID_HALF = 0x4000;   // eye 9
const LID_MEAN = 13653;    // eye 10 (or eye 0 when the default eye is 10)
const SLOPE_MEAN = 3640;
const LID_SPEED = 1024;    // per frame
const SLOPE_SPEED = 512;
const BLINK_SPEED = 4096;

// fn_60_17964: mouth -> (al_mouth03 slot, al_mouth02 slot) indices into the
// 19-entry mouth texlist (data 0x97C34 = PC al_mouth.prs, al_mouth01..19).
// Mouth 0 is (al_mouth01, al_mouth01), a fully transparent 8x8 image.
export const MOUTH_TEXTURES = Object.freeze([
  [0, 0], [2, 1], [3, 0], [4, 0], [5, 0], [6, 0], [7, 0], [4, 0], [11, 10], [13, 12], [5, 0], [16, 15], [5, 0],
]);
const MOUTH_SLOT = Object.freeze({ al_mouth03: 0, al_mouth02: 1 });

export const DEFAULT_FACE = Object.freeze({ eye: CHAO_EYES.Normal, mouth: CHAO_MOUTH.None });

let manifest = null;
let manifestTask = null;
const textureCache = new Map();

export function loadChaoFaceManifest(fetcher = fetch) {
  if (manifestTask) return manifestTask;
  manifestTask = (async () => {
    try {
      manifest = await gameFaceManifest();
      if (!manifest) {
        const response = await fetcher('/api/assets/chao-face.json', { cache: 'no-store' });
        manifest = response.ok ? await response.json() : null;
      }
    } catch {
      manifest = null;
    }
    return manifest;
  })();
  return manifestTask;
}

export function normalizeFace(face = {}) {
  const valid = (value, max) => Number.isInteger(value) && value >= 0 && value <= max;
  return {
    eye: valid(face.eye, 13) ? face.eye : DEFAULT_FACE.eye,
    mouth: valid(face.mouth, 12) ? face.mouth : DEFAULT_FACE.mouth,
  };
}

// AL_FaceSetEye: lid and slope targets in BAMS. Dark Chao use default eye 10.
export function eyeLidTargets(eye, defaultEye = 0) {
  if (eye === 9) return { lid: LID_HALF, slope: 0 };
  if (eye === 10 || (eye === 0 && defaultEye === 10)) return { lid: LID_MEAN, slope: SLOPE_MEAN };
  return { lid: LID_OPEN, slope: 0 };
}

export function eyeTextureIndex(eye, eyeColour = 'normal') {
  return eye === 0 || eye === 9 || eye === 10 ? EYE_COLOUR[eyeColour] ?? 0 : EYE_TEXTURE[eye];
}

function approach(current, target, speed) {
  return Math.abs(target - current) <= speed ? target : current + Math.sign(target - current) * speed;
}

// Native blink: gap 100+400*rand (85%) or 5*rand frames; close and open at 4096/frame.
// Eyes other than 0/9/10 freeze the lids.
export function stepLids(state, random = Math.random) {
  const lids = { ...state };
  const target = eyeLidTargets(lids.eye, lids.defaultEye);
  lids.lid = approach(lids.lid, target.lid, LID_SPEED);
  lids.slope = approach(lids.slope, target.slope, SLOPE_SPEED);
  if (![0, 9, 10].includes(lids.eye)) return lids;
  if (lids.blinkPhase === 'wait') {
    lids.blinkTimer -= 1;
    if (lids.blinkTimer <= 0) lids.blinkPhase = 'close';
  } else if (lids.blinkPhase === 'close') {
    lids.blink = Math.min(LID_HALF, lids.blink + BLINK_SPEED);
    if (lids.blink >= LID_HALF) lids.blinkPhase = 'open';
  } else {
    lids.blink = Math.max(0, lids.blink - BLINK_SPEED);
    if (lids.blink <= 0) {
      lids.blinkPhase = 'wait';
      lids.blinkTimer = random() < 0.85 ? 100 + Math.floor(400 * random()) : Math.floor(5 * random());
    }
  }
  return lids;
}

// CloseAng = blink + lid - 0x4000; SlopeAng = slope (BAMS).
export function lidAngles(lids) {
  return { close: lids.blink + lids.lid - LID_HALF, slope: lids.slope };
}

function faceParts(root) {
  if (root.metadata?.chaoFaceParts) return root.metadata.chaoFaceParts;
  const parts = { eyes: [], eyelids: [], mouths: [] };
  for (const node of [root, ...root.getDescendants(false)]) {
    const kind = node.metadata?.gltf?.extras?.chaoFace;
    const meshes = node.getChildMeshes ? [node, ...node.getChildMeshes(false)].filter(mesh => mesh.material) : [];
    if (kind === 'eye') parts.eyes.push(...meshes);
    else if (kind === 'eyelid') parts.eyelids.push({ node });
    else if (kind === 'mouth') {
      const slot = MOUTH_SLOT[node.metadata?.gltf?.extras?.mouthTexture];
      if (slot !== undefined) parts.mouths.push({ slot, meshes });
    }
  }
  root.metadata = { ...(root.metadata || {}), chaoFaceParts: parts };
  return parts;
}

// The same tables from the player's own game files: the eye and mouth archives in their own order
// (the native tables index them), each picture converted in the browser. Null without the files.
async function gameFaceManifest() {
  if (typeof indexedDB === 'undefined') return null;
  const { gameArchiveNames, gameTextureUrl } = await import('./game-assets.js');
  const urls = {};
  const files = async (archive, kind) => {
    const names = await gameArchiveNames(archive);
    return names && Promise.all(names.map(async name => {
      const file = `chao-${kind}-${name.toLowerCase()}.png`;
      urls[file] = await gameTextureUrl(`${archive}/${name}`);
      return file;
    }));
  };
  const [eyeFiles, mouthFiles] = [await files('al_eye', 'eye'), await files('al_mouth', 'mouth')];
  if (!eyeFiles || !mouthFiles || Object.values(urls).includes(null)) return null;
  // The lid turn mapping is the converter's own (scripts/convert-chao-evolution.py): node-local x closes, z slopes.
  return { eyeFiles, mouthFiles, urls, eyelid: { closeAxis: 'x', closeSign: 1, slopeAxis: 'z', slopeSign: 1 } };
}

function eyeTexture(scene, index) {
  const file = manifest?.eyeFiles?.[index];
  return file ? localTexture(scene, file) : null;
}

function ownFaceMaterial(root, mesh) {
  if (!mesh.metadata?.chaoFaceOwnMaterial) {
    mesh.material = mesh.material.clone(`${mesh.material.name}-face`);
    // The eye and mouth images are cut-outs: mask by their alpha, or the clear texels draw solid.
    mesh.material.transparencyMode = 1; mesh.material.useAlphaFromAlbedoTexture = true;
    mesh.metadata = { ...(mesh.metadata || {}), chaoFaceOwnMaterial: true };
    (root.metadata.ownedMaterials ||= []).push(mesh.material);
  }
  return mesh.material;
}

function localTexture(scene, file) {
  // Per scene: the login portraits draw in scenes (and engines) of their own.
  const key = `${scene.uid}:${file}`;
  if (!textureCache.has(key)) {
    const texture = new Texture(manifest?.urls?.[file] ?? `/api/assets/${file}`, scene, true, false, Texture.NEAREST_SAMPLINGMODE);
    texture.hasAlpha = true;
    textureCache.set(key, texture);
  }
  return textureCache.get(key);
}

// al_draw_B (0x8CD4-0x8EA8): after the node's animated transform the lids get
// node 19: Rz(+SlopeAng)·Rx(CloseAng), node 22: Rz(-SlopeAng)·Rx(CloseAng),
// post-multiplied in node space. Lids are not drawn when CloseAng == -0x4000.
// The converter records which glTF node-space axes carry native X and Z for
// these nodes (manifest.eyelid); without it the lids stay hidden.
const AXES = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };

function axisRotation(axis, sign, bams) {
  return Quaternion.RotationAxis(Vector3.FromArray(AXES[axis] || AXES.x), sign * (bams / BAMS) * 2 * Math.PI);
}

function applyLids(parts, lids) {
  const mapping = manifest?.eyelid;
  const { close, slope } = lidAngles(lids);
  for (const part of parts.eyelids) {
    const { node } = part;
    const extras = node.metadata?.gltf?.extras || {};
    if (!mapping || close === -LID_HALF || !node.rotationQuaternion) { node.setEnabled(false); continue; }
    // Animation rewrites the node each frame; a paused animation leaves our last value.
    const base = part.written && node.rotationQuaternion.equals(part.written) ? part.base : node.rotationQuaternion.clone();
    const slopeSign = (extras.lidSide === 'right' ? -1 : 1) * (mapping.slopeSign ?? 1);
    const rotation = base
      .multiply(axisRotation(mapping.slopeAxis, slopeSign, slope))
      .multiply(axisRotation(mapping.closeAxis, mapping.closeSign ?? 1, close));
    node.rotationQuaternion = rotation;
    part.base = base;
    part.written = rotation.clone();
    node.setEnabled(true);
  }
}

export function setChaoFace(root, face) {
  if (!root) return false;
  const next = normalizeFace(face);
  const state = root.metadata?.chaoFace;
  const defaultEye = state?.defaultEye ?? 0;
  const eyeColour = state?.eyeColour ?? 'normal';
  const parts = faceParts(root);
  const texture = eyeTexture(root.getScene(), eyeTextureIndex(next.eye === 0 ? defaultEye : next.eye, eyeColour));
  if (texture) for (const mesh of parts.eyes) if ('albedoTexture' in mesh.material) ownFaceMaterial(root, mesh).albedoTexture = texture;
  // Without the local mouth images the parts stay hidden (the converted
  // default mouth is the transparent al_mouth01).
  const mouthFiles = manifest?.mouthFiles;
  for (const { slot, meshes } of parts.mouths) for (const mesh of meshes) {
    const file = mouthFiles?.[MOUTH_TEXTURES[next.mouth][slot]];
    const image = file && localTexture(root.getScene(), file);
    mesh.setEnabled(!!image);
    if (image) ownFaceMaterial(root, mesh).albedoTexture = image;
  }
  const lids = state?.lids || { lid: 0, slope: 0, blink: 0, blinkPhase: 'wait', blinkTimer: 100 };
  root.metadata.chaoFace = { ...next, defaultEye, eyeColour, lids: { ...lids, eye: next.eye, defaultEye } };
  applyLids(parts, root.metadata.chaoFace.lids);
  return true;
}

export function resetChaoFace(root) {
  return setChaoFace(root, DEFAULT_FACE);
}

// Call once per 60 Hz frame for a visible Chao to run the native lid motion.
export function stepChaoFace(root, random = Math.random) {
  const state = root?.metadata?.chaoFace;
  if (!state) return;
  state.lids = stepLids(state.lids, random);
  applyLids(faceParts(root), state.lids);
}

// Adult hero/dark models carry the hero (al_eye11) or dark (al_eye10) eye
// colour natively, and dark Chao force default eye 10. How AL_BODY_INFO EyeColor
// is chosen for children is not traced yet, so children keep the normal eye.
export function configureChaoFace(root, typeName = 'child') {
  const align = typeName.startsWith('hero') ? 'hero' : typeName.startsWith('dark') ? 'dark' : 'normal';
  root.metadata = { ...(root.metadata || {}), chaoFace: { ...(root.metadata?.chaoFace || {}), eyeColour: align, defaultEye: align === 'dark' ? 10 : 0 } };
}

// EyeColorTexIdList (GC ChaoMain.rel .rodata 0xC58, Steam sonic2app.exe file offset 0x4A62C0): eye colour -> al_eye
// picture. AL_FaceInit (.text 0xB104) gives types 5..19 colour (type - 5) % 3, Neutral / Hero / Dark Chaos 3 / 4 / 5,
// every other type 0; AL_FaceSetEye (0xABB4, 0xAC34) draws that picture for the plain eyes and for a Chaos Chao.
const EYE_COLOUR_PICTURE = Object.freeze([0, 9, 8, 10, 11, 12]);

/** The al_eye picture number the game puts on the open eyes of a Chao of `type`. */
export function eyePictureOfType(type) {
  const colour = type >= 5 && type <= 19 ? (type - 5) % 3 : type >= 20 && type <= 22 ? type - 17 : 0;
  return EYE_COLOUR_PICTURE[colour];
}

/**
 * A single model built from the game files: its eye nodes 18 and 21 take the game's eye picture for `type`
 * (the model's own strip names a body picture; the game swaps it, AL_MatChangeObjectTexture 0xAC54).
 * Needs loadChaoFaceManifest() first; false when the picture or the nodes are missing.
 */
export function setChaoEyePicture(root, type) {
  const texture = eyeTexture(root.getScene(), eyePictureOfType(type));
  const meshes = root.getChildMeshes(false).filter(mesh => /^0(?:18|21)_object/.test(mesh.name) && mesh.material && 'albedoTexture' in mesh.material);
  if (!texture || !meshes.length) return false;
  for (const mesh of meshes) mesh.material.albedoTexture = texture;
  return true;
}
