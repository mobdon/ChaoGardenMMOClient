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

// His clip for how the Chao is moving: his own stand / walk / jog / run, and his top-speed pose
// (the one he flies in) when airborne or faster than a run. Thresholds match the walkers' gaits.
export function metalClip({ speed = 0, grounded = true, moving = false }) {
  if (!grounded || speed > 2.4) return { name: 'fly', rate: 1 };
  if (!moving) return { name: 'stand', rate: 1 };
  const name = speed > 1.65 ? 'run' : speed > 1.2 ? 'jog' : 'walk';
  const reference = name === 'run' ? 1.9 : name === 'jog' ? 1.5 : 1;
  return { name, rate: Math.max(0.4, Math.min(1.5, speed / reference)) };
}

export function playMetalClip(rig, { name, rate }) {
  const group = rig.groups.get(name) || rig.groups.get('stand');
  if (!group) return;
  if (rig.current !== group) { rig.current?.stop(); group.start(true, rate); rig.current = group; }
  group.speedRatio = rate;
}
