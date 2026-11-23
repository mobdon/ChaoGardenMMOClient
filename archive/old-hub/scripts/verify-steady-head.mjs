// Opt-in actual-native transform proof. No authority, credentials or saves are
// accessed. Original assets and generated evidence remain ignored/local.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LoadAssetContainerAsync, Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { createSteadyHead, HEAD_LOCK_SECONDS, SKATE_HEAD_BOB_AMPLITUDE } from '../src/chao-head.js';
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
const output = resolve(options.output || resolve(root, '.local/reviews/steady-head-gait-policy-proof.json'));
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
assert.equal(HEAD_LOCK_SECONDS, .18, 'Run lock transition contract changed');
assert.equal(SKATE_HEAD_BOB_AMPLITUDE, .002, 'Skate bob world-distance contract changed');
const dt = 1 / 60, positionTolerance = 2e-6, rotationToleranceDegrees = .01;
const modes = [['Idle', 0], ['Walk', .798], ['Run119', 1.236], ['Skate172', 1.55], ['Skate173', 1.8], ['Skate174', 1.994]];
const trialSpecs = [
  ...modes.map(([mode, speed]) => ({ mode, speed, actorScale: 1, frames: 660 })),
  ...[.6, 1.7].flatMap(actorScale => modes.slice(2).map(([mode, speed]) => ({ mode, speed, actorScale, frames: 90 }))),
  { mode: 'Run-to-Walk-to-Idle', actorScale: 1, frames: 90, transition: true },
];
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
    const nodes = visual.getDescendants(), head = pose.get('016')?.node, feet = ['006', '013'].map(index => pose.get(index)?.node);
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
    assert.ok(idle && walk);
    const trials = [];
    for (const spec of trialSpecs) {
      const { mode, actorScale, frames } = spec;
      actor.position.setAll(0); actor.rotation.setAll(0); actor.scaling.setAll(actorScale); visual.position.setAll(0);
      for (const entry of pose.values()) { entry.node.position.copyFrom(entry.position); entry.node.rotationQuaternion.copyFrom(entry.quaternion); }
      const steady = createSteadyHead(imported), skate = createShadowSkate(imported, { motion });
      const anchor = localPosition(head, actor), anchorRotation = localRotation(head, actor);
      const visibleHead = head.getChildMeshes().filter(mesh => mesh.getTotalVertices()).map(mesh => {
        mesh.computeWorldMatrix(true);
        const center = mesh.getBoundingInfo().boundingBox.center.clone();
        const actorCenter = Vector3.TransformCoordinates(Vector3.TransformCoordinates(center, mesh.getWorldMatrix()), Matrix.Invert(actor.getWorldMatrix()));
        return { mesh, center, actorCenter, actorRotation: localRotation(mesh, actor), maximumSettledCenterError: 0, maximumSettledRotationError: 0 };
      });
      const before = [], after = [], footSignatures = new Set(), policyModes = new Set(), phaseSamples = new Set();
      let maximumExpectedWorldPositionError = 0, maximumSettledRotationError = 0, maximumOtherLocalChange = 0;
      let maximumObservedWorldBob = 0, maximumBobPhaseError = 0, naturalFrames = 0, transitionFrames = 0, settledLockFrames = 0;
      let progress = 0, group = null, basePosition, baseRotation, lastGait;
      for (let frame = 0; frame < frames; frame++) {
        const frameMode = spec.transition ? frame < 30 ? 'Run119' : frame < 60 ? 'Walk' : 'Idle' : mode;
        const speed = spec.transition ? modes.find(([name]) => name === frameMode)[1] : spec.speed;
        const gait = frameMode === 'Idle' ? 'Idle' : frameMode === 'Walk' ? 'Walk' : 'Run';
        const skating = frameMode.startsWith('Skate');
        steady.restoreBase(); skate.restoreBase(); visual.position.setAll(0);
        const nextGroup = gait === 'Idle' ? idle : gait === 'Walk' ? walk : run;
        if (group !== nextGroup) {
          group?.stop(); group = nextGroup; group.enableBlending = false;
          group.start(true, nativeGaitRate(gait, speed));
        }
        // An actor-local anchor must follow the player's real heading/position.
        actor.rotation.y = Math.sin(frame / 90) * .8; actor.position.set(frame / 1000, .05 * Math.sin(frame / 50), -.2);
        group.goToFrame(group.from + ((frame / 60) * 60 * group.speedRatio) % (group.to - group.from));
        skate.update(dt, { speed, grounded: true, footwear: skating, mounted: false, swimming: false, fresh: true });
        if (skating) skate.apply();
        ground(imported, actor.position.y, actor.scaling.y);
        // Read the cadence after native skate application advances it. Head
        // motion must follow that source phase, not an unrelated frame clock.
        const { blend: skateBlend, phase: skatePhase } = skate.diagnostics;
        steady.update(dt, { gait, mounted: false, skateBlend, skatePhase });
        const target = gait === 'Run' || skateBlend > 0 ? 1 : 0;
        progress += Math.sign(target - progress) * Math.min(Math.abs(target - progress), dt / .18);
        const expectedBlend = progress * progress * (3 - 2 * progress);
        const expectedRawWorldBob = .002 * skateBlend * Math.sin(2 * Math.PI * skatePhase);
        const diagnostics = steady.diagnostics;
        assert.ok(Math.abs(diagnostics.blend - expectedBlend) < 1e-12, `${form}/${mode} .18-second lock easing diverged`);
        assert.ok(Math.abs(diagnostics.bobAmplitude - .002 * skateBlend * expectedBlend) < 1e-12, 'Bob amplitude is not a world distance');
        assert.ok(Math.abs(diagnostics.bobOffsetY - expectedRawWorldBob * expectedBlend) < 1e-12, 'Bob diagnostics do not follow native cadence');
        policyModes.add(diagnostics.mode); lastGait = gait;
        if (skating) phaseSamples.add(skatePhase.toFixed(6));
        basePosition = head.position.clone(); baseRotation = head.rotationQuaternion.clone();
        const baseline = original.map(({ node }) => ({ p: node.position.clone(), q: node.rotationQuaternion?.clone(), s: node.scaling.clone() }));
        const footBefore = feet.map(foot => localPosition(foot, actor));
        const nativeHead = localPosition(head, actor), nativeWorld = head.getAbsolutePosition().clone();
        const worldAnchor = Vector3.TransformCoordinates(anchor, actor.getWorldMatrix());
        const expectedWorld = Vector3.Lerp(nativeWorld, worldAnchor.add(new Vector3(0, expectedRawWorldBob, 0)), expectedBlend);
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
        maximumExpectedWorldPositionError = Math.max(maximumExpectedWorldPositionError, Vector3.Distance(head.getAbsolutePosition(), expectedWorld));
        if (expectedBlend === 0) {
          naturalFrames++;
          assert.deepEqual(head.position.asArray(), basePosition.asArray(), `${form}/${mode} natural head position changed`);
          assert.deepEqual(head.rotationQuaternion.asArray(), baseRotation.asArray(), `${form}/${mode} natural head rotation changed`);
        } else if (expectedBlend === 1) {
          settledLockFrames++;
          const actualWorldBob = head.getAbsolutePosition().subtract(worldAnchor);
          maximumObservedWorldBob = Math.max(maximumObservedWorldBob, Math.abs(actualWorldBob.y));
          maximumBobPhaseError = Math.max(maximumBobPhaseError, Math.abs(actualWorldBob.y - expectedRawWorldBob));
          assert.ok(Math.abs(actualWorldBob.x) < positionTolerance && Math.abs(actualWorldBob.z) < positionTolerance, 'Settled head drifts horizontally');
          maximumSettledRotationError = Math.max(maximumSettledRotationError, angle(localRotation(head, actor), anchorRotation));
          for (const part of visibleHead) {
            part.mesh.computeWorldMatrix(true);
            const actual = Vector3.TransformCoordinates(Vector3.TransformCoordinates(part.center, part.mesh.getWorldMatrix()), Matrix.Invert(actor.getWorldMatrix()));
            const expected = part.actorCenter.add(new Vector3(0, expectedRawWorldBob / actorScale, 0));
            part.maximumSettledCenterError = Math.max(part.maximumSettledCenterError, Vector3.Distance(actual, expected));
            part.maximumSettledRotationError = Math.max(part.maximumSettledRotationError, angle(localRotation(part.mesh, actor), part.actorRotation));
          }
        } else {
          transitionFrames++;
        }
      }
      assert.ok(maximumExpectedWorldPositionError < positionTolerance, `${form}/${mode} head policy world error: ${maximumExpectedWorldPositionError}`);
      assert.ok(maximumSettledRotationError < rotationToleranceDegrees, `${form}/${mode} settled orientation error: ${maximumSettledRotationError}`);
      assert.ok(maximumObservedWorldBob <= .002 + positionTolerance, `${form}/${mode} skate bob exceeded .002 world units`);
      assert.ok(maximumBobPhaseError < positionTolerance, `${form}/${mode} bob did not follow native cadence`);
      for (const part of visibleHead) {
        assert.ok(part.maximumSettledCenterError < positionTolerance, `${form}/${mode} settled head/eye mesh ${part.mesh.name} position error: ${part.maximumSettledCenterError}`);
        assert.ok(part.maximumSettledRotationError < rotationToleranceDegrees, `${form}/${mode} settled head/eye mesh ${part.mesh.name} orientation error: ${part.maximumSettledRotationError}`);
      }
      if (mode === 'Idle' || mode === 'Walk') {
        assert.equal(naturalFrames, frames, 'Idle/Walk did not remain entirely native');
        assert.deepEqual(range(after), range(before), 'Natural native head range changed');
        assert.ok(Math.max(...range(before)) > 1e-5, 'The native head motion was not exercised');
      } else assert.ok(settledLockFrames > 0 && transitionFrames > 0, 'Run/skate transition was not exercised');
      if (spec.transition) {
        assert.ok(policyModes.has('blending-out') && policyModes.has('natural'), 'Exiting Run did not restore natural Walk/Idle');
        assert.ok(naturalFrames > 30, 'Natural Walk and Idle restoration was not exercised');
      }
      if (mode.startsWith('Skate')) assert.ok(phaseSamples.size > 30 && maximumObservedWorldBob > .001, 'Native skate bob cadence was not exercised');
      if (mode !== 'Idle') assert.ok(footSignatures.size > (frames === 660 ? 30 : 12), 'Original limb strokes became static');
      steady.restoreBase();
      assert.ok(Vector3.Distance(head.position, basePosition) < 1e-9, 'Original local head position did not restore');
      assert.ok(angle(head.rotationQuaternion, baseRotation) < .0001, 'Original local head orientation did not restore');
      // Reapply the live overlay before mounting: update itself must remove it
      // immediately, rather than relying on a prior explicit restoration.
      steady.apply();
      steady.update(dt, { gait: lastGait, mounted: true }); steady.apply();
      assert.deepEqual(head.position.asArray(), basePosition.asArray(), 'Mounted state retained head-position overlay');
      assert.deepEqual(head.rotationQuaternion.asArray(), baseRotation.asArray(), 'Mounted state retained head-rotation overlay');
      assert.equal(steady.diagnostics.blend, 0, 'Mounted state retained head blend');
      trials.push({ mode, speed: spec.speed ?? null, actorScale, frames, naturalFrames, transitionFrames, settledLockFrames,
        nativeHeadPositionRangeActorUnits: range(before), presentedHeadPositionRangeActorUnits: range(after),
        maximumExpectedWorldPositionError, maximumSettledRotationErrorDegrees: maximumSettledRotationError,
        maximumObservedWorldBob, maximumBobPhaseError, distinctNativeSkatePhases: phaseSamples.size, policyModes: [...policyModes],
        visibleHeadAndEyeMeshes: visibleHead.map(({ mesh, maximumSettledCenterError, maximumSettledRotationError }) => ({ mesh: mesh.name, nativeParent: mesh.parent.name, maximumSettledCenterError, maximumSettledRotationErrorDegrees: maximumSettledRotationError })),
        maximumOtherLocalPositionChange: maximumOtherLocalChange, distinctNativeFootPoses: footSignatures.size, immediateMountedBypass: true, exactNativeLocalRestoration: true });
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
  unsupported.update(dt, { gait: 'Run', skateBlend: 1, skatePhase: .25 }); unsupported.apply(); unsupported.restoreBase();
  assert.equal(unsupported.diagnostics.available, false); assert.equal(unsupported.diagnostics.blend, 0);
  unsupported.dispose(); unsupportedActor.dispose();
  const report = { result: 'PASS', sourceHashes, forms, sampledFrames: forms.reduce((sum, form) => sum + form.trials.reduce((count, trial) => count + trial.frames, 0), 0), unsupportedRigSafe: true,
    policy: { idleAndWalk: 'exact native local head motion', run: 'smoothstep lock after .18 seconds', skate: 'native cadence-phase vertical bob, at most .002 world units', actorUniformScales: [.6, 1, 1.7], positionTolerance, rotationToleranceDegrees },
    limits: 'CPU transform proof using actual three native Chaos rigs, original Idle/Walk/Run119, Shadow172–174, native mirrored boots and production foot-grounding. Actor position/heading and uniform scale vary; flat ground only. Idle/Walk preserve exact native head local channels. Run lock entry and Run-to-Walk-to-Idle exit use a .18-second smoothstep transition; only settled lock frames constrain visible head016 and every actual head/eye mesh to their actor anchor plus native skate-phase bob. The bob is bounded in world units at all tested actor scales. Native014/015 and all limb local channels remain untouched. Rendered neck silhouette, nonuniform/tilted actors, other motions/gear and browser/service acceptance remain separate. Head compensation and skate bob are explicit presentation adaptations; source assets, parents and scale channels remain unchanged. Mounted pilot placement immediately restores the native head.' };
  await mkdir(dirname(output), { recursive: true }); await writeFile(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ result: report.result, sampledFrames: report.sampledFrames, forms: forms.map(form => ({ form: form.form, trials: form.trials })), output }));
} finally { scene.dispose(); engine.dispose(); }
