import { MaterialPluginBase, Texture } from '@babylonjs/core';

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

// Ninja USE_ENV (saio useEnvironment): the game draws that strip with UVs from
// the view-space normal (GX texgen NRM * 0.5 + 0.5), giving the Chaos shine.
class NinjaEnvPlugin extends MaterialPluginBase {
  constructor(material) { super(material, 'NinjaEnv', 200, { NINJA_ENV: false }); this._enable(true); }
  prepareDefines(defines) { defines.NINJA_ENV = true; }
  getClassName() { return 'NinjaEnvPlugin'; }
  getCustomCode(type) {
    if (type !== 'fragment') return null;
    return { CUSTOM_FRAGMENT_BEFORE_LIGHTS: `
#if defined(NINJA_ENV) && defined(ALBEDO)
  vec2 ninjaEnvUV = normalize((view * vec4(normalW, 0.0)).xyz).xy * 0.5 + 0.5;
  vec3 ninjaEnv = texture2D(albedoSampler, ninjaEnvUV).rgb;
  #ifdef GAMMAALBEDO
  ninjaEnv = toLinearSpace(ninjaEnv);
  #endif
  surfaceAlbedo = ninjaEnv * vAlbedoColor.rgb;
#endif
` };
  }
}

export function applyNinjaEnvironment(material) {
  if (material?.metadata?.gltf?.extras?.ninja?.useEnvironment && !material.pluginManager?.getPlugin?.('NinjaEnv')) new NinjaEnvPlugin(material);
}
