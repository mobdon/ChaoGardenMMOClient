import test from 'node:test';
import assert from 'node:assert/strict';
import { MeshBuilder, NullEngine, PBRMaterial, Scene, Texture, TransformNode } from '@babylonjs/core';
import { METAL_CHAO_HIDDEN, METAL_CHAO_PARTS, applyMetalSkin } from '../src/metal-chao.js';

function chao() {
  const scene = new Scene(new NullEngine());
  const root = new TransformNode('chao', scene);
  const meshes = {};
  for (const index of [1, 3, 6, 16, 18]) {
    const node = new TransformNode(`${String(index).padStart(3, '0')}_object_`, scene);
    node.parent = root;
    const mesh = MeshBuilder.CreateBox(`mesh-${index}`, {}, scene);
    mesh.material = new PBRMaterial(`mat-${index}`, scene);
    mesh.parent = node;
    meshes[index] = mesh;
  }
  return { scene, root, meshes };
}

test('metal skin hides the replaced Chao nodes and restores them', () => {
  const { scene, root, meshes } = chao();
  const original = meshes[1].material;
  applyMetalSkin(root, new Texture(null, scene));
  for (const [index, mesh] of Object.entries(meshes)) assert.equal(mesh.isVisible, !METAL_CHAO_HIDDEN.has(Number(index)));
  assert.notEqual(meshes[1].material, original);
  assert.equal(meshes[1].material.albedoTexture, null);
  applyMetalSkin(root, null);
  for (const mesh of Object.values(meshes)) assert.equal(mesh.isVisible, true);
  assert.equal(meshes[1].material, original);
});

test('every part rides a real Chao node and none rides a hidden one except the head', () => {
  for (const part of METAL_CHAO_PARTS) {
    assert.match(part.node, /^\d{3}$/);
    assert.ok(part.key === 'metal-chao-head' || !METAL_CHAO_HIDDEN.has(Number(part.node)) || /shoe/.test(part.key), part.key);
  }
});
