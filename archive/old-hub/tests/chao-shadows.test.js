import test from 'node:test';
import assert from 'node:assert/strict';
import { Engine, Material, Mesh, NullEngine, RawTexture, Scene, Texture, Vector3, VertexData } from '@babylonjs/core';
import { createNativeChaoShadow, createNativeShadowTerrainIndex, projectNativeShadow } from '../src/chao-shadows.js';

function triangle(scene, points, name = 'terrain') {
  const mesh = new Mesh(name, scene), data = new VertexData();
  data.positions = points.flat(); data.indices = [0, 1, 2]; data.applyToMesh(mesh);
  return mesh;
}

test('native shadow clips to real sloped triangles and retains their height/UV footprint', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const floor = triangle(scene, [[-2, -1, -2], [2, 1, -2], [0, 0, 2]]);
    const index = createNativeShadowTerrainIndex([floor]);
    const patch = projectNativeShadow(index, { center: new Vector3(0, 0, 0), width: .4, depth: .3, maxDrop: 1, maxRise: 1 });
    assert.ok(patch.indices.length);
    for (let i = 0; i < patch.positions.length; i += 3) {
      const [x, y, z] = patch.positions.slice(i, i + 3);
      assert.ok(Math.abs(y - x / 2 - .001) < 1e-7);
      assert.ok(Math.abs(x) <= .2000001 && Math.abs(z) <= .1500001);
    }
    assert.ok(patch.uvs.every(value => value >= -1e-7 && value <= 1.0000001));
  } finally { engine.dispose(); }
});

test('shadow projection leaves holes and rejects walls, no-shadow surfaces and distant floors', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const wall = triangle(scene, [[0, 0, 0], [0, 1, 0], [0, 0, 1]]);
    const hidden = triangle(scene, [[-1, 0, -1], [1, 0, -1], [0, 0, 1]]);
    hidden.metadata = { surfaceFlags: 0x8000 };
    const remote = triangle(scene, [[-1, -2, -1], [1, -2, -1], [0, -2, 1]]);
    const edge = triangle(scene, [[0, 0, 0], [1, 0, 0], [0, 0, 1]]);
    assert.equal(createNativeShadowTerrainIndex([wall, hidden]).triangleCount, 0);
    const footprint = { center: new Vector3(-.3, 0, -.3), width: .2, depth: .2 };
    assert.equal(projectNativeShadow(createNativeShadowTerrainIndex([remote, edge]), footprint).indices.length, 0);
    for (const width of [NaN, Infinity, 0, -1, 100]) assert.equal(projectNativeShadow(createNativeShadowTerrainIndex([edge]), { ...footprint, width }).indices.length, 0);
  } finally { engine.dispose(); }
});

test('terrain world transforms and spatial index exclude distant garden triangles', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const meshes = Array.from({ length: 120 }, (_, i) => {
      const mesh = triangle(scene, [[0, 0, 0], [1, 0, 0], [0, 0, 1]]);
      mesh.position.set(i * 3, 2, 0); return mesh;
    });
    const index = createNativeShadowTerrainIndex(meshes);
    const patch = projectNativeShadow(index, { center: new Vector3(.2, 2, .2), width: .3, depth: .3 });
    assert.equal(index.triangleCount, 120); assert.equal(patch.candidates, 1);
    assert.ok(patch.positions.filter((_, i) => i % 3 === 1).every(y => Math.abs(y - 2.001) < 1e-6));
  } finally { engine.dispose(); }
});

test('native-alpha material keeps depth testing and missing assets produce no substitute', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const floor = triangle(scene, [[-1, 0, -1], [1, 0, -1], [0, 0, 1]]);
    const footprint = { center: Vector3.Zero(), width: .35, depth: .25 };
    const absent = createNativeChaoShadow(scene, { terrainMeshes: [floor] });
    assert.equal(absent.ready, false); assert.equal(absent.update(footprint), false); assert.equal(absent.mesh.getTotalVertices(), 0);
    const pixels = new Uint8Array([0, 0, 0, 219]);
    const texture = RawTexture.CreateRGBATexture(pixels, 1, 1, scene, false, false);
    let textureDisposed = false;
    texture.onDisposeObservable.add(() => { textureDisposed = true; });
    const shadow = createNativeChaoShadow(scene, { texture, terrainMeshes: [floor] });
    assert.equal(shadow.update(footprint), true);
    assert.equal(shadow.mesh.material.alphaMode, Engine.ALPHA_COMBINE);
    assert.equal(shadow.mesh.material.transparencyMode, Material.MATERIAL_ALPHABLEND);
    assert.equal(shadow.mesh.material.disableDepthWrite, true);
    assert.equal(shadow.mesh.material.depthFunction, Engine.LEQUAL);
    assert.equal(shadow.mesh.isPickable, false); assert.equal(texture.wrapU, Texture.CLAMP_ADDRESSMODE);
    assert.deepEqual([...pixels], [0, 0, 0, 219]);
    shadow.setTerrainMeshes([]); assert.equal(shadow.update(footprint), false);
    shadow.dispose(); assert.equal(textureDisposed, false); absent.dispose();
  } finally { engine.dispose(); }
});

test('avatars share one static terrain index and invalid or cleared indices hide the patch', () => {
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const floor = triangle(scene, [[-1, 0, -1], [1, 0, -1], [0, 0, 1]]);
    const source = createNativeShadowTerrainIndex([floor]);
    let queries = 0;
    const terrainIndex = { triangleCount: source.triangleCount, query(...args) { queries++; return source.query(...args); } };
    const texture = RawTexture.CreateRGBATexture(new Uint8Array([0, 0, 0, 219]), 1, 1, scene, false, false);
    const first = createNativeChaoShadow(scene, { texture, terrainIndex });
    const second = createNativeChaoShadow(scene, { texture, terrainIndex });
    const footprint = { center: Vector3.Zero(), width: .35, depth: .25 };
    assert.equal(first.update(footprint), true); assert.equal(second.update(footprint), true);
    assert.equal(queries, 2);
    assert.equal(first.update(footprint), true); assert.equal(queries, 2);
    first.setTerrainIndex(null); assert.equal(first.update(footprint), false);
    first.setTerrainIndex({ query() { throw Error('Invalid index was queried'); }, triangleCount: NaN });
    assert.equal(first.update(footprint), false);
    first.setTerrainIndex(terrainIndex); assert.equal(first.update(footprint), true);
    assert.equal(queries, 3);
    assert.equal(projectNativeShadow({ query() {} }, footprint).indices.length, 0);
    first.dispose(); second.dispose();
  } finally { engine.dispose(); }
});
