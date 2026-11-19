import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { NullEngine, Scene, TransformNode, Vector3, Ray, LoadAssetContainerAsync } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { NATIVE_GARDENS, isNativeTerrainMesh } from '../src/world-layout.js';
import { attachNativeGardenExit, nativeGardenExitAction } from '../src/garden-exits.js';

const reports = [];
for (const [zone, layout] of Object.entries(NATIVE_GARDENS)) {
  const bytes = await readFile(new URL(`../.local/imports/${layout.asset}.glb`, import.meta.url));
  const engine = new NullEngine(), scene = new Scene(engine);
  try {
    const container = await LoadAssetContainerAsync(new Uint8Array(bytes), scene, {
      pluginExtension: '.glb', name: layout.asset, pluginOptions: { gltf: { skipMaterials: true } },
    });
    container.addAllToScene();
    const stage = new TransformNode(zone, scene);
    stage.scaling.setAll(layout.scale); stage.position.set(...layout.position);
    for (const node of container.rootNodes) node.parent = stage;
    const meshes = new Set();
    for (const mesh of stage.getChildMeshes()) {
      if (!mesh.getTotalVertices()) continue;
      mesh.computeWorldMatrix(true);
      const box = mesh.getBoundingInfo().boundingBox, size = box.maximumWorld.subtract(box.minimumWorld);
      if (isNativeTerrainMesh(mesh.name, size.x, size.z, mesh.getTotalVertices(), zone)) meshes.add(mesh);
      else mesh.setEnabled(false);
    }
    const attached = attachNativeGardenExit(zone, meshes);
    assert.equal(attached.ready, true, `${zone}: source geometry missing or moved`);
    assert.equal(attached.meshes.length, layout.exit.meshes.length);
    const hits = [];
    for (const mesh of meshes) {
      if (!mesh.metadata?.nativeGardenExit) continue;
      const positions = mesh.getVerticesData('position'), colors = mesh.getVerticesData('color'), indices = mesh.getIndices();
      const stride = colors.length / (positions.length / 3);
      let accepted = null;
      for (let i = 0; i < indices.length; i += 3) {
        const ids = [indices[i], indices[i + 1], indices[i + 2]];
        if (!ids.every(id => Math.max(...colors.slice(id * stride, id * stride + 3)) <= .12)) continue;
        const [a, b, c] = ids.map(id => Vector3.TransformCoordinates(Vector3.FromArray(positions, id * 3), mesh.getWorldMatrix()));
        const center = a.add(b).add(c).scale(1 / 3), normal = Vector3.Cross(b.subtract(a), c.subtract(a)).normalize();
        const pick = scene.pickWithRay(new Ray(center.add(normal.scale(.1)), normal.scale(-1), .2), candidate => candidate === mesh);
        if (nativeGardenExitAction(zone, pick) === 'hub') { accepted = pick.pickedPoint.asArray(); break; }
      }
      assert.ok(accepted, `${zone}/${mesh.name}: no actual native dark-recess triangle accepts a ray click`);
      const box = mesh.getBoundingInfo().boundingBox;
      assert.equal(nativeGardenExitAction(zone, { hit: true, pickedMesh: mesh, pickedPoint: box.maximumWorld.add(new Vector3(1, 1, 1)) }), null);
      hits.push({ mesh: mesh.name, originalDarkTriangleRayHit: accepted });
    }
    reports.push({ zone, asset: `${layout.asset}.glb`, sha256: createHash('sha256').update(bytes).digest('hex'),
      transform: { scale: layout.scale, position: layout.position }, ...attached, hits });
  } finally { scene.dispose(); engine.dispose(); }
}
await mkdir(new URL('../.local/reviews/', import.meta.url), { recursive: true });
await writeFile(new URL('../.local/reviews/native-garden-exits.json', import.meta.url), JSON.stringify({
  result: 'PASS', trigger: 'Adapted native recess click; original retail trigger volumes untraced',
  limits: 'Geometry/picking proof only. Browser-to-Rust round trip is a separate check; Hero doorway is outside current authoritative terrain.',
  gardens: reports,
}, null, 2) + '\n');
console.log(JSON.stringify({ result: 'PASS', gardens: reports.map(({ zone, reachable, meshes, hits }) => ({ zone, reachable, meshes, hits })) }));
