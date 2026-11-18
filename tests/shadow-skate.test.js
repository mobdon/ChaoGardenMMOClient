import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { createShadowSkate, observeSkateMotion, SkateCadence } from '../src/shadow-skate.js';

const ready = { grounded: true, footwear: true, mounted: false, swimming: false, fresh: true, available: true };

function skateFixture() {
  const engine = new NullEngine(), scene = new Scene(engine);
  const visual = new TransformNode('visual', scene), armature = new TransformNode('armature', scene);
  armature.parent = visual;
  const nodes = [];
  const make = (index, parent) => {
    const node = new TransformNode(`${index}_object_test`, scene);
    node.parent = parent; node.rotationQuaternion = Quaternion.Identity(); nodes.push(node); return node;
  };
  const root = make('000', armature), body = make('001', root);
  make('014', body);
  make('004', make('003', body)); make('011', make('010', body));
  make('005', body); make('012', body);
  const semantics = ['body', 'head', 'leftUpperArm', 'leftHand', 'rightUpperArm', 'rightHand', 'leftFoot', 'rightFoot'];
  const rotations = Object.fromEntries(semantics.map(semantic => [semantic, [0, 0, 0, 1]]));
  const motion = { version: 1, basis: 'right-handed glTF Y-up model space', alignmentQuaternion: [0, 0, 0, 1],
    bones: semantics.map(semantic => ({ semantic, restWorld: [0, 0, 0, 1] })),
    clips: [172, 173, 174].map(nativeId => ({ nativeId, durationSeconds: 1,
      frames: [{ timeSeconds: 0, rotations: structuredClone(rotations) },
        { timeSeconds: .75, rotations: structuredClone(rotations) }] })) };
  return { engine, nodes, visual, body, motion };
}

test('skating has reachable speed hysteresis and exits for air, mechs, missing shoes and stale authority', () => {
  const cadence = new SkateCadence();
  cadence.update(.1, { ...ready, speed: 1.4 }); assert.equal(cadence.active, false);
  cadence.update(.1, { ...ready, speed: 1.6 }); assert.equal(cadence.active, true); assert.ok(cadence.blend > 0 && cadence.blend < 1);
  cadence.update(.1, { ...ready, speed: 1.6 }); assert.equal(cadence.blend, 1);
  cadence.update(.1, { ...ready, speed: 1.4 }); assert.equal(cadence.active, true);
  const slowerRate = cadence.rate;
  cadence.update(.1, { ...ready, speed: 1.994 }); assert.ok(cadence.rate > slowerRate);
  cadence.update(.05, { ...ready, speed: 1.29 }); assert.equal(cadence.active, false); assert.ok(cadence.blend > 0 && cadence.blend < 1);
  for (const blocked of [{ grounded: false }, { mounted: true }, { footwear: false }, { swimming: true }, { fresh: false }]) {
    cadence.update(.1, { ...ready, speed: 1.994 });
    cadence.update(.01, { ...ready, speed: 1.994, ...blocked });
    assert.equal(cadence.active, false); assert.equal(cadence.blend, 0);
  }
});

test('cadence measures server displacement and ignores teleports and invalid clocks', () => {
  const first = observeSkateMotion(null, { x: 0, z: 6, zone: 'hub', jumpOffset: 0 }, 1000, 100);
  assert.equal(first.valid, false);
  const next = observeSkateMotion(first, { x: .1994, z: 6, zone: 'hub', jumpOffset: 0 }, 1100, 200);
  assert.equal(next.valid, true); assert.ok(Math.abs(next.speed - 1.994) < 1e-8); assert.equal(next.grounded, true);
  const jump = observeSkateMotion(next, { x: .3988, z: 6, zone: 'hub', jumpOffset: .1 }, 1200, 300);
  assert.equal(jump.grounded, false); assert.equal(jump.jumpOffset, .1); assert.equal(jump.verticalVelocity, 1);
  const falling = observeSkateMotion(jump, { x: .5982, z: 6, zone: 'hub', jumpOffset: .05 }, 1300, 400);
  assert.equal(falling.verticalVelocity, -.5); assert.equal(falling.grounded, false);
  const landed = observeSkateMotion(falling, { x: .7976, z: 6, zone: 'hub', jumpOffset: 0 }, 1400, 500);
  assert.equal(landed.grounded, true); assert.equal(landed.verticalVelocity, -.5);
  const teleported = observeSkateMotion(jump, { x: 10, z: 6, zone: 'hub', jumpOffset: .3 }, 1300, 400);
  assert.equal(teleported.valid, false); assert.equal(teleported.speed, 0); assert.equal(teleported.verticalVelocity, 0);
  assert.equal(observeSkateMotion(next, { x: .2, z: 6, zone: 'hub', jumpOffset: 0 }, 1100, 300).valid, false);
  assert.equal(observeSkateMotion(next, { x: .2, z: 6, zone: 'garden', jumpOffset: 0 }, 1200, 300).valid, false);
  const stale = observeSkateMotion(jump, { x: .4, z: 6, zone: 'hub', jumpOffset: .2 }, 2300, 1400);
  assert.equal(stale.valid, false); assert.equal(stale.verticalVelocity, 0);
  const missingHeight = observeSkateMotion(jump, { x: .4, z: 6, zone: 'hub' }, 1300, 400);
  assert.equal(missingHeight.jumpOffset, null); assert.equal(missingHeight.grounded, false); assert.equal(missingHeight.verticalVelocity, 0);
  for (const jumpOffset of [-.1, NaN, Infinity]) {
    const unknown = observeSkateMotion(jump, { x: .4, z: 6, zone: 'hub', jumpOffset }, 1300, 400);
    assert.equal(unknown.jumpOffset, null); assert.equal(unknown.grounded, false); assert.equal(unknown.verticalVelocity, 0);
  }
});

test('invalid render deltas cannot poison source sampling or the Chao rig', () => {
  const fixture = skateFixture();
  try {
    for (const clip of fixture.motion.clips) clip.frames[1].rotations.body = Quaternion.RotationYawPitchRoll(.2, 0, 0).asArray();
    const controller = createShadowSkate({ visual: fixture.visual }, { motion: fixture.motion });
    for (const dt of [.1, .1, NaN, Infinity, undefined, -1, .1]) {
      controller.restoreBase(); controller.update(dt, { ...ready, speed: 1.8 }); controller.apply();
      assert.ok(fixture.nodes.every(node => node.rotationQuaternion.asArray().every(Number.isFinite)));
      assert.ok(Number.isFinite(controller.diagnostics.blend));
    }
    assert.equal(controller.diagnostics.blend, 1);
    assert.ok(Math.abs(fixture.body.rotationQuaternion.y) > .001, 'finite sampling must recover after an invalid delta');
    controller.dispose();
    assert.deepEqual(fixture.body.rotationQuaternion.asArray(), [0, 0, 0, 1]);
  } finally { fixture.engine.dispose(); }
});

test('incomplete or unordered source data falls back without claiming genuine skating', () => {
  const fixture = skateFixture();
  try {
    const mutations = [
      motion => { delete motion.clips[0].frames[1].rotations.leftFoot; },
      motion => { motion.clips[0].frames[1].timeSeconds = 0; },
      motion => { motion.clips[0].frames[1].timeSeconds = 1.1; },
      motion => { motion.clips[0].frames[0].timeSeconds = -.1; },
      motion => { motion.clips[0].frames[1] = null; },
      motion => { motion.clips.push(structuredClone(motion.clips[0])); },
      motion => { motion.bones[0].restWorld = [0, 0, 0, NaN]; },
      motion => { motion.bones.push(structuredClone(motion.bones[0])); },
      motion => { motion.clips[0].frames[1].rotations.body = [1e308, 1e308, 1e308, 1e308]; },
      motion => { motion.alignmentQuaternion = [0, 0, 0, 100]; },
      motion => { motion.bones[0].restWorld = [0, 0, 0, 2]; },
    ];
    for (const mutate of mutations) {
      const motion = structuredClone(fixture.motion); mutate(motion);
      const controller = createShadowSkate({ visual: fixture.visual }, { motion, fetchMotion: async () => null });
      controller.update(.1, { ...ready, speed: 1.994 }); controller.apply();
      assert.equal(controller.diagnostics.active, false);
      assert.equal(controller.diagnostics.state, 'unavailable');
      assert.deepEqual(fixture.body.rotationQuaternion.asArray(), [0, 0, 0, 1]);
      controller.dispose();
    }
  } finally { fixture.engine.dispose(); }
});

test('the genuine cycle interpolates from its last sample back to its first sample', () => {
  const fixture = skateFixture();
  try {
    for (const clip of fixture.motion.clips) clip.frames[1].rotations.body = Quaternion.RotationYawPitchRoll(.2, 0, 0).asArray();
    const controller = createShadowSkate({ visual: fixture.visual }, { motion: fixture.motion });
    for (let frame = 0; frame < 9; frame++) {
      controller.restoreBase(); controller.update(.1, { ...ready, speed: 1.8 }); controller.apply();
    }
    // At phase .9, the .75 sample is already 60% of the way back to bind.
    const expected = Quaternion.RotationYawPitchRoll(.08, 0, 0);
    assert.ok(Math.abs(Quaternion.Dot(fixture.body.rotationQuaternion, expected)) > 1 - 1e-8);
    controller.restoreBase(); controller.update(.1, { ...ready, speed: 1.8 }); controller.apply();
    assert.ok(Math.abs(Quaternion.Dot(fixture.body.rotationQuaternion, Quaternion.Identity())) > 1 - 1e-8);
    controller.dispose();
  } finally { fixture.engine.dispose(); }
});

test('model-space rotation retarget preserves every original transform position, scale and parent', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const visual = new TransformNode('visual', scene), armature = new TransformNode('armature', scene);
    armature.parent = visual; visual.rotationQuaternion = Quaternion.RotationYawPitchRoll(.4, .2, -.3); visual.scaling.setAll(.1);
    const make = (index, parent, position, rotation) => {
      const node = new TransformNode(`${index}_object_test`, scene); node.parent = parent;
      node.position.copyFromFloats(...position); node.rotationQuaternion = Quaternion.RotationYawPitchRoll(...rotation); return node;
    };
    const root = make('000', armature, [0, 0, 0], [0, 0, 0]);
    const body = make('001', root, [0, 1, 0], [.1, -.2, .3]);
    const head = make('014', body, [0, 1, 0], [.2, .1, -.2]);
    const rightArm = make('003', body, [1, .5, 0], [0, .2, .1]);
    const leftArm = make('010', body, [-1, .5, 0], [0, -.2, -.1]);
    const nodes = [body, head, rightArm, make('004', rightArm, [.5, 0, 0], [.1, 0, .3]), leftArm,
      make('011', leftArm, [-.5, 0, 0], [-.1, 0, -.3]), make('005', body, [.4, -1, 0], [.1, .2, 0]), make('012', body, [-.4, -1, 0], [-.1, -.2, 0])];
    const semantics = ['body', 'head', 'leftUpperArm', 'leftHand', 'rightUpperArm', 'rightHand', 'leftFoot', 'rightFoot'];
    const modelRotation = node => {
      node.computeWorldMatrix(true); armature.computeWorldMatrix(true);
      const q = Quaternion.Identity(); node.getWorldMatrix().multiply(Matrix.Invert(armature.getWorldMatrix())).decompose(new Vector3(), q, new Vector3()); return q.normalize();
    };
    const rest = nodes.map(node => ({ node, parent: node.parent, position: node.position.asArray(), scaling: node.scaling.asArray(), q: node.rotationQuaternion.clone(), world: modelRotation(node) }));
    const alignment = Quaternion.RotationAxis(Vector3.Up(), Math.PI / 2);
    const rotations = Object.fromEntries(semantics.map((semantic, index) => [semantic, Quaternion.RotationYawPitchRoll(.015 * index, -.012 * index, .008 * index).asArray()]));
    const motion = { version: 1, basis: 'right-handed glTF Y-up model space', alignmentQuaternion: alignment.asArray(),
      bones: semantics.map(semantic => ({ semantic, restWorld: [0, 0, 0, 1] })),
      clips: [172, 173, 174].map(nativeId => ({ nativeId, durationSeconds: .5, frames: [{ timeSeconds: 0, rotations }, { timeSeconds: .5, rotations }] })) };
    const controller = createShadowSkate({ visual }, { motion });
    controller.update(.1, { ...ready, speed: 1.8 }); controller.apply(); controller.restoreBase();
    controller.update(.1, { ...ready, speed: 1.8 }); controller.apply();
    assert.equal(controller.diagnostics.blend, 1);
    rest.forEach((entry, index) => {
      const expected = alignment.multiply(Quaternion.FromArray(rotations[semantics[index]])).multiply(alignment.conjugate()).multiply(entry.world).normalize();
      assert.ok(Math.abs(Quaternion.Dot(modelRotation(entry.node), expected)) > 1 - 1e-5, semantics[index]);
      assert.deepEqual(entry.node.position.asArray(), entry.position); assert.deepEqual(entry.node.scaling.asArray(), entry.scaling); assert.equal(entry.node.parent, entry.parent);
    });
    controller.restoreBase();
    rest.forEach(entry => assert.ok(Math.abs(Quaternion.Dot(entry.node.rotationQuaternion, entry.q)) > 1 - 1e-6));
    controller.dispose();
  } finally { engine.dispose(); }
});
