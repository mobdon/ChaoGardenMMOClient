import test from 'node:test';
import assert from 'node:assert/strict';
import { familyAssetKey, bodyAssetFor, morphWeights } from '../src/chao-body.js';
import { normalizeFace, DEFAULT_FACE, CHAO_EYES, CHAO_MOUTH } from '../src/chao-face.js';

test('AL_RootObject base indices map to the native family files', () => {
  assert.equal(familyAssetKey(0), 'chao-body-al_ncz');
  assert.equal(familyAssetKey(18), 'chao-body-al_nnz');
  assert.equal(familyAssetKey(42), 'chao-body-al_hsz');
  assert.equal(familyAssetKey(96), 'chao-body-al_hpz');
  assert.equal(familyAssetKey(102), 'chao-body-al_dpz');
  for (const bad of [6, 12, -6, 3, 108, 1.5, '18', null]) assert.equal(familyAssetKey(bad), null);
});

test('chaos bodies keep their dedicated assets and missing bodies fall back', () => {
  assert.equal(bodyAssetFor({ body: { typeName: 'dark-chaos', modelBaseIndex: 120 } }), null);
  assert.equal(bodyAssetFor({}), null);
  assert.equal(bodyAssetFor({ body: { typeName: 'hero-run', modelBaseIndex: 78 } }), 'chao-body-al_hrz');
});

test('morph weights are keyed by target name and bounded', () => {
  const weights = morphWeights({ morph: { targets: ['n', 'dz', 'p'], weights: [0.5, NaN, 9] } });
  assert.deepEqual([...weights], [['n', 0.5], ['dz', 0], ['p', 2]]);
  assert.equal(morphWeights({ morph: { targets: ['n'], weights: [] } }).size, 0);
  assert.equal(morphWeights({}).size, 0);
});

test('face values follow the native ChaoEyes/ChaoMouth ranges', () => {
  assert.deepEqual(normalizeFace({ eye: CHAO_EYES.ClosedHappy, mouth: CHAO_MOUTH.OpenSmile }), { eye: 3, mouth: 5 });
  assert.deepEqual(normalizeFace({ eye: 14, mouth: 13 }), { eye: 0, mouth: 0 });
  assert.deepEqual(normalizeFace(), { ...DEFAULT_FACE });
});

import { eyeLidTargets, eyeTextureIndex, stepLids, lidAngles } from '../src/chao-face.js';

test('native eyelid targets and eye textures (AL_FaceSetEye 0xAAC0)', () => {
  assert.deepEqual(eyeLidTargets(9), { lid: 0x4000, slope: 0 });
  assert.deepEqual(eyeLidTargets(10), { lid: 13653, slope: 3640 });
  assert.deepEqual(eyeLidTargets(0, 10), { lid: 13653, slope: 3640 });
  assert.deepEqual(eyeLidTargets(4), { lid: 0, slope: 0 });
  assert.equal(eyeTextureIndex(8), 5);
  assert.equal(eyeTextureIndex(13), 12);
  assert.equal(eyeTextureIndex(0, 'hero'), 9);
  assert.equal(eyeTextureIndex(10, 'dark'), 8);
});

test('native lid motion and blink timing', () => {
  let lids = { eye: 9, defaultEye: 0, lid: 0, slope: 0, blink: 0, blinkPhase: 'wait', blinkTimer: 2 };
  lids = stepLids(lids, () => 0.5);
  assert.equal(lids.lid, 1024);
  lids = stepLids(lids, () => 0.5);
  assert.equal(lids.blinkPhase, 'close');
  for (let i = 0; i < 4; i += 1) lids = stepLids(lids, () => 0.5);
  assert.equal(lids.blink, 0x4000);
  for (let i = 0; i < 4; i += 1) lids = stepLids(lids, () => 0.5);
  assert.equal(lids.blinkPhase, 'wait');
  assert.equal(lids.blinkTimer, 300);
  assert.deepEqual(lidAngles({ blink: 0, lid: 0x4000, slope: 0 }), { close: 0, slope: 0 });
  const frozen = stepLids({ eye: 4, defaultEye: 0, lid: 0, slope: 0, blink: 0, blinkPhase: 'wait', blinkTimer: 1 });
  assert.equal(frozen.blinkTimer, 1);
});

import { blendDiffuse, lookPlan, nodeIndexOf } from '../src/chao-look.js';

test('fn_60_5140C look plan: shiny, jewel, gxColorTable colour, monotone', () => {
  const table = { gxColorTable: [[255, 255, 0, 255], [255, 255, 255, 255]] };
  assert.deepEqual(lookPlan({}, table), { monotone: false, colour: null, environment: null, additive: false });
  assert.deepEqual(lookPlan({ color: 1 }, table).colour, [1, 1, 0]);
  assert.equal(lookPlan({ color: 9 }, table).colour, null);
  assert.deepEqual(lookPlan({ jewel: 3, color: 1 }, table), { monotone: false, colour: [1, 1, 1], environment: 20, additive: false });
  const shiny = lookPlan({ shiny: true, color: 2 }, table);
  assert.equal(shiny.environment, 34);
  assert.equal(shiny.additive, true);
  assert.deepEqual(shiny.colour, [1, 1, 1]);
  assert.equal(lookPlan({ monotone: true }, table).monotone, true);
  assert.equal(nodeIndexOf({ name: 'p-016_object_0001BA34', parent: null }), 16);
  assert.equal(nodeIndexOf({ name: 'mesh', parent: { name: 'x-023_object_1', parent: null } }), 23);
});

test('material diffuse blends across the morph set with server weights', () => {
  const entry = { base: [[1, 0, 0]], f: [[0, 1, 0]], p: [[0, 0, 1]] };
  assert.deepEqual(blendDiffuse(entry, ['f', 'p'], [0, 0]), [[1, 0, 0]]);
  assert.deepEqual(blendDiffuse(entry, ['f', 'p'], [0.5, 0]), [[0.5, 0.5, 0]]);
  assert.deepEqual(blendDiffuse(entry, ['f', 'p'], [0.5, 0.5]), [[0, 0.5, 0.5]]);
  assert.equal(blendDiffuse(undefined), null);
});

import { MOUTH_TEXTURES } from '../src/chao-face.js';

test('native mouth texture pairs cover ChaoMouth 0..12', () => {
  assert.equal(MOUTH_TEXTURES.length, 13);
  assert.deepEqual(MOUTH_TEXTURES[0], [0, 0]);
  assert.deepEqual(MOUTH_TEXTURES[11], [16, 15]);
  assert.ok(MOUTH_TEXTURES.flat().every(index => index >= 0 && index < 19));
});

test('the open-eye picture follows the type: hero and dark adults, and the three Chaos Chao', async () => {
  const { eyePictureOfType } = await import('../src/chao-face.js');
  assert.deepEqual([2, 5, 6, 7, 18, 19].map(eyePictureOfType), [0, 0, 9, 8, 9, 8]);
  assert.deepEqual([20, 21, 22].map(eyePictureOfType), [10, 11, 12]);
  assert.deepEqual([23, 24, 25].map(eyePictureOfType), [0, 0, 0]);
});
