import { readFile, mkdir, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { NullEngine, Scene, TransformNode, Vector3, Ray, LoadAssetContainerAsync } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const GRID = Object.freeze({ minX: -22, minZ: -22, maxX: 22, maxZ: 22, step: 0.5 });

/** Same downward surface rule as the browser's native garden floor query. */
export function pickTerrainHeight(scene, meshes, x, z) {
  const hits = scene.multiPickWithRay(new Ray(new Vector3(x, 100, z), Vector3.Down(), 250),
    (mesh) => meshes.has(mesh) && mesh.isEnabled()) || [];
  for (const hit of hits.sort((a, b) => a.distance - b.distance)) {
    if (!hit.pickedPoint) continue;
    const normal = hit.getNormal(true);
    if (!normal || normal.y > 0.3) return hit.pickedPoint.y;
  }
  return null;
}

export function sampleTerrain(scene, meshes, grid = GRID) {
  const width = Math.round((grid.maxX - grid.minX) / grid.step) + 1;
  const depth = Math.round((grid.maxZ - grid.minZ) / grid.step) + 1;
  const heights = [];
  for (let z = 0; z < depth; z++) {
    for (let x = 0; x < width; x++) {
      const height = pickTerrainHeight(scene, meshes, grid.minX + x * grid.step, grid.minZ + z * grid.step);
      heights.push(Number.isFinite(height) ? Math.round(height * 100000) / 100000 : null);
    }
  }
  return { minX: grid.minX, minZ: grid.minZ, step: grid.step, width, depth, heights };
}

export async function buildTerrain() {
  // The renderer owns the canonical layout. Requiring it avoids silently
  // generating navigation data for an independently centered copy of a map.
  const { NATIVE_GARDENS, isNativeTerrainMesh } = await import('../src/world-layout.js');
  const output = path.join(ROOT, '.local', 'terrain');
  await mkdir(output, { recursive: true });
  const summaries = [];
  for (const [zone, layout] of Object.entries(NATIVE_GARDENS)) {
    const assetPath = path.join(ROOT, '.local', 'imports', `${layout.asset}.glb`);
    const asset = await readFile(assetPath);
    const engine = new NullEngine();
    const scene = new Scene(engine);
    try {
      const container = await LoadAssetContainerAsync(new Uint8Array(asset), scene, {
        pluginExtension: '.glb', name: `${layout.asset}.glb`,
        pluginOptions: { gltf: { skipMaterials: true } },
      });
      container.addAllToScene();
      const stage = new TransformNode(`terrain-source-${zone}`, scene);
      stage.scaling.setAll(layout.scale);
      stage.position.set(...layout.position);
      for (const node of container.rootNodes) node.parent = stage;
      const meshes = new Set();
      const excluded = [];
      for (const mesh of stage.getChildMeshes()) {
        // glTF creates empty parent meshes for primitive groups. Disabling
        // those would also disable every real primitive below them.
        if (!mesh.getTotalVertices()) continue;
        mesh.computeWorldMatrix(true);
        const box = mesh.getBoundingInfo().boundingBox;
        const size = box.maximumWorld.subtract(box.minimumWorld);
        const keep = typeof isNativeTerrainMesh === 'function'
          ? isNativeTerrainMesh(mesh.name, size.x, size.z, mesh.getTotalVertices(), zone)
          : mesh.getTotalVertices() > 0 && size.x <= 80 && size.z <= 80
            && !layout.excludedNames.some((name) => mesh.name.includes(name));
        if (keep) {
          mesh.isPickable = true;
          meshes.add(mesh);
        } else {
          mesh.setEnabled(false);
          excluded.push(mesh.name);
        }
      }
      if (!meshes.size) throw new Error(`No collision candidates in ${zone}.`);
      const field = sampleTerrain(scene, meshes);
      const valid = field.heights.filter(Number.isFinite);
      const spawnHeight = pickTerrainHeight(scene, meshes, 0, 6);
      if (!valid.length || !Number.isFinite(spawnHeight)) {
        console.error(JSON.stringify({ zone, valid: valid.length, spawnHeight,
          meshes: [...meshes].slice(0, 8).map((mesh) => ({ name: mesh.name, enabled: mesh.isEnabled(),
            bounds: [mesh.getBoundingInfo().boundingBox.minimumWorld.asArray(), mesh.getBoundingInfo().boundingBox.maximumWorld.asArray()] })) }));
        throw new Error(`${zone} has no valid canonical spawn surface at (0,6).`);
      }
      const document = {
        ...field, zone, coordinateSpace: 'canonical-world',
        source: { asset: `${layout.asset}.glb`, sha256: createHash('sha256').update(asset).digest('hex'),
          scale: layout.scale, position: layout.position, excludedMeshes: excluded, collisionMeshCount: meshes.size },
        surfaceRule: { rayOriginY: 100, rayLength: 250, minUpNormalY: 0.3, nullIsImpassable: true },
        preferredSpawn: { x: 0, z: 6, y: spawnHeight },
      };
      const temporary = path.join(output, `${zone}.tmp.json`);
      const destination = path.join(output, `${zone}.json`);
      await writeFile(temporary, `${JSON.stringify(document)}\n`, 'utf8');
      await rename(temporary, destination);
      summaries.push({ zone, cells: field.heights.length, valid: valid.length, holes: field.heights.length - valid.length,
        minY: Math.min(...valid), maxY: Math.max(...valid), spawnHeight, collisionMeshes: meshes.size });
    } finally {
      scene.dispose();
      engine.dispose();
    }
  }
  await writeFile(path.join(output, 'report.json'), `${JSON.stringify(summaries, null, 2)}\n`, 'utf8');
  return summaries;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(await buildTerrain(), null, 2));
}
