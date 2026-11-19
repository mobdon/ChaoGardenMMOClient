import { Texture } from '@babylonjs/core';

// Verified against the local SAIO import of the three AL_RootObject models.
// Facial overlay materials are deliberately absent. Flame/body RGB remains
// native; only texture sampling is restored from its source material flags.
const CHAOS_SAMPLERS = {
  'dark-chaos': { model: 'al_dchaos', body: [0, 1, 2, 3, 5, 6], repeatU: [2, 3] },
  'hero-chaos': { model: 'al_hchaos', body: [0, 1, 2, 3, 5], repeatU: [2] },
  'neutral-chaos': { model: 'al_nchaos', body: [0, 1, 2, 3, 4, 5, 6, 8, 9], repeatU: [1] },
};

export function nativeChaosSampler(key, materialName) {
  const source = CHAOS_SAMPLERS[key];
  if (!source) return null;
  const index = source.body.find(index => materialName === `${source.model}.sa2mdl_${index}`);
  if (index === undefined) return null;
  return {
    wrapU: source.repeatU.includes(index) ? Texture.WRAP_ADDRESSMODE : Texture.CLAMP_ADDRESSMODE,
    wrapV: Texture.CLAMP_ADDRESSMODE,
    samplingMode: Texture.BILINEAR_SAMPLINGMODE,
  };
}

export function restoreNativeChaosSampling(key, material) {
  const source = nativeChaosSampler(key, material?.name), texture = material?.albedoTexture;
  if (!source || !texture) return false;
  // glTF export used nearest/repeat globally. Restore smooth gradients and
  // boundary texels at native skin UV edges. Keep original flame repetition
  // along U, while clamping every source V axis.
  texture.wrapU = source.wrapU;
  texture.wrapV = source.wrapV;
  texture.updateSamplingMode(source.samplingMode);
  return true;
}
