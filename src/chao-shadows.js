import { Color3, Engine, Material, Mesh, StandardMaterial, Texture, Vector3, VertexData } from '@babylonjs/core';

// The image is SA2 PC al_mm_kage.prs/kyotu32_marukage, not generated art.
// Footprint size, terrain clipping and opacity are browser presentation
// adaptations. This does not execute the original AL_*ShadowTex engine.
const finite = value => Number.isFinite(value);
const coordinates = value => value && [value.x, value.y, value.z].every(finite);
const validIndex = index => index && typeof index.query === 'function' && Number.isSafeInteger(index.triangleCount) && index.triangleCount >= 0;
const emptyPatch = () => ({ positions: [], normals: [], uvs: [], indices: [], candidates: 0 });

function clipPolygon(input, axis, limit, keepGreater) {
  if (!input.length) return input;
  const out = [];
  let previous = input.at(-1), previousInside = keepGreater ? previous[axis] >= limit : previous[axis] <= limit;
  for (const current of input) {
    const inside = keepGreater ? current[axis] >= limit : current[axis] <= limit;
    if (inside !== previousInside) {
      const t = (limit - previous[axis]) / (current[axis] - previous[axis]);
      out.push(previous.map((value, index) => value + (current[index] - value) * t));
    }
    if (inside) out.push(current);
    previous = current; previousInside = inside;
  }
  return out;
}

// Build once after a static garden's world transforms are final. Queries visit
// only the small footprint's cells; no whole-garden scan occurs per actor/frame.
export function createNativeShadowTerrainIndex(meshes, cellSize = 1) {
  if (!finite(cellSize) || cellSize <= 0) throw new RangeError('Invalid shadow terrain cell size');
  const cells = new Map(), triangles = [], broad = [];
  for (const mesh of meshes || []) {
    if (!mesh || mesh.isDisposed?.() || mesh.metadata?.noShadow || ((mesh.metadata?.surfaceFlags || 0) & 0x8000)) continue;
    const positions = mesh.getVerticesData('position');
    if (!positions?.length) continue;
    const indices = mesh.getIndices() || Array.from({ length: positions.length / 3 }, (_, i) => i);
    mesh.computeWorldMatrix(true);
    const matrix = mesh.getWorldMatrix();
    for (let i = 0; i + 2 < indices.length; i += 3) {
      const vertices = Array.from(indices.slice(i, i + 3), index => Vector3.TransformCoordinates(Vector3.FromArray(positions, index * 3), matrix));
      if (!vertices.every(coordinates)) continue;
      const normal = Vector3.Cross(vertices[1].subtract(vertices[0]), vertices[2].subtract(vertices[0]));
      const length = normal.length();
      // Do not stamp vertical walls or degenerate geometry. Winding differs
      // between native exports; the receiving normal always faces upward.
      if (length < 1e-10 || Math.abs(normal.y) / length < .2) continue;
      normal.scaleInPlace((normal.y < 0 ? -1 : 1) / length);
      const triangle = { vertices: vertices.map(v => v.asArray()), normal: normal.asArray(),
        minX: Math.min(...vertices.map(v => v.x)), maxX: Math.max(...vertices.map(v => v.x)),
        minZ: Math.min(...vertices.map(v => v.z)), maxZ: Math.max(...vertices.map(v => v.z)) };
      const id = triangles.push(triangle) - 1;
      const x0 = Math.floor(triangle.minX / cellSize), x1 = Math.floor(triangle.maxX / cellSize);
      const z0 = Math.floor(triangle.minZ / cellSize), z1 = Math.floor(triangle.maxZ / cellSize);
      // A few huge native floor triangles need not allocate millions of bins.
      if ((x1 - x0 + 1) * (z1 - z0 + 1) > 4096) { broad.push(id); continue; }
      for (let x = x0; x <= x1; x++) for (let z = z0; z <= z1; z++) {
        const key = `${x},${z}`;
        if (!cells.has(key)) cells.set(key, []);
        cells.get(key).push(id);
      }
    }
  }
  return {
    triangleCount: triangles.length,
    query(minX, maxX, minZ, maxZ) {
      const ids = new Set(broad);
      for (let x = Math.floor(minX / cellSize); x <= Math.floor(maxX / cellSize); x++) {
        for (let z = Math.floor(minZ / cellSize); z <= Math.floor(maxZ / cellSize); z++) {
          for (const id of cells.get(`${x},${z}`) || []) ids.add(id);
        }
      }
      return [...ids].map(id => triangles[id]).filter(t => t.minX <= maxX && t.maxX >= minX && t.minZ <= maxZ && t.maxZ >= minZ);
    },
  };
}

export function projectNativeShadow(index, { center, width, depth, groundY = center?.y, maxDrop = .3, maxRise = .12, lift = .001 } = {}) {
  const result = emptyPatch();
  if (!validIndex(index) || !coordinates(center) || ![width, depth, groundY, maxDrop, maxRise, lift].every(finite)
      || width <= 0 || depth <= 0 || width > 16 || depth > 16 || maxDrop < 0 || maxRise < 0 || lift < 0 || lift > .01) return result;
  const minX = center.x - width / 2, maxX = center.x + width / 2;
  const minZ = center.z - depth / 2, maxZ = center.z + depth / 2;
  const candidates = index.query(minX, maxX, minZ, maxZ);
  result.candidates = candidates.length;
  for (const triangle of candidates) {
    let polygon = triangle.vertices;
    for (const [axis, limit, greater] of [[0, minX, true], [0, maxX, false], [2, minZ, true], [2, maxZ, false], [1, groundY - maxDrop, true], [1, groundY + maxRise, false]]) {
      polygon = clipPolygon(polygon, axis, limit, greater);
    }
    if (polygon.length < 3) continue;
    const first = result.positions.length / 3;
    for (const [x, y, z] of polygon) {
      result.positions.push(x, y + lift, z);
      result.normals.push(...triangle.normal);
      result.uvs.push((x - minX) / width, (z - minZ) / depth);
    }
    for (let i = 1; i < polygon.length - 1; i++) result.indices.push(first, first + i, first + i + 1);
  }
  return result;
}

export function createNativeChaoShadow(scene, { textureUrl, texture: suppliedTexture, terrainMeshes = [], terrainIndex, opacity = .32 } = {}) {
  const mesh = new Mesh('native-chao-contact-shadow', scene);
  mesh.isPickable = false; mesh.receiveShadows = false; mesh.setEnabled(false);
  mesh.metadata = { nativeShadow: true, source: 'al_mm_kage.prs/kyotu32_marukage', adaptedProjection: true };
  const material = new StandardMaterial('native-chao-contact-shadow-material', scene);
  material.disableLighting = true;
  material.diffuseColor = Color3.White(); material.emissiveColor = Color3.White(); material.specularColor = Color3.Black();
  material.useAlphaFromDiffuseTexture = true;
  material.transparencyMode = Material.MATERIAL_ALPHABLEND; material.alphaMode = Engine.ALPHA_COMBINE;
  material.depthFunction = Engine.LEQUAL;
  material.backFaceCulling = false; material.disableDepthWrite = true;
  material.alpha = finite(opacity) ? Math.max(0, Math.min(1, opacity)) : .32;
  mesh.material = material;
  let disposed = false, failed = false, loaded = Boolean(suppliedTexture), lastKey = '';
  let index = terrainIndex === undefined ? createNativeShadowTerrainIndex(terrainMeshes) : (validIndex(terrainIndex) ? terrainIndex : null);
  const texture = suppliedTexture || (textureUrl ? new Texture(textureUrl, scene, false, false, Texture.BILINEAR_SAMPLINGMODE,
    () => { if (!disposed) { loaded = true; lastKey = ''; } },
    () => { failed = true; loaded = false; mesh.setEnabled(false); }) : null);
  if (texture) {
    texture.hasAlpha = true;
    texture.wrapU = texture.wrapV = Texture.CLAMP_ADDRESSMODE;
    material.diffuseTexture = texture; material.emissiveTexture = texture;
  }
  return {
    mesh, texture,
    get ready() { return !disposed && !failed && loaded && Boolean(texture); },
    setTerrainMeshes(meshes) { index = createNativeShadowTerrainIndex(meshes); lastKey = ''; },
    setTerrainIndex(sharedIndex) { index = validIndex(sharedIndex) ? sharedIndex : null; lastKey = ''; },
    update(footprint = {}) {
      if (disposed || failed || !loaded || !texture || footprint.enabled === false) { mesh.setEnabled(false); lastKey = ''; return false; }
      const key = JSON.stringify([footprint.center?.x, footprint.center?.y, footprint.center?.z, footprint.width, footprint.depth, footprint.groundY, footprint.maxDrop, footprint.maxRise, footprint.lift]);
      if (key === lastKey) return mesh.isEnabled();
      lastKey = key;
      const patch = projectNativeShadow(index, footprint);
      mesh.metadata.candidateTriangles = patch.candidates;
      if (!patch.indices.length) { mesh.setEnabled(false); return false; }
      const vertices = new VertexData();
      vertices.positions = patch.positions; vertices.normals = patch.normals; vertices.uvs = patch.uvs; vertices.indices = patch.indices;
      vertices.applyToMesh(mesh, true); mesh.setEnabled(true);
      return true;
    },
    dispose() {
      if (disposed) return;
      disposed = true; mesh.dispose(); material.dispose();
      if (!suppliedTexture) texture?.dispose();
    },
  };
}
