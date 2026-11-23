import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { createSteadyHead, HEAD_LOCK_SECONDS, SKATE_HEAD_BOB_AMPLITUDE } from '../src/chao-head.js';

function rig() {
  const engine = new NullEngine(), scene = new Scene(engine);
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
  const local = node => ({ p: node.position.asArray(), q: node.rotationQuaternion.asArray(), s: node.scaling.asArray(), parent: node.parent });
  const control = createSteadyHead({ visual });
  return { engine, actor, visual, body, neck, pivot, head, eye, foot, relative, local, control };
}
const closeMatrix = (actual, expected, message) => assert.ok(actual.asArray().every((value, index) =>
  Math.abs(value - expected.asArray()[index]) < 1e-6), message);
const settle = (control, state) => { for (let frame = 0; frame < 15; frame++) control.update(1 / 60, state); };

test('idle and walk preserve every native head channel without applying a correction', () => {
  const r = rig();
  try {
    for (const gait of ['Idle', 'Walk']) for (let frame = 0; frame < 20; frame++) {
      r.body.position.y = 1 + Math.sin(frame * .3) * .25;
      r.pivot.rotationQuaternion.copyFrom(Quaternion.RotationAxis(Vector3.Right(), Math.cos(frame * .3) * .3));
      r.head.position.y = .2 + Math.sin(frame * .2) * .1;
      r.head.rotationQuaternion.copyFrom(Quaternion.RotationYawPitchRoll(.2, Math.cos(frame * .2) * .3, -.1));
      const before = r.local(r.head), matrix = r.relative(r.eye);
      r.control.update(1 / 60, { gait }); r.control.apply();
      assert.deepEqual(r.local(r.head), before);
      closeMatrix(r.relative(r.eye), matrix);
      assert.equal(r.control.diagnostics.active, false);
      assert.equal(r.control.diagnostics.mode, 'natural');
    }
  } finally { r.engine.dispose(); }
});

test('running keeps the native head motion (head bob)', () => {
  const r = rig();
  try {
    settle(r.control, { gait: 'Run' });
    assert.equal(r.control.diagnostics.active, false);
    assert.equal(r.control.diagnostics.mode, 'natural');
  } finally { r.engine.dispose(); }
});

test('skate bob follows the supplied source phase exactly with a tiny bounded vertical stroke', () => {
  const r = rig();
  try {
    const anchor = r.relative(r.head), anchorY = anchor.getTranslation().y;
    settle(r.control, { gait: 'Run', skateBlend: 1 });
    for (const phase of [0, .125, .25, .5, .75, .875, 1, 1.25, -.25]) {
      r.control.restoreBase();
      r.body.position.y = 1.2;
      r.pivot.rotationQuaternion.copyFrom(Quaternion.RotationAxis(Vector3.Right(), .3));
      r.control.update(1 / 60, { gait: 'Run', skateBlend: 1, skatePhase: phase }); r.control.apply();
      const actual = r.relative(r.head), deltaY = actual.getTranslation().y - anchorY;
      assert.ok(Math.abs(deltaY - SKATE_HEAD_BOB_AMPLITUDE * Math.sin(phase * Math.PI * 2)) < 1e-6);
      assert.ok(Math.abs(deltaY) <= SKATE_HEAD_BOB_AMPLITUDE + 1e-6);
      const expected = anchor.clone(); expected.setTranslation(anchor.getTranslation().add(new Vector3(0, deltaY, 0)));
      closeMatrix(actual, expected, 'skate stroke must not introduce head tilt or horizontal drift');
      assert.equal(r.control.diagnostics.mode, 'skate');
    }
    r.control.restoreBase();
    r.control.update(.01, { gait: 'Run', skateBlend: .25, skatePhase: .25 }); r.control.apply();
    const first = r.relative(r.head).getTranslation().y;
    assert.ok(Math.abs(first - anchorY - SKATE_HEAD_BOB_AMPLITUDE * .25) < 1e-6);
    r.control.restoreBase();
    r.control.update(.1, { gait: 'Run', skateBlend: .25, skatePhase: .25 }); r.control.apply();
    assert.ok(Math.abs(r.relative(r.head).getTranslation().y - first) < 1e-6, 'dt cannot advance a supplied skate phase');
    assert.equal(r.control.diagnostics.bobAmplitude, SKATE_HEAD_BOB_AMPLITUDE * .25);
    r.control.update(0, { mounted: true });
    r.control.update(HEAD_LOCK_SECONDS / 2, { gait: 'Run', skateBlend: 1, skatePhase: 0 }); r.control.apply();
    const transitionBaseY = r.relative(r.head).getTranslation().y;
    r.control.restoreBase();
    r.control.update(0, { gait: 'Run', skateBlend: 1, skatePhase: .25 }); r.control.apply();
    assert.ok(Math.abs(r.relative(r.head).getTranslation().y - transitionBaseY
      - SKATE_HEAD_BOB_AMPLITUDE * r.control.diagnostics.blend) < 1e-6, 'lock entry must smoothly weight the skate stroke');
  } finally { r.engine.dispose(); }
});

test('mounted pilots immediately recover the exact native pose and unmount starts a fresh blend', () => {
  const r = rig();
  try {
    r.head.position.y += .1;
    r.head.rotationQuaternion.copyFrom(Quaternion.RotationAxis(Vector3.Up(), .2));
    const before = r.local(r.head);
    settle(r.control, { gait: 'Run', skateBlend: 1, skatePhase: .25 }); r.control.apply();
    assert.notDeepEqual(r.head.position.asArray(), before.p);
    r.control.update(0, { mounted: true, gait: 'Run', skateBlend: 1 }); r.control.apply();
    assert.deepEqual(r.local(r.head), before);
    assert.equal(r.control.diagnostics.active, false); assert.equal(r.control.diagnostics.mode, 'mounted');
    r.control.update(1 / 60, { gait: 'Run', skateBlend: 1 });
    assert.ok(r.control.diagnostics.blend > 0 && r.control.diagnostics.blend < 1);
    r.control.dispose(); assert.equal(r.control.diagnostics.active, false); assert.deepEqual(r.local(r.head), before);
  } finally { r.engine.dispose(); }
});

test('skate stroke keeps its world amplitude when the actor is scaled', () => {
  for (const scale of [.6, 1, 1.7]) {
    const r = rig();
    try {
      r.actor.scaling.setAll(scale);
      settle(r.control, { gait: 'Run', skateBlend: 1 });
      r.control.apply(); r.head.computeWorldMatrix(true);
      const zero = r.head.getAbsolutePosition().clone();
      r.control.restoreBase();
      r.control.update(0, { gait: 'Run', skateBlend: 1, skatePhase: .25 }); r.control.apply();
      r.head.computeWorldMatrix(true);
      const stroke = r.head.getAbsolutePosition().subtract(zero);
      assert.ok(Math.abs(stroke.length() - SKATE_HEAD_BOB_AMPLITUDE) < 1e-6);
      assert.ok(Math.abs(stroke.x) < 1e-6 && Math.abs(stroke.z) < 1e-6);
      assert.equal(r.control.diagnostics.bobOffsetY, SKATE_HEAD_BOB_AMPLITUDE);
    } finally { r.engine.dispose(); }
  }
});

test('missing native head and nonfinite optional inputs keep a safe appearance', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const visual = new TransformNode('missing-native-rig', scene);
    const control = createSteadyHead({ visual });
    control.update(.1, { gait: 'Run' }); control.apply(); control.restoreBase();
    assert.equal(control.diagnostics.available, false); assert.equal(control.diagnostics.blend, 0);
    assert.equal(control.diagnostics.position, null); assert.equal(control.diagnostics.mode, 'unavailable'); control.dispose();
  } finally { engine.dispose(); }
  const r = rig();
  try {
    r.control.update(NaN, { gait: 'Run', skateBlend: NaN, skatePhase: Infinity }); r.control.apply();
    assert.equal(r.control.diagnostics.blend, 0); assert.equal(r.control.diagnostics.bobOffsetY, 0);
    assert.ok(r.relative(r.head).asArray().every(Number.isFinite));
  } finally { r.engine.dispose(); }
});
