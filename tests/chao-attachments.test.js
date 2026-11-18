import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, Mesh, MeshBuilder, NullEngine, Quaternion, RawTexture, Scene, StandardMaterial, TransformNode, Vector3, VertexData } from '@babylonjs/core';
import { CHAO_GEAR_JOINTS, chestSurfacePoint, createShoeExhaust } from '../src/chao-attachments.js';

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

test('heel exhaust points behind the sole, follows animated feet and disposes without destroying the head flame atlas', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const actor = new TransformNode('actor', scene), visual = new TransformNode('native-visual', scene);
    actor.position.set(4, 3, -2); actor.rotationQuaternion = Quaternion.RotationYawPitchRoll(.5, -.2, .1);
    visual.parent = actor; visual.scaling.setAll(.1);
    const material = new StandardMaterial('shared-native-flame', scene);
    const atlas = RawTexture.CreateRGBATexture(new Uint8Array([255, 255, 255, 255]), 1, 1, scene, false, false);
    material.diffuseTexture = atlas;
    let materialDisposed = false, atlasDisposed = false;
    material.onDisposeObservable.add(() => { materialDisposed = true; });
    atlas.onDisposeObservable.add(() => { atlasDisposed = true; });
    const headFlame = MeshBuilder.CreatePlane('head-flame', { size: .18 }, scene);
    headFlame.material = material; headFlame.parent = actor;
    for (const [side, , , socketIndex] of CHAO_GEAR_JOINTS) {
      const pivot = new TransformNode(`${side}-static-pivot`, scene); pivot.parent = visual;
      pivot.position.set(side === 'right' ? .5 : -.5, .3, 0);
      const foot = new TransformNode(`${socketIndex}_object_foot`, scene); foot.parent = pivot;
      foot.rotationQuaternion = Quaternion.RotationYawPitchRoll(.04, -.06, .02);
      actor.computeWorldMatrix(true); foot.computeWorldMatrix(true);
      const toRoot = Matrix.Invert(actor.getWorldMatrix());
      const bounds = { min: new Vector3(-.06, 0, -.1), max: new Vector3(.06, .06, .1),
        size: new Vector3(.12, .06, .2), center: new Vector3(0, .03, 0) };
      const fit = { socket: foot, bounds, restToRoot: foot.getWorldMatrix().multiply(toRoot) };
      const gear = new TransformNode(`${side}-air-shoe`, scene); gear.parent = foot;
      const jet = createShoeExhaust(scene, material, { root: gear, fit }), planes = jet.getChildMeshes();
      assert.equal(jet.parent, foot); assert.equal(jet.isEnabled(), false); assert.equal(planes.length, 2);
      planes.forEach(plane => {
        assert.equal(plane.material, material); assert.equal(plane.isPickable, false); assert.equal(plane.isEnabled(), false);
      });
      jet.computeWorldMatrix(true);
      const rest = jet.getWorldMatrix().multiply(toRoot), length = bounds.size.z * 1.15;
      const heel = new Vector3(0, bounds.min.y + bounds.size.y * .3, bounds.min.z);
      const direction = new Vector3(0, -.25, -1).normalize();
      assert.ok(Vector3.Distance(Vector3.TransformCoordinates(new Vector3(0, -length / 2, 0), rest), heel) < 1e-5);
      assert.ok(Vector3.Distance(Vector3.TransformCoordinates(new Vector3(0, length / 2, 0), rest), heel.add(direction.scale(length))) < 1e-5);
      const initialWorld = jet.getAbsolutePosition().clone();
      const fixedLocal = Matrix.Compose(jet.scaling, jet.rotationQuaternion, jet.position);
      const footScale = foot.scaling.asArray(), footParent = foot.parent;
      jet.setEnabled(true);
      foot.position.z = .35;
      foot.rotationQuaternion = Quaternion.RotationYawPitchRoll(.4, -.5, .2);
      foot.computeWorldMatrix(true); jet.computeWorldMatrix(true);
      const expected = fixedLocal.multiply(foot.getWorldMatrix());
      const expectedCenter = Vector3.TransformCoordinates(Vector3.Zero(), expected);
      assert.ok(Vector3.Distance(jet.getAbsolutePosition(), expectedCenter) < 1e-5);
      assert.ok(Vector3.Distance(jet.getAbsolutePosition(), initialWorld) > .02, 'exhaust must follow the foot swing');
      assert.deepEqual(foot.scaling.asArray(), footScale); assert.equal(foot.parent, footParent);
      gear.dispose();
      assert.ok(jet.isDisposed()); planes.forEach(plane => assert.ok(plane.isDisposed()));
      assert.equal(materialDisposed, false); assert.equal(atlasDisposed, false);
      assert.equal(headFlame.material, material); assert.equal(headFlame.isDisposed(), false);
      assert.ok(scene.materials.includes(material)); assert.ok(scene.textures.includes(atlas));
    }
  } finally { engine.dispose(); }
});
