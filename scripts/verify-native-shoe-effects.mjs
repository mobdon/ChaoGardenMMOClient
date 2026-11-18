// Opt-in parity proof for ignored local native aliases, not a gameplay engine.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { Engine, LoadAssetContainerAsync, NullEngine, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { collectNativeShoeParts, updateNativeShoeEffects } from '../src/chao-attachments.js';

const sourceRoot = resolve(process.argv[2] || '.'), output = resolve(process.argv[3] || '.local/native-shoe-effects/v3');
const originalDirectory = resolve(process.argv[4] || resolve(sourceRoot, '.local/imports'));
const references = JSON.parse(await readFile(resolve(output, 'saio-reference.json'))).models;
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function glb(bytes) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67);
  const size = bytes.readUInt32LE(12);
  return { document: JSON.parse(bytes.subarray(20, 20 + size)), binary: bytes.subarray(28 + size) };
}
function pack(document, binary) {
  let text = Buffer.from(JSON.stringify(document));
  text = Buffer.concat([text, Buffer.alloc((4 - text.length % 4) % 4, 32)]);
  const header = Buffer.alloc(20), binHeader = Buffer.alloc(8);
  [0x46546c67, 2, 28 + text.length + binary.length, text.length, 0x4e4f534a].forEach((v, i) => header.writeUInt32LE(v, i * 4));
  binHeader.writeUInt32LE(binary.length); binHeader.writeUInt32LE(0x004e4942, 4);
  return Buffer.concat([header, text, binHeader, binary]);
}
function accessor(asset, index) {
  const data = asset.document.accessors[index], view = asset.document.bufferViews[data.bufferView];
  const start = (view.byteOffset || 0) + (data.byteOffset || 0), width = { SCALAR: 1, VEC3: 3, VEC4: 4 }[data.type];
  const unit = data.componentType === 5126 ? 4 : 2;
  return Array.from({ length: data.count }, (_, i) => Array.from({ length: width }, (_, c) => unit === 4
    ? asset.binary.readFloatLE(start + (i * width + c) * unit) : asset.binary.readUInt16LE(start + (i * width + c) * unit)));
}
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
const triangleKey = values => [values.join(','), [values[1], values[2], values[0]].join(','), [values[2], values[0], values[1]].join(',')].sort()[0];
const engine = new NullEngine(), scene = new Scene(engine), records = [];
try {
  for (const side of ['left', 'right']) {
    const originalBytes = await readFile(resolve(originalDirectory, `shadow-air-shoes-${side}.glb`));
    const bytes = await readFile(resolve(output, `shadow-air-shoes-${side}.glb`));
    const provenance = JSON.parse(await readFile(resolve(output, `shadow-air-shoes-${side}.glb.provenance.json`)));
    const original = glb(originalBytes), prepared = glb(bytes), doc = prepared.document;
    assert.equal(hash(bytes), provenance.outputSha256);
    assert.equal(hash(originalBytes), provenance.originalShoeSha256);
    assert.deepEqual(prepared.binary.subarray(0, original.binary.length), original.binary, 'every original shoe buffer/image byte retained');
    for (const key of ['bufferViews', 'accessors', 'materials', 'meshes', 'nodes']) assert.deepEqual(doc[key].slice(0, original.document[key].length), original.document[key]);
    assert.deepEqual(doc.images, original.document.images);
    const jets = doc.nodes.filter(node => node.extras?.sa2ShoeEffect);
    assert.equal(jets.length, 4);
    const native = [];
    for (const node of jets) {
      const metadata = node.extras.sa2ShoeEffect, reference = references.find(row => row.modelId === metadata.modelId);
      const primitive = doc.meshes[node.mesh].primitives[0], material = doc.materials[primitive.material];
      assert.equal(metadata.nativeSha256, reference.sourceSha256);
      assert.equal(material.alphaMode, 'BLEND'); assert.equal(material.doubleSided, true);
      assert.deepEqual(material.extensions.KHR_materials_unlit, {});
      assert.equal(material.extras.sa2Source.useTexture, false);
      assert.equal(material.extras.sa2Source.sourceAlpha, reference.material.sourceAlpha);
      assert.equal(material.extras.sa2Source.destinationAlpha, reference.material.destinationAlpha);
      assert.ok(!material.pbrMetallicRoughness.baseColorTexture);
      material.pbrMetallicRoughness.baseColorFactor.forEach((v, i) => assert.ok(Math.abs(v - reference.material.diffuse[i]) < 1e-6));
      const positions = accessor(prepared, primitive.attributes.POSITION).map(point => point.map((v, i) => v + provenance.shoeOriginShift[i]));
      const color = accessor(prepared, primitive.attributes.COLOR_0), indices = accessor(prepared, primitive.indices).flat();
      const map = positions.map(point => reference.positions.findIndex(other => distance(point, other) < 1e-5));
      assert.ok(map.every(i => i >= 0)); assert.equal(new Set(map).size, 12);
      const actual = Array.from({ length: indices.length / 3 }, (_, i) => triangleKey(indices.slice(i * 3, i * 3 + 3).map(v => map[v]))).sort();
      assert.deepEqual(actual, reference.triangles.map(triangleKey).sort(), 'original oriented triangles must match independent SAIO conversion');
      // Compare colors by source vertex: original SAIO colors are per-corner.
      reference.triangles.flat().forEach((vertex, corner) => {
        const index = map.indexOf(vertex);
        color[index].forEach((v, channel) => assert.ok(Math.abs(v - reference.cornerColors[corner][channel]) < 1e-6));
      });
      assert.ok(color.some(value => value[3] === 0) && color.some(value => value[3] === 1));
      native.push({ modelId: metadata.modelId, vertices: positions.length, triangles: actual.length, sourceSha256: reference.sourceSha256 });
    }
    // Load the original jet materials through the real glTF PBR path. Select
    // only jet nodes in memory so Node does not need to decode boot images.
    // The prepared on-disk aliases remain untouched and are loaded below.
    const materialDoc = structuredClone(doc);
    materialDoc.scenes[materialDoc.scene ?? 0].nodes = materialDoc.nodes.flatMap((node, i) => node.extras?.sa2ShoeEffect ? [i] : []);
    const materialContainer = await LoadAssetContainerAsync(new Uint8Array(pack(materialDoc, prepared.binary)), scene, { pluginExtension: '.glb' });
    const materialEntries = materialContainer.instantiateModelsToScene(name => `pbr-${side}-${name}`, false, { doNotInstantiate: true });
    const materialRoot = new TransformNode(`pbr-${side}`, scene);
    for (const root of materialEntries.rootNodes) root.parent = materialRoot;
    const materialParts = collectNativeShoeParts(materialRoot);
    assert.equal(materialParts.effects.length, 4);
    for (const { mesh } of materialParts.effects) {
      const material = mesh.material;
      assert.equal(material.getClassName(), 'PBRMaterial'); assert.equal(material.unlit, true);
      assert.equal(material.transparencyMode, 2); assert.equal(material.alphaMode, Engine.ALPHA_COMBINE);
      assert.equal(material.disableDepthWrite, true); assert.equal(material.backFaceCulling, false);
      assert.equal(material.albedoTexture, null); assert.equal(mesh.hasVertexAlpha, true);
      const colors = mesh.getVerticesData('color');
      assert.ok(colors.some((v, i) => i % 4 === 3 && v === 0));
      assert.ok(colors.some((v, i) => i % 4 === 3 && v === 1));
    }
    materialEntries.dispose(); materialRoot.dispose(); materialContainer.dispose();
    const container = await LoadAssetContainerAsync(new Uint8Array(bytes), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
    const entries = container.instantiateModelsToScene(name => `proof-${side}-${name}`, false, { doNotInstantiate: true });
    const actor = new TransformNode(`${side}-actor`, scene), foot = new TransformNode(`${side}-foot`, scene), gear = new TransformNode(`${side}-gear`, scene);
    foot.parent = actor; gear.parent = foot; gear.scaling.setAll(.02);
    for (const root of entries.rootNodes) root.parent = gear;
    const parts = collectNativeShoeParts(gear);
    assert.equal(parts.effects.length, 4); assert.ok(parts.meshes.length > 0);
    assert.ok(parts.effects.every(effect => !parts.meshes.includes(effect.mesh)));
    const state = parts.effects.map(({ mesh }) => ({ mesh, parent: mesh.parent, local: mesh.position.asArray(), scaling: mesh.scaling.asArray() }));
    updateNativeShoeEffects(parts.effects, 1, 1.994);
    assert.equal(parts.effects.filter(effect => effect.mesh.isEnabled()).length, 2);
    const first = parts.effects[0].mesh; first.computeWorldMatrix(true); const before = first.getAbsolutePosition().clone();
    foot.position.z = .5; foot.rotation.x = .4; foot.computeWorldMatrix(true); first.computeWorldMatrix(true);
    assert.ok(Vector3.Distance(before, first.getAbsolutePosition()) > .1);
    state.forEach(({ mesh, parent, local, scaling }) => { assert.equal(mesh.parent, parent); assert.deepEqual(mesh.position.asArray(), local); assert.deepEqual(mesh.scaling.asArray(), scaling); });
    updateNativeShoeEffects(parts.effects, 0, 1.994); assert.ok(parts.effects.every(effect => !effect.mesh.isEnabled()));
    gear.dispose(); assert.ok(parts.effects.every(effect => effect.mesh.isDisposed()));
    entries.dispose(); actor.dispose(); container.dispose();
    records.push({ side, sha256: hash(bytes), originalShoeSha256: hash(originalBytes), native, originalBinaryBytes: original.binary.length, originalGeometryImagesPreserved: true, actualPbrJetMaterialsPassed: true, actualLoadAndDisposalPassed: true });
  }
} finally { engine.dispose(); }
const proof = { result: 'PASS', records, limits: 'Original source geometry/material parity and real loader/attachment proof. Native timing and Chao mount adaptation remain distinct from retail rendering; browser visual acceptance is separate.' };
await writeFile(resolve(output, 'verification.json'), JSON.stringify(proof, null, 2));
console.log(JSON.stringify(proof, null, 2));
