import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { createSteadyHead } from '../src/chao-head.js';

test('head stays fixed relative to the actor despite torso strides, grounding and actor turns', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const make = (name, parent, xyz) => {
      const node = new TransformNode(name, scene); node.parent = parent;
      node.position.set(...xyz); node.rotationQuaternion = Quaternion.Identity(); return node;
    };
    const actor = make('actor', null, [3, 2, -4]);
    const visual = make('visual', actor, [0, .015, 0]); visual.scaling.setAll(.1);
    const importedRoot = make('gltf', visual, [0, 0, 0]); importedRoot.scaling.z = -1;
    const body = make('001_object_body', importedRoot, [0, 1, 0]);
    const neck = make('014_object_neck', body, [0, .7, 0]);
    const pivot = make('015_object_pivot', neck, [0, .1, 0]);
    const head = make('016_object_head', pivot, [0, .2, .1]);
    const eye = make('018_object_eye', head, [.1, .1, .2]);
    const foot = make('006_object_foot', body, [.3, -1, .2]);
    const relative = node => {
      node.computeWorldMatrix(true); actor.computeWorldMatrix(true);
      return node.getWorldMatrix().multiply(Matrix.Invert(actor.getWorldMatrix()));
    };
    const original = relative(head), originalEye = relative(eye);
    const control = createSteadyHead({ visual });
    for (let frame = 0; frame < 120; frame++) {
      control.restoreBase();
      actor.position.x += .01;
      actor.rotationQuaternion.copyFrom(Quaternion.RotationYawPitchRoll(frame * .01, 0, 0));
      body.position.y = 1 + Math.sin(frame * .3) * .25;
      body.rotationQuaternion.copyFrom(Quaternion.RotationYawPitchRoll(.1, Math.sin(frame * .2) * .4, .1));
      pivot.rotationQuaternion.copyFrom(Quaternion.RotationAxis(Vector3.Right(), Math.cos(frame * .3) * .3));
      visual.position.y = .015 + Math.cos(frame * .3) * .05;
      foot.rotationQuaternion.copyFrom(Quaternion.RotationAxis(Vector3.Right(), Math.sin(frame * .3)));
      head.rotationQuaternion.copyFrom(Quaternion.RotationYawPitchRoll(.2, Math.cos(frame * .2) * .3, -.1));
      const base = { p: head.position.asArray(), q: head.rotationQuaternion.asArray(), s: head.scaling.asArray() };
      const limb = relative(foot).asArray();
      control.update(1 / 60, { moving: frame < 100, fresh: frame < 110 }); control.apply();
      assert.ok(relative(head).asArray().every((value, i) => Math.abs(value - original.asArray()[i]) < 1e-6));
      assert.ok(relative(eye).asArray().every((value, i) => Math.abs(value - originalEye.asArray()[i]) < 1e-6));
      assert.deepEqual(relative(foot).asArray(), limb, 'head correction cannot change a foot pose');
      assert.deepEqual(head.scaling.asArray(), base.s); assert.equal(head.parent, pivot);
      control.restoreBase();
      assert.deepEqual(head.position.asArray(), base.p); assert.deepEqual(head.rotationQuaternion.asArray(), base.q);
    }
    control.update(0, { mounted: true }); control.apply();
    assert.equal(control.diagnostics.active, false, 'standing anchor cannot override seated pilot');
    control.update(0, { mounted: false }); control.apply();
    assert.equal(control.diagnostics.blend, 1, 'unmounted head locks immediately without a bobbing fade');
    control.dispose(); assert.equal(control.diagnostics.active, false);
  } finally { engine.dispose(); }
});

test('missing native head gracefully keeps the existing appearance', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const visual = new TransformNode('missing-native-rig', scene);
    const control = createSteadyHead({ visual });
    control.update(.1, {}); control.apply(); control.restoreBase();
    assert.deepEqual(control.diagnostics, { available: false, blend: 0, active: false, position: null });
    control.dispose();
  } finally { engine.dispose(); }
});
