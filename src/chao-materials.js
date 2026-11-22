import { Color3, Engine, MaterialPluginBase, Texture } from '@babylonjs/core';

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

// The face overlay materials of each character model: only these mask by texture alpha.
const FACE_MATERIALS = {
  chao: ['al_ncz.sa2mdl_3', 'al_ncz.sa2mdl_4', 'al_ncz.sa2mdl_5'],
  'neutral-chaos': ['al_nchaos.sa2mdl_10', 'al_nchaos.sa2mdl_11', 'al_nchaos.sa2mdl_12'],
  'hero-chaos': ['al_hchaos.sa2mdl_4', 'al_hchaos.sa2mdl_6', 'al_hchaos.sa2mdl_7', 'al_hchaos.sa2mdl_8'],
  'dark-chaos': ['al_dchaos.sa2mdl_4', 'al_dchaos.sa2mdl_7', 'al_dchaos.sa2mdl_8', 'al_dchaos.sa2mdl_9'],
  tails: ['al_tails.sa2mdl_2', 'al_tails.sa2mdl_4', 'al_tails.sa2mdl_5', 'al_tails.sa2mdl_6'],
  knuckles: ['al_knuckles.sa2mdl_3', 'al_knuckles.sa2mdl_5', 'al_knuckles.sa2mdl_6', 'al_knuckles.sa2mdl_7'],
  amy: ['al_amy.sa2mdl_2', 'al_amy.sa2mdl_4', 'al_amy.sa2mdl_5', 'al_amy.sa2mdl_6'],
};

/** A Chao model material as the hub draws it: unlit native texture, solid body, masked face, blended Chaos parts. */
export function applyNativeChaoMaterial(key, mat) {
  const isFamilyBody = key.startsWith('chao-body-');
  const isCharacter = isFamilyBody || Object.hasOwn(FACE_MATERIALS, key);
  mat.albedoColor = Color3.White();
  mat.metallic = 0;
  mat.roughness = 1;
  // Body and wing PNG alpha belongs to the native Chao shader, not
  // browser cutout geometry. Only actual facial overlays may mask.
  mat.unlit = true;
  mat.alpha = 1;
  // Family bodies keep the original eye and mouth overlays; only those mask.
  const facePart = isFamilyBody ? /al_eye|al_mouth/i.test(mat.albedoTexture?.name || '') : FACE_MATERIALS[key]?.includes(mat.name);
  // The game blends these Chaos textures (SRC_ALPHA/INV_SRC_ALPHA, strips flagged USE_ALPHA in
  // al_dchaos/al_hchaos/al_nchaos.sa2mdl); the GLB export marked them opaque.
  // Converted with scripts/convert-assets.py: the part's own Ninja material in glTF extras.
  const ninja = mat.metadata?.gltf?.extras?.ninja;
  const nativeBlend = ninja ? ninja.useAlpha && !facePart : /al_(dcha01|hcha00|ncha0[012])/i.test(mat.albedoTexture?.name || '');
  const texture = mat.albedoTexture;
  if (ninja && texture) {
    texture.wrapU = ninja.mirrorU ? Texture.MIRROR_ADDRESSMODE : ninja.clampU ? Texture.CLAMP_ADDRESSMODE : Texture.WRAP_ADDRESSMODE;
    texture.wrapV = ninja.mirrorV ? Texture.MIRROR_ADDRESSMODE : ninja.clampV ? Texture.CLAMP_ADDRESSMODE : Texture.WRAP_ADDRESSMODE;
  }
  const solidCharacterPart = isCharacter && !facePart && !nativeBlend;
  mat.transparencyMode = solidCharacterPart ? 0 : mat.transparencyMode === 2 || mat.transparencyMode === 3 ? 1 : mat.transparencyMode ?? 0;
  mat.alphaCutOff = 0.45;
  mat.useAlphaFromAlbedoTexture = mat.transparencyMode === 1;
  if (nativeBlend) {
    // The export marks every texture opaque; the engine only blends by a texture that reports alpha.
    if (texture) texture.hasAlpha = true;
    mat.transparencyMode = 2; mat.useAlphaFromAlbedoTexture = true;
    mat.alphaMode = Engine.ALPHA_COMBINE; mat.backFaceCulling = false;
  }
  if (solidCharacterPart) {
    mat.opacityTexture = null;
    mat.alphaMode = Engine.ALPHA_DISABLE;
    mat.disableDepthWrite = false;
    mat.backFaceCulling = false;
    mat.twoSidedLighting = true;
  }
  restoreNativeChaosSampling(key, mat);
  applyNinjaEnvironment(mat);
}
