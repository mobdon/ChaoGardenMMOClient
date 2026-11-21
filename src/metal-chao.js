import { Texture } from '@babylonjs/core';
import { ChaoLookEnvironment, nodeIndexOf } from './chao-look.js';

// The owner's Metal Sonic Chao (owner gear slot `look`). Parts are Metal Sonic's
// own meshes (Steam metalsonicmdl.prs model 418), each converted into the frame
// of the Chao node it rides on by scripts/metal-chao/convert.py (never in Git).
export const METAL_CHAO_LOOK = 'metal-sonic-chao';
// One rigged model: Metal Sonic's own skeleton, skinned body in Chao form and his own motions
// (scripts/metal-chao/skin.py, converted locally from the player's Steam files, never in Git).
export const METAL_CHAO_RIG = 'metal-chao-rigged';
// Metal Sonic's whole body replaces the Chao's (mob: 'all metal sonic body but in chao form'): every Chao mesh hides.
export const METAL_CHAO_HIDDEN = new Set(Array.from({ length: 40 }, (_, index) => index));

let skin = null;
export function metalSkinTexture(scene) {
  skin ??= new Texture('/api/assets/metal-chao-skin.png', scene, true, false, Texture.BILINEAR_SAMPLINGMODE);
  return skin;
}

// Hide the replaced Chao nodes and draw the rest with Metal Sonic's blue metal,
// sphere-mapped like his own env-mapped strips (texture 3). null restores.
export function applyMetalSkin(root, texture) {
  for (const mesh of root.getChildMeshes(false)) {
    const index = nodeIndexOf(mesh);
    if (index === null) continue;
    mesh.isVisible = !(texture && METAL_CHAO_HIDDEN.has(index));
    const material = mesh.material;
    if (!material || !('albedoColor' in material)) continue;
    if (!texture && !material.metadata?.metalSkin) continue;
    if (!material.metadata?.metalSkin) {
      mesh.material = material.clone(`${material.name}-metal`);
      mesh.material.metadata = { ...(material.metadata || {}), metalSkin: { original: material } };
    }
    const owned = mesh.material;
    if (!texture) { mesh.material = owned.metadata.metalSkin.original; owned.dispose(); continue; }
    owned.albedoTexture = null;
    owned.albedoColor.set(1, 1, 1);
    owned.metadata.chaoLookEnvironment ??= new ChaoLookEnvironment(owned);
    owned.metadata.chaoLookEnvironment.setEnvironment(texture, false);
  }
}

// Strips flagged environment-mapped in the native model (glTF extras.env) are
// sphere-mapped by view normal, as the game draws Metal Sonic's metal.
export function metalPartMaterial(mesh) {
  const material = mesh.material;
  if (!material?.metadata?.gltf?.extras?.env || !material.albedoTexture) return;
  const environment = material.albedoTexture;
  material.albedoTexture = null;
  material.albedoColor?.set?.(1, 1, 1);
  (material.metadata.chaoLookEnvironment ??= new ChaoLookEnvironment(material)).setEnvironment(environment, false);
}

// His clip for how the Chao is moving, the way SA2 picks Metal Sonic's rows 7-10: by his speed in game
// units per frame against his physics row's jog / run / rush speeds. `native` is the rigged model's root
// extras (converter: his physics row and clip rows from the player's own sonic2app.exe); `worldPerGameUnit`
// turns game units into this world's units. Rates follow the game's motion step: a mode-3 row advances
// by its speed per frame, a mode-10 row by speed x forward speed.
export function metalClip({ speed = 0, grounded = true, moving = false }, native, worldPerGameUnit) {
  const clip = name => native.clips[name] || { mode: 3, speed: 1 };
  const gameSpeed = speed / worldPerGameUnit / 60;
  const name = !grounded ? 'fly' : !moving ? 'stand'
    : gameSpeed < native.physics.jog ? 'walk' : gameSpeed < native.physics.run ? 'jog'
    : gameSpeed < native.physics.rush ? 'run' : 'fly';
  const { mode, speed: rowSpeed } = clip(name);
  // shortcut: mode-11 rows scale by the player's +0x44 speed, which is not traced yet; walk/jog/run use
  // the forward speed like mode 10 until it is.
  return { name, rate: mode === 3 ? rowSpeed : rowSpeed * gameSpeed };
}

export function playMetalClip(rig, { name, rate }) {
  const group = rig.groups.get(name) || rig.groups.get('stand');
  if (!group) return;
  if (rig.current !== group) { rig.current?.stop(); group.start(true, rate); rig.current = group; }
  group.speedRatio = rate;
}
