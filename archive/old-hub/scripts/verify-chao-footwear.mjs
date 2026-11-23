import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LoadAssetContainerAsync, Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { CHAO_GEAR_JOINTS, CHAO_SHOE_LENGTH_RATIO, CHAO_SHOE_REFERENCE_SIDE, attachRigidGearAtRest, chestSurfacePoint, collectNativeShoeParts, mirroredShadowShoeRest, shadowShoeMountRotation } from '../src/chao-attachments.js';
import { attachNativeRun, captureChaoPose } from '../src/chao-locomotion.js';
import { createShadowSkate } from '../src/shadow-skate.js';

// Opt-in proof against ignored, locally converted retail assets. This probes
// original rigs and rigid shoe trajectories; browser/effect QA is separate.
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const options = {};
for (let index = 2; index < process.argv.length; index += 2) {
  const flag = process.argv[index], value = process.argv[index + 1];
  assert.ok(['--asset-root', '--shoe-root', '--source-root', '--output'].includes(flag) && value && !value.startsWith('--'),
    'Use --asset-root <asset directory>, --shoe-root <prepared shoes directory>, or --output <JSON file>.');
  options[flag.slice(2)] = value;
}
const assetRoot = resolve(options['asset-root'] || process.env.CHAO_FOOTWEAR_ASSET_ROOT || resolve(root, '.local/imports'));
const shoeRoot = resolve(options['shoe-root'] || process.env.CHAO_FOOTWEAR_SHOE_ROOT || assetRoot);
const nativeRoot = resolve(options['source-root'] || process.env.CHAO_BASIS_SOURCE_ROOT || resolve(root, '.local/extracted/shadow-skate/teriosmdl'));
const output = resolve(options.output || resolve(root, '.local/reviews/chao-footwear-proof.json'));
const engine = new NullEngine(), scene = new Scene(engine);
const sourceHashes = {};
for (const file of ['src/scene.js', 'src/chao-attachments.js', 'src/chao-locomotion.js', 'src/shadow-skate.js', 'scripts/verify-chao-footwear.mjs']) {
  sourceHashes[file] = createHash('sha256').update(await readFile(resolve(root, file))).digest('hex');
}
async function load(name) {
  const bytes = await readFile(resolve(name.startsWith('shadow-air-shoes-') ? shoeRoot : assetRoot, `${name}.glb`));
  sourceHashes[name] = createHash('sha256').update(bytes).digest('hex');
  return LoadAssetContainerAsync(new Uint8Array(bytes), scene,
    { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
}
const find = (nodes, index) => nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));
function geometryVertices(meshes, toRoot = Matrix.Identity()) {
  const points = [];
  for (const mesh of meshes) {
    mesh.computeWorldMatrix(true);
    const matrix = mesh.getWorldMatrix().multiply(toRoot), positions = mesh.getVerticesData('position');
    for (let i = 0; i < positions.length; i += 3) {
      const point = Vector3.TransformCoordinates(Vector3.FromArray(positions, i), matrix);
      points.push(point);
    }
  }
  return points;
}
function geometryBounds(meshes, toRoot = Matrix.Identity()) {
  let min = new Vector3(Infinity, Infinity, Infinity), max = min.negate();
  for (const point of geometryVertices(meshes, toRoot)) {
    min = Vector3.Minimize(min, point); max = Vector3.Maximize(max, point);
  }
  return { min, max, size: max.subtract(min), center: min.add(max).scale(.5) };
}
function inBody(point, parent, body) {
  parent.computeWorldMatrix(true); body.computeWorldMatrix(true);
  return Vector3.TransformCoordinates(point, parent.getWorldMatrix().multiply(Matrix.Invert(body.getWorldMatrix())));
}
function statistics(samples) {
  return Object.fromEntries(['x', 'y', 'z'].map(axis => {
    const columns = [0, 1].map(side => samples.map(sample => sample[side][axis]));
    const means = columns.map(values => values.reduce((a, b) => a + b) / values.length);
    let ab = 0, aa = 0, bb = 0;
    for (let i = 0; i < samples.length; i++) {
      const a = columns[0][i] - means[0], b = columns[1][i] - means[1];
      ab += a * b; aa += a * a; bb += b * b;
    }
    return [axis, { ranges: columns.map(values => Math.max(...values) - Math.min(...values)),
      correlation: aa * bb > 1e-16 ? ab / Math.sqrt(aa * bb) : null }];
  }));
}
const forms = [];
try {
  const canonicalBytes = await readFile(resolve(shoeRoot, `shadow-air-shoes-${CHAO_SHOE_REFERENCE_SIDE}.glb`));
  const canonicalDocument = JSON.parse(canonicalBytes.subarray(20, 20 + canonicalBytes.readUInt32LE(12)));
  const canonicalSource = canonicalDocument.asset.extras.sa2Source, mounting = canonicalSource.mountingReference;
  assert.equal(canonicalSource.side, 'right'); assert.equal(canonicalSource.modelId, 78);
  for (const [id, expected] of [[65, canonicalSource.baseBoot.nativeSha256], [78, canonicalSource.nativeSha256]]) {
    const actual = createHash('sha256').update(await readFile(resolve(nativeRoot, `${id}.sa2mdl`))).digest('hex');
    assert.equal(actual, expected); sourceHashes[`native-model-${id}`] = actual;
  }
  const rows = mounting.nativeHeelRestWorldBlenderMatrix;
  const nativeRotation = Matrix.FromArray([
    rows[0][0], rows[1][0], rows[2][0], 0, rows[0][1], rows[1][1], rows[2][1], 0,
    rows[0][2], rows[1][2], rows[2][2], 0, 0, 0, 0, 1,
  ]);
  const localToe = Vector3.TransformNormal(Vector3.FromArray(mounting.nativeHeelToToeWorldBlender), Matrix.Invert(nativeRotation));
  const nativeToe = new Vector3(-localToe.x, localToe.z, -localToe.y);
  const nativeUp = Vector3.FromArray(mounting.exportedPropStandingUp); nativeUp.x *= -1;
  const productionRotation = Matrix.FromQuaternionToRef(shadowShoeMountRotation(), Matrix.Identity());
  const transformedUp = Vector3.TransformNormal(nativeUp, productionRotation).normalize();
  const transformedToe = Vector3.TransformNormal(nativeToe, productionRotation).normalize();
  const productionYawDegrees = Math.atan2(transformedToe.x, transformedToe.z) * 180 / Math.PI;
  assert.ok(Vector3.Distance(transformedUp, Vector3.Up()) < 1e-5);
  assert.ok(Math.abs(productionYawDegrees) < .0001);
  const canonicalBasis = { sourceSide: 'right', nativeModel: 78, productionYawDegrees,
    originalNativeUpPreserved: true, adaptation: 'Same original right boot mirrored rigidly for the left side; the original left mesh/model79 is not used.' };
  const locomotion = await load('chao-locomotion');
  const propSource = await load(`shadow-air-shoes-${CHAO_SHOE_REFERENCE_SIDE}`);
  const motionBytes = await readFile(resolve(assetRoot, 'shadow-skate-motion.json'));
  sourceHashes['shadow-skate-motion'] = createHash('sha256').update(motionBytes).digest('hex');
  const skateMotion = JSON.parse(motionBytes);
  for (const name of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const asset = await load(name), entries = asset.instantiateModelsToScene(value => `${name}-${value}`, false, { doNotInstantiate: true });
    const actor = new TransformNode(`${name}-actor`, scene), visual = new TransformNode(`${name}-visual`, scene);
    visual.parent = actor; visual.scaling.setAll(.1);
    for (const root of entries.rootNodes) root.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { visual, entries }, pose = captureChaoPose(imported);
    const nodes = visual.getDescendants(), body = find(nodes, '001');
    const fixed = nodes.map(node => ({ node, parent: node.parent, scale: node.scaling.asArray() }));
    const shoes = [];
    let mirroredRest;
    actor.computeWorldMatrix(true);
    const toRoot = Matrix.Invert(actor.getWorldMatrix());
    const torsoMeshes = body.getChildren().filter(node => /(?:^|-)attach_/.test(node.name))
      .flatMap(node => [node, ...node.getChildMeshes()]).filter(mesh => mesh.getTotalVertices?.());
    const torsoBounds = geometryBounds(torsoMeshes, toRoot), skin = chestSurfacePoint(torsoMeshes, toRoot, torsoBounds);
    assert.ok(skin.asArray().every(Number.isFinite));
    assert.ok(skin.z <= torsoBounds.max.z + 1e-6);
    for (const [side, , , socketIndex, meshIndex] of CHAO_GEAR_JOINTS) {
      const socket = find(nodes, socketIndex), meshNode = find(nodes, meshIndex);
      socket.computeWorldMatrix(true);
      const nativeMeshes = meshNode.getChildMeshes().filter(mesh => mesh.getTotalVertices());
      const fit = geometryBounds(nativeMeshes, toRoot), restToRoot = socket.getWorldMatrix().multiply(toRoot);
      const prop = propSource.instantiateModelsToScene(value => `${name}-shoe-${side}-${value}`, false, { doNotInstantiate: true });
      const root = new TransformNode(`${name}-shoe-${side}`, scene);
      for (const node of prop.rootNodes) node.parent = root;
      root.rotationQuaternion = shadowShoeMountRotation();
      const { meshes, effects } = collectNativeShoeParts(root), source = geometryBounds(meshes);
      root.computeWorldMatrix(true);
      const marker = Vector3.TransformCoordinates(source.center, Matrix.Invert(root.getWorldMatrix()));
      if (side === CHAO_SHOE_REFERENCE_SIDE) mirroredRest = mirroredShadowShoeRest(source, fit);
      assert.ok(mirroredRest, 'Reference shoe must be fitted before its mirror.');
      attachRigidGearAtRest(root, { socket, restToRoot }, mirroredRest[side]);
      root.computeWorldMatrix(true);
      const restCenter = Vector3.TransformCoordinates(marker, root.getWorldMatrix());
      // Track the footbed at the native foot's center as well as the shell's
      // geometry center. A tall boot center describes a different pitch arc;
      // the contact/stride marker must preserve the original alternating feet.
      const footbedWorld = Vector3.TransformCoordinates(fit.center, actor.getWorldMatrix());
      const footbed = Vector3.TransformCoordinates(footbedWorld, Matrix.Invert(root.getWorldMatrix()));
      const oldSocket = find(nodes, side === 'right' ? '005' : '012'); oldSocket.computeWorldMatrix(true);
      const oldMarker = Vector3.TransformCoordinates(footbedWorld, Matrix.Invert(oldSocket.getWorldMatrix()));
      assert.ok(restCenter.asArray().every(Number.isFinite));
      shoes.push({ side, root, marker, footbed, oldSocket, oldMarker, nativeMeshes, prop, meshes, effects,
        restMatrix: root.getWorldMatrix().multiply(toRoot).asArray() });
      for (const mesh of nativeMeshes) mesh.setEnabled(false);
    }
    const mirrorErrors = {};
    for (const part of ['boot', 'nativeJets']) {
      const vertices = shoes.map(shoe => geometryVertices(part === 'boot' ? shoe.meshes : shoe.effects.map(effect => effect.mesh), toRoot));
      assert.equal(vertices[0].length, vertices[1].length, 'Mirrored shoes retain matching native topology.');
      const maximum = Math.max(0, ...vertices[0].map((point, index) => Vector3.Distance(new Vector3(-point.x, point.y, point.z), vertices[1][index])));
      assert.ok(maximum < 2e-6, `${name}: every original ${part} vertex must be mirrored at rest (${maximum}).`);
      mirrorErrors[part] = { vertices: vertices[0].length, maximumWorldError: maximum };
    }
    const restMirror = { sourceSide: CHAO_SHOE_REFERENCE_SIDE, sourceAsset: `shadow-air-shoes-${CHAO_SHOE_REFERENCE_SIDE}`,
      lengthRatio: CHAO_SHOE_LENGTH_RATIO, scale: mirroredRest.scale, mirrorErrors,
      mounts: shoes.map(shoe => ({ side: shoe.side, slot: 'footwear', asset: `shadow-air-shoes-${CHAO_SHOE_REFERENCE_SIDE}`,
        matrix: shoe.restMatrix, mirrored: shoe.side !== CHAO_SHOE_REFERENCE_SIDE,
        nativeEffectIds: shoe.effects.map(effect => effect.modelId) })) };
    const run = attachNativeRun(imported, locomotion, pose), walk = entries.animationGroups.find(group => /Walk$/.test(group.name));
    assert.ok(run && walk);
    const gaitResults = [];
    for (const group of [walk, run]) {
      group.enableBlending = false; group.start(true, 1);
      const samples = [], centerSamples = [], oldSamples = [];
      for (let frame = 0; frame <= 120; frame++) {
        group.goToFrame(group.from + (group.to - group.from) * frame / 120);
        samples.push(shoes.map(shoe => inBody(shoe.footbed, shoe.root, body)));
        centerSamples.push(shoes.map(shoe => inBody(shoe.marker, shoe.root, body)));
        oldSamples.push(shoes.map(shoe => inBody(shoe.oldMarker, shoe.oldSocket, body)));
        fixed.forEach(({ node, parent, scale }) => {
          assert.equal(node.parent, parent);
          // Retail Walk contains near-one scale channels differing from bind
          // by float32 rounding. Keep that native data; reject any deformation.
          node.scaling.asArray().forEach((value, axis) => assert.ok(Math.abs(value - scale[axis]) < 2e-6));
        });
        shoes.forEach(({ root }) => {
          const sizes = root.scaling.asArray().map(Math.abs);
          assert.ok(Math.max(...sizes) - Math.min(...sizes) < 1e-7, 'prop scale remains uniform');
        });
      }
      const correct = statistics(samples), old = statistics(oldSamples);
      assert.ok(correct.z.ranges.every(range => range > .04), `${name} ${group.name}: shoes must move fore/aft`);
      // Native Run swings each foot through a pitch arc; z alone can correlate
      // even when opposite feet lift alternately. Vertical lift proves phase.
      assert.ok(correct.y.ranges.every(range => range > .1) && correct.y.correlation < -.5,
        `${name} ${group.name}: opposite feet must lift alternately: ${JSON.stringify(correct)}`);
      assert.ok(Object.values(old).every(axis => axis.ranges.every(range => range < 1e-4)), 'old pivots reproduce the frozen-shoe regression');
      gaitResults.push({ clip: group.name, samples: samples.length, nativeFootbedOnShoe: correct,
        shoeShellCenters: statistics(centerSamples), oldStaticSockets: old });
      group.stop();
    }
    // This overlay runs on the same actual native nodes. Preserve every native
    // translation, hierarchy and scale while sampling its shoe articulation.
    const skate = createShadowSkate(imported, { motion: skateMotion });
    const translations = nodes.map(node => node.position.asArray());
    const skateSamples = [];
    for (let frame = 0; frame < 180; frame++) {
      skate.restoreBase();
      skate.update(1 / 60, { speed: 1.994, grounded: true, footwear: true, mounted: false, swimming: false, fresh: true });
      skate.apply(); skateSamples.push(shoes.map(shoe => inBody(shoe.marker, shoe.root, body)));
      fixed.forEach(({ node, parent, scale }, index) => {
        assert.equal(node.parent, parent);
        node.scaling.asArray().forEach((value, axis) => assert.ok(Math.abs(value - scale[axis]) < 2e-6));
        assert.deepEqual(node.position.asArray(), translations[index]);
      });
    }
    assert.equal(skate.diagnostics.blend, 1);
    const skateResult = statistics(skateSamples.slice(15));
    assert.ok(skateResult.z.ranges.every(range => range > .01), 'both shoes articulate under Shadow overlay');
    skate.restoreBase(); skate.dispose();
    forms.push({ name, originalNodes: nodes.length, originalParentsScalesPreserved: true, nativeScaleTolerance: 2e-6,
      gaitResults, skate: { samples: 165, ...skateResult, originalTranslationsPreserved: true },
      chest: { surface: skin.asArray(), aabbFront: torsoBounds.max.z } });
    forms.at(-1).restMirror = restMirror;
    shoes.forEach(shoe => { shoe.prop.dispose(); shoe.root.dispose(); });
    entries.dispose(); visual.dispose(); actor.dispose(); asset.dispose();
  }
  const report = { result: 'PASS', canonicalBasis, footwearSockets: CHAO_GEAR_JOINTS.map(([side, , , socket]) => ({ side, socket })), sourceHashes, forms,
    limits: 'NullEngine asset/rig proof; actual renderer, trusted snapshot path, heel effects and visual grounding require separate live verification.' };
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ result: report.result, footwearSockets: report.footwearSockets,
    forms: forms.map(form => ({ name: form.name, originalNodes: form.originalNodes,
      gaitResults: form.gaitResults.map(gait => ({ clip: gait.clip, samples: gait.samples,
        alternatingLiftCorrelation: gait.nativeFootbedOnShoe.y.correlation,
        shoeForeAftRanges: gait.shoeShellCenters.z.ranges,
        oldSocketMaximumRange: Math.max(...Object.values(gait.oldStaticSockets).flatMap(axis => axis.ranges)) })),
      skateSamples: form.skate.samples, chest: form.chest })),
    fullReport: output, limits: report.limits }, null, 2));
} finally { engine.dispose(); }
