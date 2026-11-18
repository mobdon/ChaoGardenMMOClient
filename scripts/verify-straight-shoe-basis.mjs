// Opt-in proof against locally supplied, converted native assets. Example:
// node scripts/verify-straight-shoe-basis.mjs --asset-root C:/private/.local/imports --source-root C:/private/.local/extracted/shadow-skate/teriosmdl
// CHAO_BASIS_ASSET_ROOT and CHAO_BASIS_SOURCE_ROOT provide the same directories.
// No service, credential, profile or save is accessed. This is a transform
// proof; browser silhouette, materials and terrain contact need separate QA.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  LoadAssetContainerAsync, Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3,
} from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { CHAO_GEAR_JOINTS, collectNativeShoeParts, nativeShoeEffect } from '../src/chao-attachments.js';
import { attachNativeRun, captureChaoPose } from '../src/chao-locomotion.js';
import { createShadowSkate } from '../src/shadow-skate.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const options = {};
for (let index = 2; index < process.argv.length; index += 2) {
  const flag = process.argv[index], value = process.argv[index + 1];
  assert.ok(['--asset-root', '--source-root', '--output'].includes(flag) && value && !value.startsWith('--'),
    'Use --asset-root <GLB/JSON directory>, --source-root <native .sa2mdl directory>, or --output <JSON file>.');
  options[flag.slice(2)] = value;
}
const assetRoot = resolve(options['asset-root'] || process.env.CHAO_BASIS_ASSET_ROOT || resolve(root, '.local/imports'));
const sourceRoot = resolve(options['source-root'] || process.env.CHAO_BASIS_SOURCE_ROOT
  || resolve(root, '.local/extracted/shadow-skate/teriosmdl'));
const output = resolve(options.output || resolve(root, '.local/reviews/straight-shoe-basis-proof.json'));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const mirrored = vector => new Vector3(-vector.x, vector.y, vector.z);
const degrees = radians => radians * 180 / Math.PI;
const angle = (a, b) => degrees(Math.acos(Math.max(-1, Math.min(1,
  Vector3.Dot(a.normalizeToNew(), b.normalizeToNew())))));
const find = (nodes, index) => nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));

// Exercise the actual production function, not a copied pair of constants.
// It is a self-contained presentation function with these three dependencies.
const sceneSource = await readFile(resolve(root, 'src/scene.js'), 'utf8');
const functionStart = sceneSource.indexOf('  function gearMountRotation(');
const functionEnd = sceneSource.indexOf('\n  async function attachOwnerGear', functionStart);
assert.ok(functionStart >= 0 && functionEnd > functionStart, 'Production gearMountRotation must be located explicitly.');
const productionMount = new Function('Vector3', 'Matrix', 'Quaternion',
  `return (${sceneSource.slice(functionStart, functionEnd).trim()});`)(Vector3, Matrix, Quaternion);

function referenceMount(up, forward) {
  up = mirrored(up).normalize(); forward = mirrored(forward).normalize();
  const basis = Matrix.Identity();
  Matrix.FromXYZAxesToRef(Vector3.Cross(up, forward).normalize(), up, forward, basis);
  return Quaternion.FromRotationMatrix(basis).conjugate().normalize();
}

function geometryBounds(meshes) {
  assert.ok(meshes.length, 'Native boot fitting requires original boot geometry.');
  let min = new Vector3(Infinity, Infinity, Infinity), max = min.negate();
  for (const mesh of meshes) {
    mesh.computeWorldMatrix(true);
    const positions = mesh.getVerticesData('position');
    assert.ok(positions?.length, 'Boot mesh must retain source positions.');
    for (let index = 0; index < positions.length; index += 3) {
      const point = Vector3.TransformCoordinates(Vector3.FromArray(positions, index), mesh.getWorldMatrix());
      min = Vector3.Minimize(min, point); max = Vector3.Maximize(max, point);
    }
  }
  assert.ok(max.z > min.z, 'Native boot must have measurable depth.');
  return { min, max, center: min.add(max).scale(.5), size: max.subtract(min) };
}

const sourceHashes = {}, assets = {}, references = {};
const report = {
  result: 'PENDING', sourceClientSha256: hash(sceneSource), sourceHashes, basis: [], rigs: [],
  limits: 'Native asset/rig transform proof. No game-service round trip, browser silhouette/material/grounding acceptance, native engine port or rollout is claimed.',
};
for (const file of ['src/chao-attachments.js', 'src/chao-locomotion.js', 'src/shadow-skate.js', 'scripts/verify-straight-shoe-basis.mjs']) {
  sourceHashes[file] = hash(await readFile(resolve(root, file)));
}
const engine = new NullEngine(), scene = new Scene(engine);
async function assetBytes(name) {
  const bytes = await readFile(resolve(assetRoot, `${name}.glb`));
  sourceHashes[name] = hash(bytes);
  return bytes;
}
async function load(bytes) {
  return LoadAssetContainerAsync(new Uint8Array(bytes), scene,
    { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
}

try {
  for (const side of ['left', 'right']) {
    const bytes = await assetBytes(`shadow-air-shoes-${side}`);
    assert.equal(bytes.readUInt32LE(0), 0x46546c67, 'Expected converted glTF binary.');
    const document = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString());
    const source = document.asset?.extras?.sa2Source, mounting = source?.mountingReference;
    assert.ok(mounting && source.baseBoot?.modelId === 65, 'Original footwear conversion metadata is required.');
    assert.equal(source.side, side);
    assert.equal(source.modelId, side === 'left' ? 79 : 78);
    const nativeBoot = await readFile(resolve(sourceRoot, '65.sa2mdl'));
    const nativeOverlay = await readFile(resolve(sourceRoot, `${source.modelId}.sa2mdl`));
    assert.equal(hash(nativeBoot), source.baseBoot.nativeSha256, 'Native base model65 must match conversion provenance.');
    assert.equal(hash(nativeOverlay), source.nativeSha256, 'Native overlay must match conversion provenance.');
    const rows = mounting.nativeHeelRestWorldBlenderMatrix;
    assert.ok(rows?.length === 4 && rows.every(row => row.length === 4 && row.every(Number.isFinite)));
    const nativeRotation = Matrix.FromArray([
      rows[0][0], rows[1][0], rows[2][0], 0,
      rows[0][1], rows[1][1], rows[2][1], 0,
      rows[0][2], rows[1][2], rows[2][2], 0,
      0, 0, 0, 1,
    ]);
    const local = Vector3.TransformNormal(Vector3.FromArray(mounting.nativeHeelToToeWorldBlender), Matrix.Invert(nativeRotation));
    const toe = new Vector3(local.x, local.z, -local.y);
    const up = Vector3.FromArray(mounting.exportedPropStandingUp).normalize();
    const forward = toe.subtract(up.scale(Vector3.Dot(toe, up))).normalize();
    const expected = referenceMount(up, forward), production = productionMount('shadow-air-shoes', side);
    const productionMatrix = Matrix.FromQuaternionToRef(production, Matrix.Identity());
    const nominal = referenceMount(up, Vector3.FromArray(mounting.exportedPropNominalForward));
    const oldToe = Vector3.TransformNormal(mirrored(toe), Matrix.FromQuaternionToRef(nominal, Matrix.Identity())).normalize();
    const actualToe = Vector3.TransformNormal(mirrored(toe), productionMatrix).normalize();
    const actualUp = Vector3.TransformNormal(mirrored(up), productionMatrix).normalize();
    const oldNominalYaw = degrees(Math.atan2(oldToe.x, oldToe.z));
    const productionYaw = degrees(Math.atan2(actualToe.x, actualToe.z));
    const rotationErrorDegrees = degrees(2 * Math.acos(Math.min(1, Math.abs(Quaternion.Dot(expected, production)))));
    assert.ok(Math.abs(oldNominalYaw) > 10 && Math.abs(oldNominalYaw) < 13,
      'Original nominal global-forward reference must reproduce the inward-toe regression.');
    assert.ok(rotationErrorDegrees < .0001, `${side}: actual production mounting must match native heel-to-toe correction.`);
    assert.ok(Math.abs(productionYaw) < .0001, `${side}: actual production toe projection must face forward.`);
    assert.ok(angle(actualUp, Vector3.Up()) < .0001, `${side}: actual production native up must remain upright.`);
    references[side] = { production, toe };
    assets[side] = await load(bytes);
    report.basis.push({ side, sourceModel65Sha256: hash(nativeBoot), sourceOverlaySha256: hash(nativeOverlay),
      oldNominalYaw, productionYaw, rotationErrorDegrees, upErrorDegrees: angle(actualUp, Vector3.Up()),
      exportLocalUp: up.asArray(), exportLocalToeForward: forward.asArray() });
  }
  const runSource = await load(await assetBytes('chao-locomotion'));
  const motionBytes = await readFile(resolve(assetRoot, 'shadow-skate-motion.json'));
  sourceHashes['shadow-skate-motion'] = hash(motionBytes);
  const motion = JSON.parse(motionBytes);
  for (const form of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const source = await load(await assetBytes(form));
    const entries = source.instantiateModelsToScene(name => `${form}-${name}`, false, { doNotInstantiate: true });
    const actor = new TransformNode(form, scene), visual = new TransformNode(`${form}-visual`, scene);
    visual.parent = actor; visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { entries, visual }, nodes = visual.getDescendants();
    const fixed = nodes.map(node => ({ node, parent: node.parent, scale: node.scaling.asArray() }));
    const run = attachNativeRun(imported, runSource, captureChaoPose(imported));
    const walk = entries.animationGroups.find(group => /Walk$/.test(group.name));
    assert.ok(run && walk, `${form}: native Walk and Run must be available.`);
    const shoes = [];
    for (const [side, , , socketIndex] of CHAO_GEAR_JOINTS) {
      const socket = find(nodes, socketIndex);
      assert.ok(socket, `${form}: original animated foot${socketIndex} must exist.`);
      socket.computeWorldMatrix(true);
      const rest = socket.getWorldMatrix().clone();
      const fit = geometryBounds(socket.getChildMeshes().filter(mesh => mesh.getTotalVertices()));
      const prop = assets[side].instantiateModelsToScene(name => `${form}-${name}`, false, { doNotInstantiate: true });
      const shoeRoot = new TransformNode(`${form}-shoe-${side}`, scene);
      for (const node of prop.rootNodes) node.parent = shoeRoot;
      // Every sample uses the actual extracted production rotation.
      shoeRoot.rotationQuaternion = references[side].production.clone();
      const { meshes, effects } = collectNativeShoeParts(shoeRoot);
      assert.ok(meshes.every(mesh => !nativeShoeEffect(mesh)), 'Embedded native jets must not inflate boot fitting.');
      assert.ok(effects.every(effect => !effect.mesh.isEnabled()), 'Bundled jets start disabled during fitting.');
      const sourceBounds = geometryBounds(meshes), scale = fit.size.z * 1.06 / sourceBounds.size.z;
      const position = fit.center.subtract(sourceBounds.center.scale(scale));
      position.y = fit.min.y - sourceBounds.min.y * scale;
      const local = Matrix.Compose(new Vector3(scale, scale, scale), shoeRoot.rotationQuaternion, position)
        .multiply(Matrix.Invert(rest));
      local.decompose(shoeRoot.scaling, shoeRoot.rotationQuaternion, shoeRoot.position);
      const magnitude = (Math.abs(shoeRoot.scaling.x) + Math.abs(shoeRoot.scaling.y) + Math.abs(shoeRoot.scaling.z)) / 3;
      shoeRoot.scaling.set(Math.sign(shoeRoot.scaling.x) * magnitude,
        Math.sign(shoeRoot.scaling.y) * magnitude, Math.sign(shoeRoot.scaling.z) * magnitude);
      shoeRoot.parent = socket; shoeRoot.computeWorldMatrix(true);
      shoes.push({ side, socket, root: shoeRoot, rest, restWorld: shoeRoot.getWorldMatrix().clone(),
        toe: mirrored(references[side].toe), prop, nativeEffectMeshesExcluded: effects.length });
    }
    const skate = createShadowSkate(imported, { motion });
    const clips = [];
    function sample() {
      let maximum = 0;
      for (const shoe of shoes) {
        shoe.socket.computeWorldMatrix(true); shoe.root.computeWorldMatrix(true);
        const delta = Matrix.Invert(shoe.rest).multiply(shoe.socket.getWorldMatrix());
        const expected = Vector3.TransformNormal(Vector3.TransformNormal(shoe.toe, shoe.restWorld), delta);
        const actual = Vector3.TransformNormal(shoe.toe, shoe.root.getWorldMatrix());
        const error = angle(expected, actual);
        maximum = Math.max(maximum, error);
        assert.ok(error < .005, 'Shoe must follow native foot delta without correcting original animation.');
        assert.equal(shoe.root.parent, shoe.socket);
        const scales = shoe.root.scaling.asArray().map(Math.abs);
        assert.ok(Math.max(...scales) - Math.min(...scales) < 1e-7, 'Prop scale must remain uniform.');
      }
      for (const { node, parent, scale } of fixed) {
        assert.equal(node.parent, parent, 'Original native parents must remain intact.');
        node.scaling.asArray().forEach((value, axis) => assert.ok(Math.abs(value - scale[axis]) < 2e-6,
          'Only original float32 scale-track rounding is allowed.'));
      }
      return maximum;
    }
    for (const group of [walk, run]) {
      group.enableBlending = false; group.start(true, 1);
      let maximum = 0;
      for (let frame = 0; frame <= 120; frame++) {
        group.goToFrame(group.from + (group.to - group.from) * frame / 120);
        maximum = Math.max(maximum, sample());
      }
      group.stop();
      clips.push({ clip: group.name, samples: 121, maxNativeDeltaErrorDegrees: maximum });
    }
    let maximum = 0;
    run.start(true, 1);
    for (let frame = 0; frame < 120; frame++) {
      skate.restoreBase();
      run.goToFrame(run.from + (run.to - run.from) * (frame % 60) / 60);
      skate.update(1 / 60, { speed: 1.994, grounded: true, footwear: true, mounted: false, swimming: false, fresh: true });
      skate.apply();
      maximum = Math.max(maximum, sample());
    }
    assert.ok(skate.diagnostics.active, 'Actual Shadow overlay must be active for skate samples.');
    skate.restoreBase(); skate.dispose(); run.stop();
    clips.push({ clip: 'ShadowSkate', samples: 120, maxNativeDeltaErrorDegrees: maximum });
    report.rigs.push({ form, originalNodes: fixed.length, nativeEffectMeshesExcluded: shoes.map(shoe => ({
      side: shoe.side, meshes: shoe.nativeEffectMeshesExcluded,
    })), clips });
    for (const shoe of shoes) { shoe.prop.dispose(); shoe.root.dispose(); }
    entries.dispose(); actor.dispose(); source.dispose();
  }
  report.samples = report.rigs.reduce((sum, rig) => sum + rig.clips.reduce((count, clip) => count + clip.samples, 0), 0);
  assert.equal(report.samples, 1086);
  report.result = 'PASS';
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ result: report.result, forms: report.rigs.length, samples: report.samples,
    productionYawDegrees: report.basis.map(row => ({ side: row.side, yaw: row.productionYaw })), output }));
} finally {
  engine.dispose();
}
