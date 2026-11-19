import test from 'node:test';
import assert from 'node:assert/strict';
import { Animation, AnimationGroup, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { attachNativeRun, captureChaoPose, chooseNativeGait, nativeGaitRate, smoothObservedSpeed, stepInputPace } from '../src/chao-locomotion.js';

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
