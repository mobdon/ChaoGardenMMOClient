import { Texture } from '@babylonjs/core';
import { ChaoLookEnvironment, nodeIndexOf } from './chao-look.js';

// The owner's Metal Sonic Chao (owner gear slot `look`). Parts are Metal Sonic's
// own meshes (Steam metalsonicmdl.prs model 418), each converted into the frame
// of the Chao node it rides on by scripts/metal-chao/convert.py (never in Git).
export const METAL_CHAO_LOOK = 'metal-sonic-chao';
export const METAL_CHAO_PARTS = [
  { key: 'metal-chao-torso', node: '001' }, { key: 'metal-chao-head', node: '016' },
  { key: 'metal-chao-arm-right', node: '010' }, { key: 'metal-chao-arm-left', node: '003' },
  { key: 'metal-chao-leg-right', node: '013' }, { key: 'metal-chao-leg-left', node: '006' },
];
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
