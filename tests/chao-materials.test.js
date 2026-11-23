import test from 'node:test';
import assert from 'node:assert/strict';
import { Engine, NullEngine, PBRMaterial, RawTexture, Scene, Texture } from '@babylonjs/core';
import { applyChunkDiffuse, applyNativeChaoMaterial, applyNinjaLight, ninjaLight, restoreNativeChaosSampling } from '../src/chao-materials.js';

test('native Chaos skin clamps its RGB gradient without changing pixels, tint or opacity', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const pixels = new Uint8Array([128, 0, 128, 255, 64, 0, 0, 255]);
    const original = pixels.slice();
    const material = new PBRMaterial('al_dchaos.sa2mdl_0', scene);
    material.albedoTexture = RawTexture.CreateRGBATexture(pixels, 1, 2, scene, false, false, Texture.NEAREST_SAMPLINGMODE);
    material.albedoTexture.wrapU = material.albedoTexture.wrapV = Texture.WRAP_ADDRESSMODE;
    material.transparencyMode = PBRMaterial.PBRMATERIAL_OPAQUE;
    material.alphaMode = Engine.ALPHA_DISABLE;
    const color = material.albedoColor.asArray();
    assert.equal(restoreNativeChaosSampling('dark-chaos', material), true);
    assert.equal(material.albedoTexture.wrapU, Texture.CLAMP_ADDRESSMODE);
    assert.equal(material.albedoTexture.wrapV, Texture.CLAMP_ADDRESSMODE);
    assert.equal(material.albedoTexture.samplingMode, Texture.BILINEAR_SAMPLINGMODE);
    assert.equal(material.needAlphaBlending(), false);
    assert.equal(material.needAlphaTesting(), false);
    assert.deepEqual(material.albedoColor.asArray(), color);
    assert.equal(material.alphaMode, Engine.ALPHA_DISABLE);
    assert.deepEqual(pixels, original);
  } finally { engine.dispose(); }
});

test('native flame U repetition and alpha survive; eyes and separate effects remain untouched', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const texture = () => {
      const result = RawTexture.CreateRGBATexture(new Uint8Array([255, 120, 20, 109]), 1, 1, scene, false, false, Texture.NEAREST_SAMPLINGMODE);
      result.wrapU = result.wrapV = Texture.WRAP_ADDRESSMODE;
      return result;
    };
    const flame = new PBRMaterial('al_dchaos.sa2mdl_2', scene);
    flame.albedoTexture = texture(); flame.transparencyMode = PBRMaterial.PBRMATERIAL_ALPHABLEND;
    flame.alphaMode = Engine.ALPHA_COMBINE; flame.useAlphaFromAlbedoTexture = true;
    flame.albedoTexture.hasAlpha = true;
    assert.equal(restoreNativeChaosSampling('dark-chaos', flame), true);
    assert.equal(flame.albedoTexture.wrapU, Texture.WRAP_ADDRESSMODE);
    assert.equal(flame.albedoTexture.wrapV, Texture.CLAMP_ADDRESSMODE);
    assert.equal(flame.needAlphaBlending(), true);
    assert.equal(flame.useAlphaFromAlbedoTexture, true);
    for (const [key, name] of [['dark-chaos', 'al_dchaos.sa2mdl_4'], ['dark-chaos', 'native-shadow-jet-92'], ['emerald-cyan', 'al_dchaos.sa2mdl_0'], ['chao', 'al_ncz.sa2mdl_0']]) {
      const material = new PBRMaterial(name, scene);
      material.albedoTexture = texture();
      assert.equal(restoreNativeChaosSampling(key, material), false);
      assert.equal(material.albedoTexture.wrapV, Texture.WRAP_ADDRESSMODE);
      assert.equal(material.albedoTexture.samplingMode, Texture.NEAREST_SAMPLINGMODE);
    }
  } finally { engine.dispose(); }
});

test('the game light row becomes a to-light direction with mirrored X and byte-cut colours', () => {
  const light = ninjaLight({ direction: [3, -4, 0], intensity: 0.9, ambient: 0.45, colour: [1, 0.5, 0] }); // made-up row
  assert.deepEqual(light.toLight, [0.6, 0.8, -0]);
  assert.deepEqual(light.colour, [229 / 255, 114 / 255, 0]);
  assert.deepEqual(light.ambient, [114 / 255, 57 / 255, 0]);
});

test('a strip flagged IGNORE_LIGHT stays unlit; others get one light plugin and their chunk colour', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const material = flags => {
      const result = new PBRMaterial('strip', scene);
      result.metadata = { gltf: { extras: { sa2Chunk: { stripFlags: flags, material: { diffuseRgba: [255, 0, 0, 255] } } } } };
      return result;
    };
    const light = ninjaLight({ direction: [0, -1, 0], intensity: 1, ambient: 0.5, colour: [1, 1, 1] });
    const lit = material(2), flat = material(3);
    assert.equal(applyNinjaLight(lit, light), true);
    assert.equal(applyNinjaLight(lit, light), true);
    assert.equal(lit.pluginManager.getPlugin('NinjaLight').light, light);
    assert.equal(applyNinjaLight(flat, light), false);
    assert.equal(flat.pluginManager?.getPlugin('NinjaLight') ?? null, null);
    assert.equal(applyChunkDiffuse(lit), true);
    assert.deepEqual(lit.albedoColor.asArray(), [1, 0, 0]);
    assert.equal(applyChunkDiffuse(new PBRMaterial('server', scene)), false);
  } finally { engine.dispose(); }
});

test('a strip from the game files blends, drops alpha of 64 or less, writes depth and culls unless double-sided', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const material = (flags, alpha) => {
      const result = new PBRMaterial('strip', scene);
      result.metadata = { gltf: { extras: { sa2Chunk: { stripFlags: flags, material: { diffuseRgba: [255, 255, 255, alpha] } } } } };
      applyNativeChaoMaterial('neutral-chaos', result);
      return result;
    };
    const plain = material(2, 255), fin = material(26, 128);
    for (const mat of [plain, fin]) {
      assert.equal(mat.transparencyMode, 3);
      assert.equal(mat.alphaCutOff, 65 / 255);
      assert.equal(mat.forceDepthWrite, true);
    }
    assert.equal(plain.backFaceCulling, true);
    assert.equal(fin.backFaceCulling, false);
    assert.equal(fin.alpha, 128 / 255);
  } finally { engine.dispose(); }
});
