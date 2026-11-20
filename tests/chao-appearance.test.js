import test from 'node:test';
import assert from 'node:assert/strict';
import { familyAssetKey, bodyAssetFor, morphWeights } from '../src/chao-body.js';
import { normalizeFace, DEFAULT_FACE, CHAO_EYES, CHAO_MOUTH } from '../src/chao-face.js';

test('AL_RootObject base indices map to the 18 native families', () => {
  assert.equal(familyAssetKey(0), 'chao-body-al_ncz');
  assert.equal(familyAssetKey(6), 'chao-body-al_hcz');
  assert.equal(familyAssetKey(12), 'chao-body-al_dcz');
  assert.equal(familyAssetKey(18), 'chao-body-al_nnz');
  assert.equal(familyAssetKey(42), 'chao-body-al_hsz');
  assert.equal(familyAssetKey(102), 'chao-body-al_dpz');
  for (const bad of [-6, 3, 108, 1.5, '18', null]) assert.equal(familyAssetKey(bad), null);
});

test('chaos bodies keep their dedicated assets and missing bodies fall back', () => {
  assert.equal(bodyAssetFor({ body: { typeName: 'dark-chaos', modelBaseIndex: 120 } }), null);
  assert.equal(bodyAssetFor({}), null);
  assert.equal(bodyAssetFor({ body: { typeName: 'hero-run', modelBaseIndex: 78 } }), 'chao-body-al_hrz');
});

test('morph weights come from the server and are bounded', () => {
  assert.deepEqual(morphWeights({ morph: { weights: [0.5, 0, 1, 0, 0] } }), [0.5, 0, 1, 0, 0]);
  assert.deepEqual(morphWeights({ morph: { weights: [NaN, 9, -9, 0, 0] } }), [0, 2, -2, 0, 0]);
  assert.deepEqual(morphWeights({}), [0, 0, 0, 0, 0]);
});

test('face values follow the native ChaoEyes/ChaoMouth ranges', () => {
  assert.deepEqual(normalizeFace({ eye: CHAO_EYES.ClosedHappy, mouth: CHAO_MOUTH.OpenSmile, eyelid: 0.4 }), { eye: 3, mouth: 5, eyelid: 0.4 });
  assert.deepEqual(normalizeFace({ eye: 14, mouth: 16, eyelid: 3 }), { eye: 0, mouth: 0, eyelid: 1 });
  assert.deepEqual(normalizeFace(), { ...DEFAULT_FACE });
});
