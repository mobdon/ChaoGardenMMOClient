import test from 'node:test';
import assert from 'node:assert/strict';
import { rasterizeActivityCollision } from '../scripts/build-activity-terrain.js';

const mesh = (vertices, triangles, flags = {}) => ({ vertices, triangles, flags: { sf_solid: true, ...flags } });
const document = meshes => ({ worldId: 'synthetic', bounds: { min: [30,0,0], max: [34,4,4] }, meshes });

test('native terrain retains distant coordinates, slopes, holes and topmost solid surface', () => {
  const base = mesh([[30,0,0],[30,2,2],[32,2,2],[32,0,0]], [[0,1,2],[0,2,3]]);
  const upper = mesh([[30,3,0],[30,3,1],[31,3,0]], [[0,1,2]]);
  const field = rasterizeActivityCollision(document([base, upper]), { step: 1 });
  assert.equal(field.minX, 30); assert.equal(field.width, 5);
  const height = (x,z) => field.heights[z * field.width + x - 30];
  assert.equal(height(30,0), 3); assert.equal(height(31,1), 1);
  assert.equal(height(32,2), 2); assert.equal(height(33,2), null);
});

test('non-solid, cannot-land, vertical and downward surfaces cannot fill native holes', () => {
  const flat = [[30,2,0],[30,2,4],[34,2,0]];
  const field = rasterizeActivityCollision(document([
    mesh(flat, [[0,1,2]], { sf_solid: false }),
    mesh(flat, [[0,1,2]], { sf_cannot_land: true }),
    mesh(flat, [[0,2,1]]),
    mesh([[31,0,0],[31,4,0],[31,0,4]], [[0,1,2]]),
    mesh([[30,0,0],[30,0,1],[31,0,0]], [[0,1,2]]),
  ]), { step: 1 });
  assert.equal(field.heights[0], 0);
  assert.equal(field.heights[2 * field.width + 1], null);
  assert.equal(field.collisionSummary.walkableTriangles, 1);
});
