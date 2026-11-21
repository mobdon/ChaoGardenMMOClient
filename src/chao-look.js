import { Color3, MaterialPluginBase, Texture } from '@babylonjs/core';

// Native Chao body look, ported from SA2B GC ChaoMain.rel (GSNE8P).
//
// al_draw_B calls fn_60_52480(Jewel, Color, Multi, NonTex) for every body node
// except the eyes (18/21), mouth (27) and nodes 37/39. fn_60_5140C draws them:
// - MultiNum != 0 (shiny): body texture x lit colour, then chao_rare00 sphere-
//   mapped by view-space normal is added weighted by its own alpha
//   (CPREV + TEXC*TEXA). Also on monotone.
// - else JewelNum > 0: the body texture is replaced by al_body[JewelNum + 17],
//   sphere-mapped, times white.
// - JewelNum == 0 and ColorNum > 0: gxColorTable[ColorNum - 1] (data 0x38E7D0)
//   replaces the material colour.
// - NonTex (monotone): no texture, lit colour only.
// Uncoloured Chao use the material diffuse, which AL_DeformObject blends across
// the morph set (server `body.morph.diffuseWeights`; per-node values from the
// converter in extras.chaoMorph.materialDiffuse).
//
// Tables and images are read from the owner's own disc into .local (never Git):
// .local/imports/chao-look.json and chao-look-*.png.

let manifest = null;
let manifestTask = null;
const textures = new Map();
const EXCLUDED = new Set([18, 21, 27, 37, 39]);

export function loadChaoLookManifest(fetcher = fetch) {
  if (manifestTask) return manifestTask;
  manifestTask = (async () => {
    try {
      const response = await fetcher('/api/assets/chao-look.json', { cache: 'no-store' });
      manifest = response.ok ? await response.json() : null;
    } catch {
      manifest = null;
    }
    return manifest;
  })();
  return manifestTask;
}

// TEXMTX9: view-space normal x scale(0.5, -0.5) + 0.5.
export class ChaoLookEnvironment extends MaterialPluginBase {
  constructor(material) {
    super(material, 'ChaoLookEnvironment', 210, { CHAO_LOOK_ENV: false, CHAO_LOOK_ADD: false }, true, false);
    this.doNotSerialize = true;
    this.texture = null;
    this.additive = false;
  }

  setEnvironment(texture, additive) {
    this.texture = texture;
    this.additive = additive;
    this._enable(!!texture);
    this.markAllDefinesAsDirty();
  }

  prepareDefines(defines) {
    defines.CHAO_LOOK_ENV = !!this.texture;
    defines.CHAO_LOOK_ADD = !!this.texture && this.additive;
  }

  getSamplers(samplers) {
    samplers.push('chaoLookSampler');
  }

  bindForSubMesh(uniformBuffer) {
    if (this.texture) uniformBuffer.setTexture('chaoLookSampler', this.texture);
  }

  getCustomCode(shaderType) {
    if (shaderType !== 'fragment') return null;
    const uv = 'normalize(vec3(view * vec4(normalW, 0.0))).xy * vec2(0.5, -0.5) + 0.5';
    return {
      CUSTOM_FRAGMENT_DEFINITIONS: '#ifdef CHAO_LOOK_ENV\nuniform sampler2D chaoLookSampler;\n#endif',
      CUSTOM_FRAGMENT_BEFORE_LIGHTS: `#if defined(CHAO_LOOK_ENV) && !defined(CHAO_LOOK_ADD)
        surfaceAlbedo *= toLinearSpace(texture2D(chaoLookSampler, ${uv}).rgb);
      #endif`,
      CUSTOM_FRAGMENT_BEFORE_FRAGCOLOR: `#ifdef CHAO_LOOK_ADD
        vec4 chaoShine = texture2D(chaoLookSampler, ${uv});
        finalColor.rgb += toLinearSpace(chaoShine.rgb) * chaoShine.a;
      #endif`,
    };
  }
}

export function nodeIndexOf(node) {
  for (let current = node; current; current = current.parent) {
    const match = /(?:^|-)(\d{3})_/.exec(current.name || '');
    if (match) return Number(match[1]);
  }
  return null;
}

// Pure decision of what fn_60_52480 draws for one body node.
export function lookPlan(look = {}, table = manifest) {
  const jewel = Number.isInteger(look.jewel) ? look.jewel : 0;
  const color = Number.isInteger(look.color) ? look.color : 0;
  const shiny = !!look.shiny;
  const plan = { monotone: !!look.monotone, colour: null, environment: null, additive: false };
  if (shiny) {
    plan.environment = 34;
    plan.additive = true;
  } else if (jewel > 0) {
    plan.environment = jewel + 17;
    plan.colour = [1, 1, 1];
  }
  if (jewel === 0 && color > 0) {
    const entry = table?.gxColorTable?.[color - 1];
    if (Array.isArray(entry)) plan.colour = entry.slice(0, 3).map(value => value / 255);
  }
  return plan;
}

// Material diffuse blended across the morph set with the server's weights.
export function blendDiffuse(entry, targets = [], weights = []) {
  if (!Array.isArray(entry?.base)) return null;
  return entry.base.map((base, slot) => base.map((value, channel) => {
    let out = value;
    targets.forEach((name, i) => {
      const target = entry[name]?.[slot];
      if (target && Number.isFinite(weights[i])) out += weights[i] * (target[channel] - value);
    });
    return out;
  }));
}

function ownMaterial(root, mesh) {
  if (!mesh.metadata?.chaoLookOwnMaterial) {
    mesh.material = mesh.material.clone(`${mesh.material.name}-look`);
    mesh.metadata = { ...(mesh.metadata || {}), chaoLookOwnMaterial: true };
    (root.metadata ||= {}).ownedMaterials ||= [];
    root.metadata.ownedMaterials.push(mesh.material);
  }
  return mesh.material;
}

function bodyTexture(scene, index) {
  const file = manifest?.bodyTextureFiles?.[index];
  if (!file) return null;
  if (!textures.has(file)) textures.set(file, new Texture(`/api/assets/${file}`, scene, true, false, Texture.BILINEAR_SAMPLINGMODE));
  return textures.get(file);
}

function morphRecord(root) {
  for (const node of [root, ...root.getDescendants(false)]) {
    const record = node.metadata?.gltf?.extras?.chaoMorph;
    if (record) return record;
  }
  return null;
}

// Primitive meshes per native node, in material-slot (plist) order.
function nodeMeshes(root) {
  if (root.metadata?.chaoLookNodes) return root.metadata.chaoLookNodes;
  const nodes = new Map();
  for (const mesh of root.getChildMeshes(false)) {
    if (!mesh.material || !('albedoColor' in mesh.material)) continue;
    const index = nodeIndexOf(mesh);
    if (index === null) continue;
    if (!nodes.has(index)) nodes.set(index, []);
    nodes.get(index).push(mesh);
  }
  root.metadata = { ...(root.metadata || {}), chaoLookNodes: nodes };
  return nodes;
}

export function applyChaoLook(root, body) {
  if (!root) return false;
  const targets = body?.morph?.targets || [];
  const weights = body?.morph?.diffuseWeights || [];
  // Re-applied on every body sync; skip when nothing the look reads changed.
  const signature = JSON.stringify([body?.look || null, targets, weights, !!manifest]);
  if (root.metadata?.chaoLookSignature === signature) return true;
  const scene = root.getScene();
  const record = morphRecord(root);
  const diffuse = record?.materialDiffuse?.nodes || {};
  const plan = lookPlan(body?.look);
  const environment = plan.environment !== null ? bodyTexture(scene, plan.environment) : null;
  for (const [index, meshes] of nodeMeshes(root)) {
    const blended = blendDiffuse(diffuse[String(index).padStart(3, '0')] || diffuse[index], targets, weights);
    const bodyNode = !EXCLUDED.has(index);
    meshes.forEach((mesh, slot) => {
      const colour = (bodyNode && plan.colour) || blended?.[slot];
      const changes = colour || (bodyNode && (plan.monotone || environment));
      // A material changed earlier is restored when the look no longer applies.
      if (!changes && !mesh.metadata?.chaoLookOwnMaterial) return;
      const material = ownMaterial(root, mesh);
      material.metadata = { ...(material.metadata || {}) };
      material.metadata.nativeAlbedoColor ??= material.albedoColor.clone();
      material.metadata.nativeAlbedo ??= material.albedoTexture;
      material.albedoColor = colour ? new Color3(colour[0], colour[1], colour[2]) : material.metadata.nativeAlbedoColor.clone();
      if (!bodyNode) return;
      // Monotone and jewel draw without the body texture.
      material.albedoTexture = plan.monotone || (environment && !plan.additive) ? null : material.metadata.nativeAlbedo;
      if (environment) material.metadata.chaoLookEnvironment ??= new ChaoLookEnvironment(material);
      material.metadata.chaoLookEnvironment?.setEnvironment(environment, plan.additive);
    });
  }
  root.metadata = { ...(root.metadata || {}), chaoLookSignature: signature };
  return true;
}
