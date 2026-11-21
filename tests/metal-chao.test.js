import test from 'node:test';
import assert from 'node:assert/strict';
import { MeshBuilder, NullEngine, PBRMaterial, Scene, Texture, TransformNode } from '@babylonjs/core';
import { METAL_CHAO_HIDDEN, applyMetalSkin, metalClip, playMetalClip } from '../src/metal-chao.js';

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

test('his clip follows how the Chao moves', () => {
  assert.equal(metalClip({ moving: false }).name, 'stand');
  assert.equal(metalClip({ moving: true, speed: 1 }).name, 'walk');
  assert.equal(metalClip({ moving: true, speed: 1.4 }).name, 'jog');
  assert.equal(metalClip({ moving: true, speed: 2 }).name, 'run');
  assert.equal(metalClip({ moving: true, speed: 3 }).name, 'fly');
  assert.equal(metalClip({ grounded: false }).name, 'fly');
});

test('switching clips stops the old one and loops the new one', () => {
  const log = [];
  const group = name => ({ name, speedRatio: 1, start: (loop, rate) => log.push(['start', name, loop, rate]), stop: () => log.push(['stop', name]) });
  const rig = { groups: new Map(['stand', 'walk'].map(n => [n, group(n)])), current: null };
  playMetalClip(rig, { name: 'stand', rate: 1 });
  playMetalClip(rig, { name: 'stand', rate: 1 });
  playMetalClip(rig, { name: 'walk', rate: 0.8 });
  assert.deepEqual(log, [['start', 'stand', true, 1], ['stop', 'stand'], ['start', 'walk', true, 0.8]]);
  assert.equal(rig.groups.get('walk').speedRatio, 0.8);
});
