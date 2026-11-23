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
  // A model built from the game files carries each strip's own flags: USE_ALPHA (bit 3) strips blend.
  const chunk = mat.metadata?.gltf?.extras?.sa2Chunk;
  const nativeBlend = chunk ? !!(chunk.stripFlags & 8) && !facePart
    : ninja ? ninja.useAlpha && !facePart : /al_(dcha01|hcha00|ncha0[012])/i.test(mat.albedoTexture?.name || '');
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
  if (chunk) applyChaoStripState(mat, chunk);
}

// How ChaoMain's own drawer puts every Chao strip on screen (fn_60_51008 .text 0x51114, fn_60_51E20 0x51E58 with main.dol
// fn_80122A44, fn_60_51CDC 0x51CF8): blended by alpha (SRC_ALPHA, INV_SRC_ALPHA), alpha of 64 or less not drawn
// (GXSetAlphaCompare GREATER 0x40), depth tested and written, culled unless the strip is DOUBLE_SIDE (bit 4).
// Alpha is texture alpha x the chunk's diffuse alpha. The USE_ALPHA flag is not asked.
function applyChaoStripState(mat, chunk) {
  if (mat.albedoTexture) mat.albedoTexture.hasAlpha = true;
  mat.useAlphaFromAlbedoTexture = !!mat.albedoTexture;
  mat.opacityTexture = null;
  mat.alpha = (chunk.material?.diffuseRgba?.[3] ?? 255) / 255;
  mat.transparencyMode = 3; // alpha test and blend
  mat.alphaCutOff = 65 / 255;
  mat.alphaMode = Engine.ALPHA_COMBINE;
  mat.forceDepthWrite = true; mat.disableDepthWrite = false;
  mat.backFaceCulling = !(chunk.stripFlags & 16);
  mat.twoSidedLighting = false;
}

/**
 * Draw order of a model built from the game files: the game draws node by node in tree order and each node's strips
 * in file order (al_draw_B .text 0x88CC), with depth written, so a see-through part hides what is drawn after it.
 */
export function orderChaoStrips(root) {
  const number = node => { for (let n = node; n; n = n.parent) { const m = /(?:^|-)(\d{3})_/.exec(n.name || ''); if (m) return Number(m[1]); } return 0; };
  const meshes = root.getChildMeshes(false).filter(mesh => mesh.material);
  meshes.map((mesh, at) => [number(mesh), at, mesh]).sort((a, b) => a[0] - b[0] || a[1] - b[1]).forEach(([, , mesh], order) => { mesh.alphaIndex = order; });
  return meshes.length;
}

// The game's light on a Chao (GC main.dol SetLight fn_80024CB8 and ChaoMain's own strip drawer fn_60_50A8C):
// one light, GX channel COLOR0 with the clamped diffuse rule and no attenuation, so each strip draws
//   texture x material diffuse x clamp(ambient + colour x max(0, N.L), 0, 1)
// with ambient = row.ambient x row.colour and colour = row.intensity x row.colour, both cut to bytes
// (fn_8011FBBC, fn_8011FD30), and L = the reverse of row.direction (the light is put 100000 units back along it).
// A strip flagged IGNORE_LIGHT (bit 0) draws texture x diffuse. The chunk's ambient and specular colours are not
// drawn: the specular channel COLOR1 reaches no TEV stage, and the Chao drawer runs one channel (GXSetNumChans(1)).

/** `row` (game-assets.js gameChaoLight) as the three shader values, in this scene's space (the game's X mirrored). */
export function ninjaLight(row) {
  const [x, y, z] = row.direction, length = Math.hypot(x, y, z);
  const byte = value => Math.max(0, Math.min(255, Math.trunc(255 * value))) / 255;
  return {
    toLight: [x / length, -y / length, -z / length],
    colour: row.colour.map(c => byte(row.intensity * c)),
    ambient: row.colour.map(c => byte(row.ambient * c)),
  };
}

class NinjaLightPlugin extends MaterialPluginBase {
  constructor(material) {
    // After the texture work (200) and before the shiny picture is added (ChaoLookEnvironment, 210).
    super(material, 'NinjaLight', 205, { NINJA_LIGHT: false });
    this.doNotSerialize = true;
    this.light = null;
    this._enable(true);
  }
  prepareDefines(defines) { defines.NINJA_LIGHT = !!this.light; }
  getClassName() { return 'NinjaLightPlugin'; }
  getUniforms() {
    return {
      ubo: [{ name: 'ninjaToLight', size: 3, type: 'vec3' }, { name: 'ninjaLightColour', size: 3, type: 'vec3' }, { name: 'ninjaAmbient', size: 3, type: 'vec3' }],
      fragment: '#ifdef NINJA_LIGHT\nuniform vec3 ninjaToLight;\nuniform vec3 ninjaLightColour;\nuniform vec3 ninjaAmbient;\n#endif',
    };
  }
  bindForSubMesh(uniformBuffer) {
    if (!this.light) return;
    uniformBuffer.updateFloat3('ninjaToLight', ...this.light.toLight);
    uniformBuffer.updateFloat3('ninjaLightColour', ...this.light.colour);
    uniformBuffer.updateFloat3('ninjaAmbient', ...this.light.ambient);
  }
  getCustomCode(type) {
    if (type !== 'fragment') return null;
    // finalColor is the finished screen colour here, so this is the game's own byte multiply.
    return { CUSTOM_FRAGMENT_BEFORE_FRAGCOLOR: `
#ifdef NINJA_LIGHT
  finalColor.rgb *= clamp(ninjaAmbient + ninjaLightColour * max(dot(normalize(normalW), ninjaToLight), 0.0), 0.0, 1.0);
#endif
` };
  }
}

/** Lights `material` with `light` (from ninjaLight) unless its strips are flagged IGNORE_LIGHT. Safe to call again. */
export function applyNinjaLight(material, light) {
  if (!material?.pluginManager || material.metadata?.gltf?.extras?.sa2Chunk?.stripFlags & 1) return false;
  const plugin = material.pluginManager.getPlugin('NinjaLight') || new NinjaLightPlugin(material);
  if (plugin.light !== light) { plugin.light = light; plugin.markAllDefinesAsDirty(); }
  return true;
}

/** The strip's own material colour for a model built from the game files (fn_60_50DDC: the chunk's diffuse). */
export function applyChunkDiffuse(material) {
  const diffuse = material?.metadata?.gltf?.extras?.sa2Chunk?.material?.diffuseRgba;
  if (!Array.isArray(diffuse)) return false;
  // The engine multiplies this colour before its screen curve; the game multiplies bytes.
  material.albedoColor = new Color3(diffuse[0] / 255, diffuse[1] / 255, diffuse[2] / 255).toLinearSpace();
  return true;
}
