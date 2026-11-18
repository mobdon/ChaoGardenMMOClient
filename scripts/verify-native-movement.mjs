// Opt-in CPU proof using locally converted original assets. The controlled
// snapshot sequence exercises presentation; it is not a live server recording.
// CHAO_ASSET_ROOT may point at a separate checkout's ignored source assets.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { LoadAssetContainerAsync, NullEngine, Scene, TransformNode } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { attachNativeLocomotion, captureChaoPose, createNativeMotionState, updateNativeLocomotion } from '../src/chao-locomotion.js';
import { createShadowSkate } from '../src/shadow-skate.js';

const assetRoot = resolve(process.env.CHAO_ASSET_ROOT || '.');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
async function localAsset(name) {
  try { return await readFile(`.local/imports/${name}`); }
  catch (error) { if (error.code !== 'ENOENT') throw error; return readFile(resolve(assetRoot, `.local/imports/${name}`)); }
}
const libraryBytes = await localAsset('chao-locomotion.glb');
const libraryJson = JSON.parse(libraryBytes.subarray(20, 20 + libraryBytes.readUInt32LE(12)));
const shadowBytes = await localAsset('shadow-skate-motion.json'), motion = JSON.parse(shadowBytes);
const engine = new NullEngine(), scene = new Scene(engine), forms = [];
const load = bytes => LoadAssetContainerAsync(new Uint8Array(bytes), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
const signature = pose => [...pose.values()].map(({ node }) => [...node.position.asArray(), ...node.rotationQuaternion.asArray()].map(value => value.toFixed(5)).join(',')).join(';');
const dt = 1 / 60;
const phases = [
  { name: 'idle', frames: 36, speed: 0 },
  { name: 'walk', frames: 60, speed: .798 },
  { name: 'run', frames: 60, speed: 1.236 },
  { name: 'jump', frames: 45, grounded: false, speed: 1.236, offset: frame => .02 + frame * .01, verticalVelocity: .6 },
  { name: 'apex', frames: 24, grounded: false, speed: 1.236, offset: () => .47, verticalVelocity: 0 },
  { name: 'fall', frames: 45, grounded: false, speed: 1.236, offset: frame => .47 - frame * .01, verticalVelocity: -.6 },
  { name: 'landing', frames: 30, speed: 1.236 },
  { name: 'skate-slow', frames: 75, speed: 1.6 },
  { name: 'skate-fast', frames: 75, speed: 1.994 },
  { name: 'stop', frames: 36, speed: 0 },
  { name: 'air-before-mount', frames: 12, grounded: false, speed: 1.994, offset: () => .2, verticalVelocity: .6 },
  { name: 'mounted-pilot', frames: 24, speed: 1.994, mounted: true },
  { name: 'dismounted', frames: 24, speed: .798 },
  { name: 'air-before-stale', frames: 12, grounded: false, speed: 1.994, offset: () => .2, verticalVelocity: .6 },
  { name: 'stale', frames: 24, speed: 1.994, fresh: false },
  { name: 'reconnected', frames: 24, speed: .798 },
  { name: 'final-idle', frames: 24, speed: 0 },
];
try {
  const library = await load(libraryBytes);
  assert.deepEqual(library.animationGroups.map(group => group.name), ['alm_run', 'alm_jump_up', 'alm_jump_down']);
  for (const name of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const modelBytes = await localAsset(`${name}.glb`), asset = await load(modelBytes);
    const entries = asset.instantiateModelsToScene(value => `${name}-${value}`, false, { doNotInstantiate: true });
    const visual = new TransformNode(`${name}-visual`, scene); visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { visual, entries,
      idle: entries.animationGroups.find(group => /idle/i.test(group.name)),
      walk: entries.animationGroups.find(group => /walk/i.test(group.name)) };
    assert.ok(imported.idle && imported.walk, 'original Idle and Walk remain available');
    const pose = captureChaoPose(imported), nodes = visual.getDescendants();
    const fixed = nodes.map(node => ({ parent: node.parent, scale: node.scaling.asArray() }));
    const skate = createShadowSkate(imported, { motion }), state = createNativeMotionState();
    attachNativeLocomotion(imported, library, pose);
    for (const group of entries.animationGroups) group.enableBlending = false;
    for (const key of ['run', 'jumpUp', 'jumpDown']) assert.equal(imported[key].targetedAnimations.length, 80);
    let previousMode, transitions = 0, sampledFrames = 0, skatingFrames = 0;
    const counts = {}, nativeStates = new Set(), phaseReports = [], poses = new Map();
    for (const phase of phases) {
      const phaseModes = new Set(), rates = new Set();
      for (let frame = 0; frame < phase.frames; frame++) {
        skate.restoreBase();
        const snapshot = { speed: phase.speed, grounded: phase.grounded ?? true, fresh: phase.fresh ?? true,
          jumpOffset: phase.offset?.(frame) ?? 0, verticalVelocity: phase.verticalVelocity ?? 0,
          mounted: phase.mounted ?? false, swimming: false };
        const selected = updateNativeLocomotion(imported, state, dt, snapshot);
        if (selected.mode !== previousMode) { transitions++; previousMode = selected.mode; }
        if (!Number.isFinite(selected.frame)) {
          const group = imported.current;
          group.goToFrame(group.from + ((sampledFrames * dt * 60 * group.speedRatio) % Math.max(.0001, group.to - group.from)));
        }
        const beforeOverlay = nodes.map(node => ({ position: node.position.asArray(), scale: node.scaling.asArray() }));
        skate.update(dt, { ...snapshot, grounded: snapshot.grounded && selected.mode !== 'Land', footwear: true });
        skate.apply();
        nodes.forEach((node, index) => {
          assert.equal(node.parent, fixed[index].parent, 'original hierarchy changed');
          // Existing Idle/Walk scale channels round exported float32 bind
          // values (e.g.0.99999994) to1. Retargeted clips have no scale tracks.
          assert.ok(node.scaling.asArray().every((value, axis) => Math.abs(value - fixed[index].scale[axis]) < 1e-6), 'original local scale changed');
          assert.deepEqual(node.scaling.asArray(), beforeOverlay[index].scale, 'Shadow overlay changed scale');
          assert.deepEqual(node.position.asArray(), beforeOverlay[index].position, 'Shadow source positions were copied');
          node.computeWorldMatrix(true); assert.ok(node.getWorldMatrix().asArray().every(Number.isFinite));
          if (node.rotationQuaternion) assert.ok(Math.abs(node.rotationQuaternion.length() - 1) < .0001);
        });
        if (phase.name === 'mounted-pilot' || phase.name === 'stale') {
          assert.equal(selected.mode, 'Idle'); assert.equal(state.wasGrounded, null); assert.equal(skate.diagnostics.active, false);
        }
        if (phase.name === 'dismounted' || phase.name === 'reconnected') assert.equal(selected.mode, 'Walk', 'resume manufactured a landing');
        if (['Jump', 'Fall', 'Land'].includes(selected.mode)) assert.equal(skate.diagnostics.active, false, 'skating leaked into airborne/recovery action');
        if (selected.nativeState) nativeStates.add(selected.nativeState);
        poses.set(selected.mode, poses.get(selected.mode) || new Set()); poses.get(selected.mode).add(signature(pose));
        phaseModes.add(selected.mode); rates.add(Number(skate.diagnostics.rate.toFixed(4)));
        counts[selected.mode] = (counts[selected.mode] || 0) + 1; sampledFrames++;
        if (skate.diagnostics.active) skatingFrames++;
      }
      phaseReports.push({ phase: phase.name, sampledFrames: phase.frames, modes: [...phaseModes], skateRates: [...rates] });
    }
    for (const mode of ['Idle', 'Walk', 'Run', 'Jump', 'Fall', 'Land']) assert.ok(counts[mode] > 10, `${mode} was not exercised`);
    for (const mode of ['Walk', 'Run', 'Jump', 'Fall', 'Land']) assert.ok(poses.get(mode).size >= 10, `${mode} source pose is static`);
    assert.ok(skatingFrames > 100); assert.equal(skate.diagnostics.active, false);
    forms.push({ name, sourceSha256: hash(modelBytes), originalRigNodes: nodes.length, indexedNativeNodes: pose.size,
      sampledFrames, skatingFrames, transitions, nativeStates: [...nativeStates].sort((a, b) => a - b), counts,
      distinctNativePoses: Object.fromEntries([...poses].map(([mode, values]) => [mode, values.size])), phaseReports,
      originalHierarchyAndLocalScalesPreserved: true, localScaleTolerance: 1e-6, shadowOverlayPositionsAndScalesExact: true, shadowPositionsNeverApplied: true, finiteMatrices: true });
    skate.dispose(); entries.dispose(); visual.dispose(); asset.dispose();
  }
  library.dispose();
  const report = { version: 1, result: 'PASS', sampledFrames: forms.reduce((sum, form) => sum + form.sampledFrames, 0),
    sourceLibrarySha256: hash(libraryBytes), sourceShadowMotionSha256: hash(shadowBytes), nativeClips: libraryJson.asset.extras.chaoLocomotion.clips,
    nativeShadowMotionIds: [172, 173, 174], forms,
    limitation: 'Controlled CPU presentation sequence, not live server, shoe/glove visual, terrain contact or Walker vehicle evidence. Native translations remain bind-relative; Shadow adaptation is rotation-only.' };
  await mkdir('.local/imports', { recursive: true });
  await writeFile('.local/imports/native-movement-runtime-proof.json', `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ result: report.result, sampledFrames: report.sampledFrames,
    sourceLibrarySha256: report.sourceLibrarySha256,
    forms: forms.map(({ name, originalRigNodes, sampledFrames, skatingFrames, transitions, distinctNativePoses }) => ({ name, originalRigNodes, sampledFrames, skatingFrames, transitions, distinctNativePoses })),
    report: '.local/imports/native-movement-runtime-proof.json' }));
} finally { engine.dispose(); }
