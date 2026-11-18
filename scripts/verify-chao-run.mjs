import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { NullEngine, Scene, LoadAssetContainerAsync, TransformNode } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { attachNativeRun, captureChaoPose, nativeGaitRate } from '../src/chao-locomotion.js';

const engine = new NullEngine(), scene = new Scene(engine);
try {
  const bytes = await readFile('.local/imports/chao-locomotion.glb');
  const source = await LoadAssetContainerAsync(new Uint8Array(bytes), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
  assert.deepEqual(source.animationGroups.map(group => group.name), ['alm_run']);
  const forms = [];
  for (const name of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const asset = await LoadAssetContainerAsync(new Uint8Array(await readFile(`.local/imports/${name}.glb`)), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
    const entries = asset.instantiateModelsToScene(value => `${name}-${value}`, false, { doNotInstantiate: true });
    const visual = new TransformNode(name, scene); visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { visual, entries }, pose = captureChaoPose(imported);
    const nodes = visual.getDescendants(), fixed = nodes.map(node => ({ parent: node.parent, scale: node.scaling.asArray() }));
    const group = attachNativeRun(imported, source, pose);
    assert.ok(group); assert.equal(group.targetedAnimations.length, 80); group.enableBlending = false;
    group.start(true, nativeGaitRate('Run', 1.236));
    const signatures = new Set();
    for (let frame = 0; frame <= 20; frame++) {
      group.goToFrame(group.from + (group.to - group.from) * frame / 20);
      nodes.forEach((node, index) => {
        assert.equal(node.parent, fixed[index].parent); assert.deepEqual(node.scaling.asArray(), fixed[index].scale);
        node.computeWorldMatrix(true); assert.ok(node.getWorldMatrix().asArray().every(Number.isFinite));
      });
      signatures.add([...pose.values()].map(({ node }) => [...node.position.asArray(), ...node.rotationQuaternion.asArray()].map(value => value.toFixed(4)).join(',')).join(';'));
    }
    assert.ok(signatures.size >= 10, 'native Run should have distinct poses');
    forms.push({ name, originalNodes: nodes.length, indexedNativeNodes: pose.size, animationChannels: group.targetedAnimations.length,
      sampledFrames: 21, distinctPoses: signatures.size, originalParentsAndScalesPreserved: true, rateAt1_236UnitsPerSecond: group.speedRatio });
    entries.dispose(); visual.dispose(); asset.dispose();
  }
  const report = { result: 'PASS', nativeState: 119, nativeClip: 'alm_run', nativePlaySpeed: .21,
    sourceSha256: createHash('sha256').update(bytes).digest('hex'), forms,
    note: 'Source Chao translation deltas and bind-relative rotations on original40nodes; no skeleton/mesh/weight/scale replacement. Visual/browser recording remains separate.' };
  await writeFile('.local/imports/chao-run-runtime-proof.json', `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report));
} finally { engine.dispose(); }
