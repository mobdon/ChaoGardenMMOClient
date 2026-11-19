import test from 'node:test';
import assert from 'node:assert/strict';
import { Engine, NullEngine, PBRMaterial, RawTexture, Scene, Texture } from '@babylonjs/core';
import { restoreNativeChaosSampling } from '../src/chao-materials.js';

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
