import test from 'node:test';
import assert from 'node:assert/strict';
import { Animation, AnimationGroup, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { attachNativeLocomotion, attachNativeRun, captureChaoPose, chooseNativeGait, createNativeMotionState, nativeGaitRate, selectNativeMotion, smoothObservedSpeed, stepInputPace, updateNativeLocomotion } from '../src/chao-locomotion.js';

test('pace controls stay within ordinary analog input and reach walk, native run, then skate bands', () => {
  for (const [mode, expected] of [['walk', .4], ['run', .62], ['auto', 1]]) {
    let value = 0;
    for (let index = 0; index < 30; index++) { value = stepInputPace(value, mode, true, .05); assert.ok(value >= 0 && value <= 1); }
    assert.equal(value, expected);
  }
  assert.equal(stepInputPace(1, 'auto', false, .05), 0);
  assert.equal(stepInputPace(0, 'walk', true, .05, true), 1, 'mech input stays unchanged');
  const max = 1.994;
  assert.equal(chooseNativeGait(max * .4), 'Walk');
  assert.equal(chooseNativeGait(max * .62), 'Run');
  assert.ok(max * .62 < 1.3, 'Run mode must leave the skate hysteresis band');
  assert.ok(max > 1.55, 'Auto must reach skating without any speed grant');
  assert.equal(chooseNativeGait(.99, 'Run'), 'Run');
  assert.equal(chooseNativeGait(.94, 'Run'), 'Walk');
  assert.equal(nativeGaitRate('Run', 1.2), .21, 'native119 PlaySpeed is applied to60Hz reference frames');
  assert.equal(nativeGaitRate('Walk', .8), 1);
});

test('observed speed eases animation cadence continuously without changing gait thresholds', () => {
  let speed = 0;
  const rates = [];
  for (let frame = 0; frame < 120; frame++) {
    const next = smoothObservedSpeed(speed, 1.994, 1 / 60);
    assert.ok(next > speed && next <= 1.994);
    speed = next;
    rates.push(nativeGaitRate('Run', speed));
  }
  assert.ok(speed > 1.99);
  assert.ok(rates.at(-1) > rates[0]);
  const slower = smoothObservedSpeed(speed, .8, 1 / 60);
  assert.ok(slower < speed && slower > .8, 'a snapshot speed change cannot snap animation speed');
  assert.equal(chooseNativeGait(.8, 'Run'), 'Walk', 'raw observed speed still selects the native gait');
  const together = smoothObservedSpeed(0, 1.8, .1);
  const partitioned = smoothObservedSpeed(smoothObservedSpeed(0, 1.8, .05), 1.8, .05);
  assert.ok(Math.abs(together - partitioned) < 1e-12, 'equivalent render time partitions agree');
  assert.equal(smoothObservedSpeed(1, 2, 0), 1);
  assert.equal(smoothObservedSpeed(1, 2, -.1), 1);
  assert.equal(smoothObservedSpeed(1, 2, NaN), 1);
  assert.equal(smoothObservedSpeed(0, 2, Infinity), 0);
  assert.equal(smoothObservedSpeed(0, 2, 5), smoothObservedSpeed(0, 2, .1), 'long frames are bounded');
  for (const previous of [NaN, Infinity, -1, 9]) for (const target of [NaN, Infinity, -1, 9]) {
    const value = smoothObservedSpeed(previous, target, .1);
    assert.ok(Number.isFinite(value) && value >= 0 && value <= 3);
  }
});

test('native Run cloning retains target bind offsets, original parents and scale without mutating source motion', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const visual = new TransformNode('visual', scene), target = new TransformNode('owner-001_object_body', scene);
    target.parent = visual; target.position.set(.1, .9, .2); target.scaling.setAll(1.1); target.rotationQuaternion = Quaternion.RotationAxis(Vector3.Up(), .2);
    const imported = { visual, entries: { animationGroups: [] } }, pose = captureChaoPose(imported);
    const sourceNode = new TransformNode('001_object_source', scene); sourceNode.position.set(0, 1, 0); sourceNode.rotationQuaternion = Quaternion.Identity();
    const native = new AnimationGroup('alm_run', scene);
    const position = new Animation('native-position', 'position', 60, Animation.ANIMATIONTYPE_VECTOR3);
    position.setKeys([{ frame: 0, value: new Vector3(0, 1, 0) }, { frame: 15, value: new Vector3(.2, 1.2, .1) }, { frame: 30, value: new Vector3(0, 1, 0) }]);
    const rotation = new Animation('native-rotation', 'rotationQuaternion', 60, Animation.ANIMATIONTYPE_QUATERNION);
    rotation.setKeys([{ frame: 0, value: Quaternion.Identity() }, { frame: 15, value: Quaternion.RotationAxis(Vector3.Right(), .4) }, { frame: 30, value: Quaternion.Identity() }]);
    const scale = new Animation('native-scale', 'scaling', 60, Animation.ANIMATIONTYPE_VECTOR3);
    scale.setKeys([{ frame: 0, value: new Vector3(1, 1, 1) }, { frame: 30, value: new Vector3(5, 5, 5) }]);
    native.addTargetedAnimation(position, sourceNode); native.addTargetedAnimation(rotation, sourceNode); native.addTargetedAnimation(scale, sourceNode);
    target.position.set(9, 9, 9); // Async completion must use the captured bind pose.
    const run = attachNativeRun(imported, { animationGroups: [native] }, pose);
    assert.equal(run.targetedAnimations.length, 2); run.enableBlending = false;
    run.start(true, 1); run.goToFrame(15);
    assert.ok(Vector3.Distance(target.position, new Vector3(.3, 1.1, .3)) < 1e-6);
    assert.deepEqual(target.scaling.asArray(), [1.1, 1.1, 1.1]); assert.equal(target.parent, visual);
    const expected = pose.get('001').quaternion.multiply(rotation.getKeys()[1].value).normalize();
    assert.ok(Math.abs(Quaternion.Dot(target.rotationQuaternion, expected)) > 1 - 1e-6);
    assert.deepEqual(position.getKeys()[1].value.asArray(), [.2, 1.2, .1]);
    assert.equal(imported.run, run); assert.equal(imported.entries.animationGroups[0], run);
  } finally { engine.dispose(); }
});

test('native jump, apex, descent and reverse landing follow authoritative vertical motion before resuming Run', () => {
  const state = createNativeMotionState();
  const snapshot = { speed: 1.24, grounded: true, jumpOffset: 0, fresh: true };
  assert.equal(selectNativeMotion(state, .05, snapshot).mode, 'Run');
  let selected = selectNativeMotion(state, .05, { ...snapshot, grounded: false, jumpOffset: .2, verticalVelocity: 2 });
  assert.deepEqual([selected.mode, selected.nativeState, selected.frame], ['Jump', 132, 0]);
  for (let frame = 0; frame < 6; frame++) selected = selectNativeMotion(state, .1, { ...snapshot, grounded: false, jumpOffset: .4, verticalVelocity: 0 });
  assert.deepEqual([selected.nativeState, selected.frame], [133, 6], 'ascent holds the source apex instead of replaying takeoff');
  selected = selectNativeMotion(state, .05, { ...snapshot, grounded: false, jumpOffset: .35, verticalVelocity: -1 });
  assert.deepEqual([selected.mode, selected.nativeState, selected.frame], ['Fall', 135, 0]);
  for (let frame = 0; frame < 4; frame++) selected = selectNativeMotion(state, .1, { ...snapshot, grounded: false, jumpOffset: .1, verticalVelocity: -1 });
  assert.deepEqual([selected.nativeState, selected.frame], [136, 4]);
  selected = selectNativeMotion(state, .05, snapshot);
  assert.deepEqual([selected.mode, selected.nativeState, selected.frame, selected.rate], ['Land', 137, 4, -.2]);
  selected = selectNativeMotion(state, .1, snapshot); assert.ok(selected.frame < 4 && selected.frame > 0);
  for (let frame = 0; frame < 5; frame++) selected = selectNativeMotion(state, .1, snapshot);
  assert.equal(selected.mode, 'Run');
});

test('stale snapshots, swimming and mount transitions discard airborne history without a false landing', () => {
  for (const blocked of [{ fresh: false }, { mounted: true }, { swimming: true }]) {
    const state = createNativeMotionState();
    selectNativeMotion(state, .05, { grounded: false, jumpOffset: .2, fresh: true });
    const selected = selectNativeMotion(state, .05, { grounded: false, jumpOffset: .1, fresh: true, ...blocked });
    assert.equal(selected.mode, 'Idle'); assert.equal(state.wasGrounded, null);
    assert.equal(selectNativeMotion(state, .05, { grounded: true, speed: .8, jumpOffset: 0, fresh: true }).mode, 'Walk');
  }
  const state = createNativeMotionState();
  selectNativeMotion(state, .05, { grounded: true, jumpOffset: 0, fresh: true });
  selectNativeMotion(state, .05, { grounded: false, jumpOffset: .4, fresh: true });
  assert.equal(selectNativeMotion(state, .05, { grounded: false, jumpOffset: .3, fresh: true }).mode, 'Fall', 'snapshot displacement can identify descent without velocity');
});

test('native airborne sampling retains the target rig and returns to idle with compatible Run-only assets', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const visual = new TransformNode('visual', scene), target = new TransformNode('owner-001_object_body', scene);
    target.parent = visual; target.position.set(0, .7, 0); target.scaling.setAll(1.1); target.rotationQuaternion = Quaternion.Identity();
    const imported = { visual, entries: { animationGroups: [] } }, pose = captureChaoPose(imported);
    const sourceNode = new TransformNode('001_object_source', scene); sourceNode.rotationQuaternion = Quaternion.Identity();
    const groups = ['alm_run', 'alm_jump_up', 'alm_jump_down'].map(name => {
      const group = new AnimationGroup(name, scene);
      const animation = new Animation(`${name}-rotation`, 'rotationQuaternion', 60, Animation.ANIMATIONTYPE_QUATERNION);
      animation.setKeys([{ frame: 0, value: Quaternion.Identity() }, { frame: 6, value: Quaternion.RotationAxis(Vector3.Right(), .6) }]);
      group.addTargetedAnimation(animation, sourceNode); return group;
    });
    imported.idle = new AnimationGroup('Idle', scene); imported.idle.addTargetedAnimation(groups[0].targetedAnimations[0].animation.clone(), target);
    attachNativeLocomotion(imported, { animationGroups: groups }, pose);
    for (const group of imported.entries.animationGroups) group.enableBlending = false;
    const state = createNativeMotionState();
    updateNativeLocomotion(imported, state, .05, { grounded: true, speed: 0, jumpOffset: 0, fresh: true });
    assert.equal(imported.current, imported.idle);
    updateNativeLocomotion(imported, state, .05, { grounded: false, jumpOffset: .2, fresh: true });
    assert.equal(imported.current, imported.jumpUp);
    updateNativeLocomotion(imported, state, .1, { grounded: false, jumpOffset: .3, fresh: true });
    assert.ok(target.rotationQuaternion.x > 0, 'native source rotation is sampled during ascent');
    assert.equal(target.parent, visual); assert.deepEqual(target.scaling.asArray(), [1.1, 1.1, 1.1]); assert.deepEqual(target.position.asArray(), [0, .7, 0]);
    updateNativeLocomotion(imported, state, .05, { grounded: true, speed: 0, jumpOffset: 0, fresh: true, mounted: true });
    assert.equal(imported.current, imported.idle);
    const oldAsset = { visual, entries: { animationGroups: [] }, idle: imported.idle };
    attachNativeLocomotion(oldAsset, { animationGroups: [groups[0]] }, pose);
    assert.equal(oldAsset.jumpUp, null); assert.ok(oldAsset.run);
    assert.equal(updateNativeLocomotion(oldAsset, createNativeMotionState(), .05, { grounded: false, jumpOffset: .2, fresh: true }).available, false);
    assert.equal(oldAsset.current, oldAsset.idle);
  } finally { engine.dispose(); }
});

test('unknown vertical samples reset air history and a new jump can interrupt landing recovery', () => {
  for (const jumpOffset of [undefined, null, NaN, Infinity, -.1]) {
    const state = createNativeMotionState();
    selectNativeMotion(state, .05, { fresh: true, grounded: false, jumpOffset: .3, verticalVelocity: -1 });
    assert.equal(selectNativeMotion(state, .05, { fresh: true, grounded: false, jumpOffset }).mode, 'Idle');
    assert.equal(state.wasGrounded, null);
    assert.equal(selectNativeMotion(state, .05, { fresh: true, grounded: true, jumpOffset: 0, speed: .8 }).mode, 'Walk');
  }
  const state = createNativeMotionState();
  selectNativeMotion(state, .05, { fresh: true, grounded: false, jumpOffset: .1, verticalVelocity: -1 });
  assert.equal(selectNativeMotion(state, .05, { fresh: true, grounded: true, jumpOffset: 0 }).mode, 'Land');
  const nextJump = selectNativeMotion(state, .05, { fresh: true, grounded: false, jumpOffset: .15, verticalVelocity: 1 });
  assert.equal(nextJump.mode, 'Jump');
  assert.equal(nextJump.frame, 0);
  assert.equal(nextJump.nativeState, 132);
});
