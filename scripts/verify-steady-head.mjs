// Opt-in actual-native transform proof. No authority, credentials or saves are
// accessed. Original assets and generated evidence remain ignored/local.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LoadAssetContainerAsync, Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { createSteadyHead } from '../src/chao-head.js';
import { attachNativeRun, captureChaoPose, nativeGaitRate } from '../src/chao-locomotion.js';
import { createShadowSkate } from '../src/shadow-skate.js';
import { CHAO_GEAR_JOINTS, attachRigidGearAtRest, chestSurfacePoint, collectNativeShoeParts, mirroredShadowShoeRest, shadowShoeMountRotation } from '../src/chao-attachments.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..'), options = {};
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index], value = process.argv[index + 1];
  assert.ok(['--asset-root', '--output'].includes(key) && value && !value.startsWith('--'));
  options[key.slice(2)] = value;
}
const assetRoot = resolve(options['asset-root'] || resolve(root, '.local/imports'));
const output = resolve(options.output || resolve(root, '.local/reviews/steady-head-proof.json'));
const hash = value => createHash('sha256').update(value).digest('hex'), sourceHashes = {};
const sceneSource = await readFile(resolve(root, 'src/scene.js'), 'utf8');
sourceHashes['src/scene.js'] = hash(sceneSource);
for (const name of ['src/chao-head.js', 'src/chao-locomotion.js', 'src/shadow-skate.js', 'src/chao-attachments.js', 'scripts/verify-steady-head.mjs']) sourceHashes[name] = hash(await readFile(resolve(root, name)));

function geometryPoints(meshes, toLocal = Matrix.Identity()) {
  return meshes.flatMap(mesh => {
    mesh.computeWorldMatrix(true); const positions = mesh.getVerticesData('position') || [];
    const transform = mesh.getWorldMatrix().multiply(toLocal);
    return Array.from({ length: positions.length / 3 }, (_, i) => Vector3.TransformCoordinates(Vector3.FromArray(positions, i * 3), transform));
  });
}
function boundsFor(points) {
  if (!points.length) return null;
  let min = new Vector3(Infinity, Infinity, Infinity), max = min.negate();
  for (const point of points) { min = Vector3.Minimize(min, point); max = Vector3.Maximize(max, point); }
  return { min, max, center: min.add(max).scale(.5), size: max.subtract(min) };
}
const captureStart = sceneSource.indexOf('  function captureGearSockets('), captureEnd = sceneSource.indexOf('\n  function gearMountRotation(', captureStart);
assert.ok(captureStart >= 0 && captureEnd > captureStart, 'Production gear capture function unavailable');
const capture = new Function('CHAO_GEAR_JOINTS', 'Matrix', 'Vector3', 'geometryPoints', 'boundsFor', 'chestSurfacePoint',
  `return (${sceneSource.slice(captureStart, captureEnd).trim()});`)(CHAO_GEAR_JOINTS, Matrix, Vector3, geometryPoints, boundsFor, chestSurfacePoint);
const groundStart = sceneSource.indexOf('  function groundVisualFeet('), groundEnd = sceneSource.indexOf('\n  function groundImportedFeet(', groundStart);
assert.ok(groundStart >= 0 && groundEnd > groundStart, 'Production foot grounding function unavailable');
const ground = new Function(`return (${sceneSource.slice(groundStart, groundEnd).trim()});`)();

const engine = new NullEngine(), scene = new Scene(engine), assets = new Map();
async function asset(name) {
  if (!assets.has(name)) {
    const bytes = await readFile(resolve(assetRoot, `${name}.glb`)); sourceHashes[`${name}.glb`] = hash(bytes);
    assets.set(name, await LoadAssetContainerAsync(new Uint8Array(bytes), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } }));
  }
  return assets.get(name);
}
function localPosition(node, relativeTo) {
  node.computeWorldMatrix(true); relativeTo.computeWorldMatrix(true);
  return Vector3.TransformCoordinates(node.getAbsolutePosition(), Matrix.Invert(relativeTo.getWorldMatrix()));
}
function localRotation(node, relativeTo) {
  node.computeWorldMatrix(true); relativeTo.computeWorldMatrix(true);
  const rotation = Quaternion.Identity();
  node.getWorldMatrix().multiply(Matrix.Invert(relativeTo.getWorldMatrix())).decompose(new Vector3(), rotation, new Vector3());
  return rotation.normalize();
}
const angle = (a, b) => 2 * Math.acos(Math.min(1, Math.abs(Quaternion.Dot(a, b)))) * 180 / Math.PI;
const range = samples => samples[0].map((_, axis) => Math.max(...samples.map(p => p[axis])) - Math.min(...samples.map(p => p[axis])));
const ready = { moving: true, grounded: true, swimming: false, mounted: false, fresh: true };
const forms = [];
try {
  const runAsset = await asset('chao-locomotion'), shoeAsset = await asset('shadow-air-shoes-right');
  const motionBytes = await readFile(resolve(assetRoot, 'shadow-skate-motion.json'));
  sourceHashes['shadow-skate-motion.json'] = hash(motionBytes); const motion = JSON.parse(motionBytes);
  for (const form of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const container = await asset(form), entries = container.instantiateModelsToScene(name => `${form}-${name}`, false, { doNotInstantiate: true });
    const actor = new TransformNode(`${form}-actor`, scene), visual = new TransformNode(`${form}-visual`, scene);
    visual.parent = actor; visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { visual, entries, meshes: visual.getChildMeshes() }, pose = captureChaoPose(imported);
    const nodes = visual.getDescendants(), head = pose.get('016')?.node, innerHead = pose.get('016')?.node, feet = ['006', '013'].map(index => pose.get(index)?.node);
    assert.ok(head && feet.every(Boolean));
    const original = nodes.map(node => ({ node, parent: node.parent, scale: node.scaling.asArray() }));
    const fits = capture({ root: actor, imported }).footwear, shoes = [];
    for (const fit of fits) {
      const prop = shoeAsset.instantiateModelsToScene(name => `${form}-${fit.side}-${name}`, false, { doNotInstantiate: true });
      const root = new TransformNode(`${form}-${fit.side}-shoe`, scene); root.rotationQuaternion = shadowShoeMountRotation();
      for (const node of prop.rootNodes) node.parent = root;
      const { meshes } = collectNativeShoeParts(root);
      shoes.push({ root, meshes, fit, source: boundsFor(geometryPoints(meshes)), prop });
    }
    const reference = shoes.find(shoe => shoe.fit.side === 'right'), rest = mirroredShadowShoeRest(reference.source, reference.fit.bounds);
    for (const shoe of shoes) {
      attachRigidGearAtRest(shoe.root, shoe.fit, rest[shoe.fit.side]);
      for (const mesh of shoe.fit.meshes) mesh.setEnabled(false);
    }
    imported.footwearMeshes = shoes.flatMap(shoe => shoe.meshes);
    const run = attachNativeRun(imported, runAsset, pose); assert.ok(run);
    const idle = entries.animationGroups.find(group => /Idle$/.test(group.name)), walk = entries.animationGroups.find(group => /Walk$/.test(group.name));
    assert.ok(idle && walk && innerHead);
    const trials = [];
    for (const [mode, speed] of [['Idle', 0], ['Walk', .798], ['Run119', 1.236], ['Skate172', 1.55], ['Skate173', 1.8], ['Skate174', 1.994]]) {
      actor.position.setAll(0); actor.rotation.setAll(0); visual.position.setAll(0);
      for (const entry of pose.values()) { entry.node.position.copyFrom(entry.position); entry.node.rotationQuaternion.copyFrom(entry.quaternion); }
      const steady = createSteadyHead(imported), skate = createShadowSkate(imported, { motion });
      const anchor = localPosition(head, actor), anchorRotation = localRotation(head, actor);
      const innerAnchor = localPosition(innerHead, actor), innerAnchorRotation = localRotation(innerHead, actor);
      const visibleHead = head.getChildMeshes().filter(mesh => mesh.getTotalVertices()).map(mesh => {
        mesh.computeWorldMatrix(true);
        const center = mesh.getBoundingInfo().boundingBox.center.clone();
        return { mesh, center, actorCenter: Vector3.TransformCoordinates(center, mesh.getWorldMatrix()), actorRotation: localRotation(mesh, actor), maximumCenterError: 0, maximumRotationError: 0 };
      });
      const skating = mode.startsWith('Skate'), before = [], after = [], footSignatures = new Set();
      let maximumAnchorError = 0, maximumRotationError = 0, maximumOtherLocalChange = 0, maximumInnerHeadError = 0, maximumInnerHeadRotationError = 0, basePosition, baseRotation;
      const group = mode === 'Idle' ? idle : mode === 'Walk' ? walk : run;
      group.enableBlending = false; group.start(true, nativeGaitRate(mode === 'Walk' ? 'Walk' : mode === 'Idle' ? 'Idle' : 'Run', speed));
      for (let frame = 0; frame < 660; frame++) {
        steady.restoreBase(); skate.restoreBase(); visual.position.setAll(0);
        // An actor-local anchor must follow the player's real heading/position.
        actor.rotation.y = Math.sin(frame / 90) * .8; actor.position.set(frame / 1000, .05 * Math.sin(frame / 50), -.2);
        group.goToFrame(group.from + ((frame / 60) * 60 * group.speedRatio) % (group.to - group.from));
        skate.update(1 / 60, { speed, grounded: true, footwear: skating, mounted: false, swimming: false, fresh: true });
        if (skating) skate.apply();
        ground(imported, actor.position.y, actor.scaling.y);
        steady.update(1 / 60, { ...ready, moving: mode !== 'Idle' });
        basePosition = head.position.clone(); baseRotation = head.rotationQuaternion.clone();
        const baseline = original.map(({ node }) => ({ p: node.position.clone(), q: node.rotationQuaternion?.clone(), s: node.scaling.clone() }));
        const footBefore = feet.map(foot => localPosition(foot, actor));
        const nativeHead = localPosition(head, actor);
        steady.apply();
        for (let index = 0; index < original.length; index++) {
          const node = original[index].node, saved = baseline[index];
          assert.equal(node.parent, original[index].parent);
          assert.deepEqual(node.scaling.asArray(), saved.s.asArray(), 'Head constraint changed scale');
          if (node === head) continue;
          const changed = Vector3.Distance(node.position, saved.p);
          maximumOtherLocalChange = Math.max(maximumOtherLocalChange, changed);
          assert.equal(changed, 0, `Head constraint changed another local position: ${node.name}`);
          if (saved.q) assert.deepEqual(node.rotationQuaternion.asArray(), saved.q.asArray(), `Head constraint changed another local rotation: ${node.name}`);
        }
        for (let side = 0; side < feet.length; side++) assert.ok(Vector3.Distance(footBefore[side], localPosition(feet[side], actor)) < 1e-7, 'Head constraint moved a foot');
        footSignatures.add(feet.map(foot => foot.rotationQuaternion.asArray().map(value => value.toFixed(4)).join(',')).join(';'));
        const fixedHead = localPosition(head, actor);
        before.push(nativeHead.asArray()); after.push(fixedHead.asArray());
        maximumAnchorError = Math.max(maximumAnchorError, Vector3.Distance(fixedHead, anchor));
        maximumRotationError = Math.max(maximumRotationError, angle(localRotation(head, actor), anchorRotation));
        maximumInnerHeadError = Math.max(maximumInnerHeadError, Vector3.Distance(localPosition(innerHead, actor), innerAnchor));
        maximumInnerHeadRotationError = Math.max(maximumInnerHeadRotationError, angle(localRotation(innerHead, actor), innerAnchorRotation));
        for (const part of visibleHead) {
          part.mesh.computeWorldMatrix(true);
          const actual = Vector3.TransformCoordinates(Vector3.TransformCoordinates(part.center, part.mesh.getWorldMatrix()), Matrix.Invert(actor.getWorldMatrix()));
          part.maximumCenterError = Math.max(part.maximumCenterError, Vector3.Distance(actual, part.actorCenter));
          part.maximumRotationError = Math.max(part.maximumRotationError, angle(localRotation(part.mesh, actor), part.actorRotation));
        }
      }
      assert.ok(maximumAnchorError < 2e-6, `${form}/${mode} head did not hold actor-local position: ${maximumAnchorError}`);
      assert.ok(maximumRotationError < .01, `${form}/${mode} head did not hold actor-local orientation: ${maximumRotationError}`);
      assert.ok(maximumInnerHeadError < 2e-6, `${form}/${mode} visible head016 still moves: ${maximumInnerHeadError}`);
      assert.ok(maximumInnerHeadRotationError < .01, `${form}/${mode} visible head016 still rotates: ${maximumInnerHeadRotationError}`);
      for (const part of visibleHead) {
        assert.ok(part.maximumCenterError < 2e-6, `${form}/${mode} head/eye mesh ${part.mesh.name} still moves: ${part.maximumCenterError}`);
        assert.ok(part.maximumRotationError < .01, `${form}/${mode} head/eye mesh ${part.mesh.name} still rotates: ${part.maximumRotationError}`);
      }
      if (mode !== 'Idle') assert.ok(footSignatures.size > 30, 'Original limb strokes became static');
      steady.restoreBase();
      assert.ok(Vector3.Distance(head.position, basePosition) < 1e-9, 'Original local head position did not restore');
      assert.ok(angle(head.rotationQuaternion, baseRotation) < .0001, 'Original local head orientation did not restore');
      const fallback = [];
      for (const block of [{ mounted: true }]) {
        steady.update(1 / 60, { ...ready, ...block }); steady.apply();
        assert.deepEqual(head.position.asArray(), basePosition.asArray(), 'Blocked state retained head-position overlay');
        assert.deepEqual(head.rotationQuaternion.asArray(), baseRotation.asArray(), 'Blocked state retained head-rotation overlay');
        fallback.push(Object.keys(block)[0]);
      }
      trials.push({ mode, speed, frames: 660, measuredSteadyFrames: after.length, nativeHeadPositionRangeActorUnits: range(before),
        constrainedHeadPositionRangeActorUnits: range(after), maximumAnchorError, maximumRotationErrorDegrees: maximumRotationError,
        maximumVisibleHead016AnchorError: maximumInnerHeadError, maximumVisibleHead016RotationErrorDegrees: maximumInnerHeadRotationError,
        visibleHeadAndEyeMeshes: visibleHead.map(({ mesh, maximumCenterError, maximumRotationError }) => ({ mesh: mesh.name, nativeParent: mesh.parent.name, maximumCenterError, maximumRotationErrorDegrees: maximumRotationError })),
        maximumOtherLocalPositionChange: maximumOtherLocalChange, distinctNativeFootPoses: footSignatures.size, immediateFallbacks: fallback });
      steady.dispose(); skate.restoreBase(); skate.dispose(); group.stop();
    }
    for (const { node, parent, scale } of original) {
      assert.equal(node.parent, parent);
      node.scaling.asArray().forEach((value, index) => assert.ok(Math.abs(value - scale[index]) < 2e-6, 'Only original native scale-channel roundoff is allowed'));
    }
    forms.push({ form, originalNodes: nodes.length, originalHierarchyAndScalesPreserved: true, trials });
    for (const shoe of shoes) { shoe.prop.dispose(); shoe.root.dispose(); }
    entries.dispose(); visual.dispose(); actor.dispose();
  }
  const unsupportedActor = new TransformNode('unsupported-actor', scene), unsupportedVisual = new TransformNode('unsupported-visual', scene);
  unsupportedVisual.parent = unsupportedActor;
  const unsupported = createSteadyHead({ visual: unsupportedVisual });
  unsupported.update(1 / 60, ready); unsupported.apply(); unsupported.restoreBase(); unsupported.dispose(); unsupportedActor.dispose();
  const report = { result: 'PASS', sourceHashes, forms, sampledFrames: forms.reduce((sum, form) => sum + form.trials.reduce((count, trial) => count + trial.frames, 0), 0), unsupportedRigSafe: true,
    limits: 'CPU transform proof using actual three native Chaos rigs, original Idle/Walk/Run119, Shadow172–174, native mirrored boots and production foot-grounding. Actor motion/heading vary; flat ground only. Visible head016 and every actual head/eye mesh are measured from the first frame; native014/015 and all limb local channels remain untouched by the constraint. Rendered neck silhouette, other motions/gear and browser/service acceptance remain separate. Permanent on-foot head local position/rotation compensation is an explicit presentation adaptation; scales, source assets, parents and other local channels are unchanged. Mounted pilot placement bypasses the lock.' };
  await mkdir(dirname(output), { recursive: true }); await writeFile(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ result: report.result, sampledFrames: report.sampledFrames, forms: forms.map(form => ({ form: form.form, trials: form.trials })), output }));
} finally { scene.dispose(); engine.dispose(); }
