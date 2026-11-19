import test from 'node:test';
import assert from 'node:assert/strict';
import { Engine, Matrix, Mesh, MeshBuilder, NullEngine, Quaternion, Scene, StandardMaterial, TransformNode, Vector3, VertexData } from '@babylonjs/core';
import { CHAO_GEAR_JOINTS, attachRigidGearAtRest, chestSurfacePoint, collectNativeShoeParts, mirroredShadowShoeRest, shadowShoeMountRotation, updateNativeShoeEffects } from '../src/chao-attachments.js';

test('one original rigid boot mirrors at rest and each copy follows its own native foot', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const actor = new TransformNode('actor', scene);
    actor.position.set(4, 2, -3); actor.rotationQuaternion = Quaternion.RotationYawPitchRoll(.5, -.1, .2);
    const feet = ['right', 'left'].map((side, i) => {
      const socket = new TransformNode(side, scene); socket.parent = actor;
      socket.position.set(i ? .23 : -.2, i ? .01 : 0, i ? .07 : .1);
      socket.rotationQuaternion = Quaternion.RotationYawPitchRoll(i ? -.15 : .12, .08, 0);
      socket.computeWorldMatrix(true); actor.computeWorldMatrix(true);
      return { side, socket, restToRoot: socket.getWorldMatrix().multiply(Matrix.Invert(actor.getWorldMatrix())) };
    });
    const source = { min: new Vector3(-1, -1, -2), size: new Vector3(2, 3, 5), center: new Vector3(0, .5, .5) };
    const nativeFoot = { min: new Vector3(-.3, 0, -.1), size: new Vector3(.2, .1, .3), center: new Vector3(-.2, .05, .05) };
    const rest = mirroredShadowShoeRest(source, nativeFoot);
    const roots = feet.map(fit => {
      const root = new TransformNode(`shoe-${fit.side}`, scene); root.rotationQuaternion = shadowShoeMountRotation();
      attachRigidGearAtRest(root, fit, rest[fit.side]); return root;
    });
    const toActor = Matrix.Invert(actor.getWorldMatrix());
    const point = new Vector3(.7, -.2, 1.3);
    const actual = roots.map(root => { root.computeWorldMatrix(true); return Vector3.TransformCoordinates(point, root.getWorldMatrix().multiply(toActor)); });
    assert.ok(Vector3.Distance(new Vector3(-actual[0].x, actual[0].y, actual[0].z), actual[1]) < 1e-6);
    assert.ok(roots[0].getWorldMatrix().determinant() * roots[1].getWorldMatrix().determinant() < 0, 'mirror must retain handedness, not counterrotate the original shape');
    const local = roots.map(root => root.computeWorldMatrix(true).multiply(Matrix.Invert(root.parent.getWorldMatrix())));
    feet[0].socket.rotationQuaternion = Quaternion.RotationYawPitchRoll(.12, .68, 0); feet[0].socket.position.z += .2;
    feet[1].socket.rotationQuaternion = Quaternion.RotationYawPitchRoll(-.15, .08, -.4); feet[1].socket.position.z -= .2;
    roots.forEach((root, i) => {
      root.computeWorldMatrix(true); root.parent.computeWorldMatrix(true);
      const current = root.getWorldMatrix().multiply(Matrix.Invert(root.parent.getWorldMatrix()));
      current.asArray().forEach((value, n) => assert.ok(Math.abs(value - local[i].asArray()[n]) < 1e-6));
      assert.equal(root.parent, feet[i].socket);
      const sizes = root.scaling.asArray().map(Math.abs);
      assert.ok(Math.max(...sizes) - Math.min(...sizes) < 1e-9);
    });
  } finally { engine.dispose(); }
});

test('footwear follows independently animated feet instead of their static parent pivots', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const body = new TransformNode('001_object_body', scene);
    body.rotationQuaternion = Quaternion.RotationYawPitchRoll(.4, -.2, .1);
    body.scaling.setAll(.1);
    const feet = new Map(), originals = [];
    for (const [pivotIndex, footIndex, x] of [['005', '006', .5], ['012', '013', -.5]]) {
      const pivot = new TransformNode(`${pivotIndex}_object_pivot`, scene);
      pivot.parent = body; pivot.position.set(x, -.5, 0);
      const foot = new TransformNode(`${footIndex}_object_foot`, scene);
      foot.parent = pivot; foot.rotationQuaternion = Quaternion.Identity();
      const mesh = MeshBuilder.CreateBox(`${footIndex}-geometry`, { size: .2 }, scene);
      mesh.parent = foot; mesh.position.z = .2;
      feet.set(footIndex, foot); feet.set(pivotIndex, pivot);
      originals.push(pivot, foot, mesh);
    }
    const fixed = originals.map(node => ({ node, parent: node.parent, scale: node.scaling.asArray() }));
    const shoes = CHAO_GEAR_JOINTS.map(([, , , socketIndex, meshIndex]) => {
      const socket = feet.get(socketIndex), animated = feet.get(meshIndex);
      assert.ok(socket && animated, 'mapping must address an original foot');
      animated.computeWorldMatrix(true); socket.computeWorldMatrix(true);
      const geometry = animated.getChildMeshes()[0]; geometry.computeWorldMatrix(true);
      const shoe = new TransformNode(`shoe-${socketIndex}`, scene);
      shoe.parent = socket;
      shoe.position.copyFrom(Vector3.TransformCoordinates(geometry.getBoundingInfo().boundingBox.centerWorld,
        Matrix.Invert(socket.getWorldMatrix())));
      return shoe;
    });
    const z = [[], []];
    for (let frame = 0; frame <= 60; frame++) {
      const phase = frame / 60 * Math.PI * 2;
      for (const [index, sign] of [['006', 1], ['013', -1]]) {
        const foot = feet.get(index);
        foot.position.z = sign * .2 * Math.sin(phase);
        foot.rotationQuaternion = Quaternion.RotationAxis(Vector3.Right(), sign * .3 * Math.sin(phase));
      }
      body.computeWorldMatrix(true);
      const inverseBody = Matrix.Invert(body.getWorldMatrix());
      shoes.forEach((shoe, side) => {
        shoe.computeWorldMatrix(true);
        z[side].push(Vector3.TransformCoordinates(shoe.getAbsolutePosition(), inverseBody).z);
      });
      fixed.forEach(({ node, parent, scale }) => {
        assert.equal(node.parent, parent); assert.deepEqual(node.scaling.asArray(), scale);
      });
    }
    for (const samples of z) assert.ok(Math.max(...samples) - Math.min(...samples) > .3, 'shoes must swing independently');
    for (let i = 0; i < z[0].length; i++) assert.ok(Math.abs(z[0][i] + z[1][i] - .4) < .02, 'left and right stride must alternate');
  } finally { engine.dispose(); }
});

test('chest gem uses the actual torso skin rather than the front of its bounding box', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const root = new TransformNode('body-root', scene);
    root.position.set(4, 3, -2); root.rotationQuaternion = Quaternion.RotationYawPitchRoll(.5, -.2, .1);
    root.scaling.setAll(.2);
    const torso = new Mesh('sloped-torso', scene); torso.parent = root;
    const data = new VertexData();
    // Front skin is z=x+y. The AABB front (z=3) floats ahead of the chest.
    data.positions = [-1, 0, -1, 1, 0, 1, 1, 2, 3, -1, 2, 1];
    data.indices = [0, 1, 2, 0, 2, 3]; data.applyToMesh(torso);
    root.computeWorldMatrix(true);
    const bounds = { min: new Vector3(-1, 0, -1), max: new Vector3(1, 2, 3),
      size: new Vector3(2, 2, 4), center: new Vector3(0, 1, 1) };
    const point = chestSurfacePoint([torso], Matrix.Invert(root.getWorldMatrix()), bounds);
    assert.ok(point instanceof Vector3);
    assert.ok(Math.abs(point.x) < 1e-5);
    assert.ok(Math.abs(point.y - 1.24) < 1e-5);
    assert.ok(Math.abs(point.z - 1.24) < 1e-5, 'gem center must sit on intersected skin');
    assert.equal(torso.parent, root); assert.deepEqual(root.scaling.asArray(), [.2, .2, .2]);
  } finally { engine.dispose(); }
});

test('chest surface selects the front skin and has a finite bounds fallback', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const bounds = { min: new Vector3(-1, 0, -1), max: new Vector3(1, 2, 1),
      size: new Vector3(2, 2, 2), center: new Vector3(0, 1, 0) };
    const planes = [.2, -.4].map(z => {
      const mesh = new Mesh(`skin-${z}`, scene), data = new VertexData();
      data.positions = [-1, 0, z, 1, 0, z, 1, 2, z, -1, 2, z];
      data.indices = [0, 1, 2, 0, 2, 3]; data.applyToMesh(mesh); return mesh;
    });
    const point = chestSurfacePoint(planes.reverse(), Matrix.Identity(), bounds);
    assert.ok(Math.abs(point.z - .2) < 1e-6, 'inner/back skin must not win by array order');
    assert.deepEqual(chestSurfacePoint([], Matrix.Identity(), bounds).asArray(), [0, 1.24, 1]);
  } finally { engine.dispose(); }
});

test('native shoe effects retain alpha and follow gear while staying outside boot fitting', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const foot = new TransformNode('006_object_foot', scene), gear = new TransformNode('shoe', scene);
    gear.parent = foot; gear.scaling.setAll(.03);
    const boot = MeshBuilder.CreateBox('native-boot', { size: 1 }, scene); boot.parent = gear;
    const headMaterial = new StandardMaterial('head-flame', scene);
    const head = MeshBuilder.CreatePlane('head-flame', { size: 1 }, scene); head.material = headMaterial;
    const jets = [[92, 'toe', 'narrow'], [93, 'toe', 'wide'], [94, 'heel', 'wide'], [95, 'heel', 'narrow']].map(([id, part, variant]) => {
      const mesh = MeshBuilder.CreateBox(`native-shadow-jet-${id}-left-${part}-${variant}`, { size: 8 }, scene);
      mesh.parent = gear; mesh.position.z = -3;
      mesh.material = new StandardMaterial(`original-${id}`, scene);
      return mesh;
    });
    const fixed = jets.map(mesh => ({ mesh, parent: mesh.parent, position: mesh.position.asArray(), scale: mesh.scaling.asArray() }));
    const parts = collectNativeShoeParts(gear);
    assert.deepEqual(parts.meshes, [boot], 'native flame geometry must never enlarge the fitted boot or contact bounds');
    assert.equal(parts.effects.length, 4);
    for (const { mesh } of parts.effects) {
      assert.equal(mesh.isEnabled(), false); assert.equal(mesh.hasVertexAlpha, true);
      assert.equal(mesh.material.alphaMode, Engine.ALPHA_COMBINE);
      assert.equal(mesh.material.disableDepthWrite, true); assert.equal(mesh.material.backFaceCulling, false);
      assert.equal(mesh.material.unlit, true); assert.equal(mesh.material.diffuseTexture, null);
    }
    updateNativeShoeEffects(parts.effects, .5, 1.6);
    assert.deepEqual(parts.effects.filter(effect => effect.mesh.isEnabled()).map(effect => effect.modelId), [92, 95]);
    assert.ok(parts.effects.every(effect => effect.mesh.visibility === .5));
    updateNativeShoeEffects(parts.effects, 1, 1.994);
    assert.deepEqual(parts.effects.filter(effect => effect.mesh.isEnabled()).map(effect => effect.modelId), [93, 94]);
    foot.computeWorldMatrix(true); jets[0].computeWorldMatrix(true); const before = jets[0].getAbsolutePosition().clone();
    foot.position.set(.4, .1, .3); foot.rotationQuaternion = Quaternion.RotationYawPitchRoll(.4, .3, -.2);
    foot.computeWorldMatrix(true); jets[0].computeWorldMatrix(true);
    assert.ok(Vector3.Distance(before, jets[0].getAbsolutePosition()) > .1);
    fixed.forEach(({ mesh, parent, position, scale }) => {
      assert.equal(mesh.parent, parent); assert.deepEqual(mesh.position.asArray(), position); assert.deepEqual(mesh.scaling.asArray(), scale);
    });
    updateNativeShoeEffects(parts.effects, 0, 1.994);
    assert.ok(parts.effects.every(effect => !effect.mesh.isEnabled()));
    updateNativeShoeEffects(parts.effects, NaN, 1.994);
    assert.ok(parts.effects.every(effect => !effect.mesh.isEnabled()));
    gear.dispose(); assert.ok(jets.every(mesh => mesh.isDisposed()));
    assert.equal(head.isDisposed(), false); assert.ok(scene.materials.includes(headMaterial));
  } finally { engine.dispose(); }
});

test('older shoe aliases never fabricate replacement exhaust geometry', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const root = new TransformNode('old-shoe', scene), mesh = MeshBuilder.CreateBox('native-shoe', { size: 1 }, scene);
    mesh.parent = root; const before = scene.meshes.length;
    const parts = collectNativeShoeParts(root); updateNativeShoeEffects(parts.effects, 1, 1.994);
    assert.deepEqual(parts.meshes, [mesh]); assert.deepEqual(parts.effects, []); assert.equal(scene.meshes.length, before);
  } finally { engine.dispose(); }
});
