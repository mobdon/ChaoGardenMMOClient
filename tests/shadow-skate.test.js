import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { createShadowSkate, observeSkateMotion, SkateCadence } from '../src/shadow-skate.js';

const ready = { grounded: true, footwear: true, mounted: false, swimming: false, fresh: true, available: true };

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
  assert.equal(jump.grounded, false);
  assert.equal(observeSkateMotion(next, { x: 10, z: 6, zone: 'hub', jumpOffset: 0 }, 1200, 300).valid, false);
  assert.equal(observeSkateMotion(next, { x: .2, z: 6, zone: 'hub', jumpOffset: 0 }, 1100, 300).valid, false);
  assert.equal(observeSkateMotion(next, { x: .2, z: 6, zone: 'garden', jumpOffset: 0 }, 1200, 300).valid, false);
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
      make('011', leftArm, [-.5, 0, 0], [-.1, 0, -.3]),
      make('006', make('005', body, [.4, -1, 0], [.1, .2, 0]), [0, -.1, .1], [0, .1, 0]),
      make('013', make('012', body, [-.4, -1, 0], [-.1, -.2, 0]), [0, -.1, .1], [0, -.1, 0])];
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
