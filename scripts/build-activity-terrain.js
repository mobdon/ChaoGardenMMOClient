import { readFile, mkdir, writeFile, rename } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MODELS = ['race-normal', 'race-dark', 'race-hero', 'karate-arena'];

// Collision vertices already use native coordinates ×0.1. Do not scale/recenter
// them again or derive navigation surfaces from the visible skybox/decoration.
export function rasterizeActivityCollision(document, { step = 0.2, minUpNormalY = 0.3 } = {}) {
  if (!Number.isFinite(step) || step <= 0 || !Array.isArray(document.meshes)) throw new Error('Invalid collision raster input.');
  const { min, max } = document.bounds || {};
  if (![min?.[0], min?.[2], max?.[0], max?.[2]].every(Number.isFinite)) throw new Error('Missing native collision bounds.');
  const minX = Math.floor(min[0] / step) * step;
  const minZ = Math.floor(min[2] / step) * step;
  const width = Math.ceil((max[0] - minX) / step) + 1;
  const depth = Math.ceil((max[2] - minZ) / step) + 1;
  if (width < 1 || depth < 1 || width > 4096 || depth > 4096) throw new Error('Activity terrain dimensions exceed server limits.');
  const heights = Array(width * depth).fill(null);
  let solidMeshes = 0; let walkableTriangles = 0;
  for (const mesh of document.meshes) {
    if (!mesh.flags?.sf_solid || mesh.flags.sf_cannot_land) continue;
    solidMeshes++;
    for (const indices of mesh.triangles) {
      if (indices.length !== 3 || indices.some(i => !Number.isInteger(i) || i < 0 || i >= mesh.vertices.length)) throw new Error('Invalid native triangle index.');
      const [a, b, c] = indices.map(i => mesh.vertices[i]);
      if ([...a, ...b, ...c].length !== 9 || ![...a, ...b, ...c].every(Number.isFinite)) throw new Error('Invalid native vertex.');
      const ab = b.map((v, i) => v - a[i]); const ac = c.map((v, i) => v - a[i]);
      const nx = ab[1] * ac[2] - ab[2] * ac[1];
      const ny = ab[2] * ac[0] - ab[0] * ac[2];
      const nz = ab[0] * ac[1] - ab[1] * ac[0];
      if (ny <= 0 || ny / Math.hypot(nx, ny, nz) <= minUpNormalY) continue;
      walkableTriangles++;
      const x0 = Math.max(0, Math.ceil((Math.min(a[0], b[0], c[0]) - minX) / step - 1e-7));
      const x1 = Math.min(width - 1, Math.floor((Math.max(a[0], b[0], c[0]) - minX) / step + 1e-7));
      const z0 = Math.max(0, Math.ceil((Math.min(a[2], b[2], c[2]) - minZ) / step - 1e-7));
      const z1 = Math.min(depth - 1, Math.floor((Math.max(a[2], b[2], c[2]) - minZ) / step + 1e-7));
      const denominator = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
      for (let row = z0; row <= z1; row++) {
        for (let column = x0; column <= x1; column++) {
          const x = minX + column * step; const z = minZ + row * step;
          const u = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / denominator;
          const v = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / denominator;
          const w = 1 - u - v;
          if (Math.min(u, v, w) < -1e-7) continue;
          const height = Math.round((u * a[1] + v * b[1] + w * c[1]) * 1e6) / 1e6;
          const cell = row * width + column;
          if (heights[cell] === null || height > heights[cell]) heights[cell] = height;
        }
      }
    }
  }
  if (!heights.some(Number.isFinite)) throw new Error('Native collision has no walkable cells.');
  return { minX, minZ, step, width, depth, heights, worldId: document.worldId,
    coordinateSpace: 'canonical native ×0.1; origin retained',
    surfaceRule: { minUpNormalY, solidOnly: true, cannotLandExcluded: true, nullIsImpassable: true,
      limitation: 'Topmost static surface only; multi-level navigation, water, obstacles and native race routes require separate rules.' },
    collisionSummary: { solidMeshes, walkableTriangles } };
}

export async function buildActivityTerrain(root = ROOT) {
  const directory = path.join(root, '.local', 'activities');
  const destination = path.join(directory, 'terrain'); await mkdir(destination, { recursive: true });
  const summaries = [];
  for (const model of MODELS) {
    const filename = `${model}-collision.json`;
    const bytes = await readFile(path.join(directory, filename));
    const collision = JSON.parse(bytes);
    if (collision.worldId !== `native-${model}` || collision.scale !== 0.1
      || JSON.stringify(collision.position) !== '[0,0,0]') throw new Error(`Unexpected native coordinate mapping for ${model}.`);
    const field = rasterizeActivityCollision(collision);
    field.source = { collision: filename, sha256: createHash('sha256').update(bytes).digest('hex'), scaleAlreadyApplied: 0.1 };
    const output = path.join(destination, `${field.worldId}.json`);
    await writeFile(`${output}.tmp`, `${JSON.stringify(field)}\n`); await rename(`${output}.tmp`, output);
    const valid = field.heights.filter(Number.isFinite);
    summaries.push({ worldId: field.worldId, cells: field.heights.length, valid: valid.length,
      minY: valid.reduce((a,b) => Math.min(a,b), Infinity), maxY: valid.reduce((a,b) => Math.max(a,b), -Infinity),
      ...field.collisionSummary });
  }
  await writeFile(path.join(destination, 'report.json'), `${JSON.stringify(summaries, null, 2)}\n`);
  return summaries;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(await buildActivityTerrain(), null, 2));
}
