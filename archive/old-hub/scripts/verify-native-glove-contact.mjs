// Opt-in proof using local native rigs/props. No service, save or credential
// is accessed. It proves rigid palm contact, not rendered silhouette quality.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LoadAssetContainerAsync, Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { CHAO_GEAR_JOINTS, attachRigidGearAtRest, chestSurfacePoint, shadowShoeMountRotation } from '../src/chao-attachments.js';
import { attachNativeRun, captureChaoPose } from '../src/chao-locomotion.js';
import { createShadowSkate } from '../src/shadow-skate.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..'), options = {};
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index], value = process.argv[index + 1];
  assert.ok(['--asset-root', '--output'].includes(key) && value && !value.startsWith('--'));
  options[key.slice(2)] = value;
}
const assetRoot = resolve(options['asset-root'] || resolve(root, '.local/imports'));
const output = resolve(options.output || resolve(root, '.local/reviews/native-glove-contact-proof.json'));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sourceHashes = {};
const engine = new NullEngine(), scene = new Scene(engine);
const source = await readFile(resolve(root, 'src/scene.js'), 'utf8');
sourceHashes['src/scene.js'] = hash(source);
for (const file of ['src/chao-attachments.js', 'src/chao-locomotion.js', 'src/shadow-skate.js', 'scripts/verify-native-glove-contact.mjs']) {
  sourceHashes[file] = hash(await readFile(resolve(root, file)));
}
function geometryPoints(meshes, toLocal = Matrix.Identity()) {
  const points = [];
  for (const mesh of meshes) {
    mesh.computeWorldMatrix(true);
    const positions = mesh.getVerticesData('position'), transform = mesh.getWorldMatrix().multiply(toLocal);
    for (let index = 0; index < positions.length; index += 3) points.push(Vector3.TransformCoordinates(Vector3.FromArray(positions, index), transform));
  }
  return points;
}
function boundsFor(points) {
  if (!points.length) return null;
  let min = new Vector3(Infinity, Infinity, Infinity), max = min.negate();
  for (const point of points) { min = Vector3.Minimize(min, point); max = Vector3.Maximize(max, point); }
  return { min, max, center: min.add(max).scale(.5), size: max.subtract(min) };
}
const captureStart = source.indexOf('  function captureGearSockets('), captureEnd = source.indexOf('\n  function gearMountRotation(', captureStart);
const mountEnd = source.indexOf('\n  async function attachOwnerGear', captureEnd);
assert.ok(captureStart >= 0 && captureEnd > captureStart && mountEnd > captureEnd);
const capture = new Function('CHAO_GEAR_JOINTS', 'Matrix', 'Vector3', 'geometryPoints', 'boundsFor', 'chestSurfacePoint',
  `return (${source.slice(captureStart, captureEnd).trim()});`)(CHAO_GEAR_JOINTS, Matrix, Vector3, geometryPoints, boundsFor, chestSurfacePoint);
const mount = new Function('Matrix', 'Vector3', 'Quaternion', 'shadowShoeMountRotation',
  `return (${source.slice(captureEnd, mountEnd).trim()});`)(Matrix, Vector3, Quaternion, shadowShoeMountRotation);
const find = (nodes, index) => nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));
async function load(name) {
  const bytes = await readFile(resolve(assetRoot, `${name}.glb`)); sourceHashes[name] = hash(bytes);
  return LoadAssetContainerAsync(new Uint8Array(bytes), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
}
const forms = [];
try {
  const locomotion = await load('chao-locomotion');
  const props = Object.fromEntries(await Promise.all(['right', 'left'].map(async side => [side, await load(`knuckles-hammer-gloves-${side}`)])));
  const motionBytes = await readFile(resolve(assetRoot, 'shadow-skate-motion.json'));
  sourceHashes['shadow-skate-motion'] = hash(motionBytes); const motion = JSON.parse(motionBytes);
  for (const form of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const asset = await load(form), entries = asset.instantiateModelsToScene(name => `${form}-${name}`, false, { doNotInstantiate: true });
    const actor = new TransformNode(`${form}-actor`, scene), visual = new TransformNode(`${form}-visual`, scene);
    visual.parent = actor; visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const imported = { visual, entries }, pose = captureChaoPose(imported), nodes = visual.getDescendants();
    const original = nodes.map(node => ({ node, parent: node.parent, scale: node.scaling.asArray() }));
    const item = { root: actor, imported }, fits = capture(item).hands;
    assert.equal(fits.length, 2);
    const gloves = [];
    for (const fit of fits) {
      const armIndex = fit.side === 'right' ? '003' : '010', tipIndex = fit.side === 'right' ? '004' : '011';
      const palm = find(nodes, armIndex), oldTip = find(nodes, tipIndex);
      assert.equal(fit.socket, palm, 'Production capture must attach to the visible native palm frame.');
      assert.ok(fit.meshes.every(mesh => mesh.parent === palm || mesh.isDescendantOf(palm)));
      assert.equal(oldTip.getChildMeshes().filter(mesh => mesh.getTotalVertices()).length, 0, 'Old tip socket has no visible hand mesh.');
      const prop = props[fit.side].instantiateModelsToScene(name => `${form}-${fit.side}-${name}`, false, { doNotInstantiate: true });
      const root = new TransformNode(`${form}-${fit.side}-glove`, scene);
      for (const node of prop.rootNodes) node.parent = root;
      root.rotationQuaternion = mount('knuckles-hammer-gloves', fit.side);
      const sourceBounds = boundsFor(geometryPoints(root.getChildMeshes().filter(mesh => mesh.getTotalVertices())));
      const scale = Math.max(...fit.bounds.size.asArray()) * 1.12 / Math.max(...sourceBounds.size.asArray());
      const position = fit.bounds.center.subtract(sourceBounds.center.scale(scale));
      attachRigidGearAtRest(root, fit, Matrix.Compose(new Vector3(scale, scale, scale), root.rotationQuaternion, position));
      root.computeWorldMatrix(true); palm.computeWorldMatrix(true); oldTip.computeWorldMatrix(true);
      const anchor = Vector3.TransformCoordinates(fit.bounds.center, actor.getWorldMatrix());
      const onPalm = Vector3.TransformCoordinates(anchor, Matrix.Invert(palm.getWorldMatrix()));
      const onGlove = Vector3.TransformCoordinates(anchor, Matrix.Invert(root.getWorldMatrix()));
      const onOldTip = Vector3.TransformCoordinates(anchor, Matrix.Invert(oldTip.getWorldMatrix()));
      gloves.push({ side: fit.side, root, palm, oldTip, prop, onPalm, onGlove, onOldTip,
        bindRelative: root.getWorldMatrix().multiply(Matrix.Invert(palm.getWorldMatrix())),
        maximumWorldContactDrift: 0, maximumOldSocketDrift: 0, maximumRelativeMatrixError: 0 });
    }
    const walk = entries.animationGroups.find(group => /Walk$/.test(group.name)), run = attachNativeRun(imported, locomotion, pose);
    assert.ok(walk && run);
    function sample() {
      for (const glove of gloves) {
        glove.root.computeWorldMatrix(true); glove.palm.computeWorldMatrix(true); glove.oldTip.computeWorldMatrix(true);
        const expected = Vector3.TransformCoordinates(glove.onPalm, glove.palm.getWorldMatrix());
        const actual = Vector3.TransformCoordinates(glove.onGlove, glove.root.getWorldMatrix());
        const old = Vector3.TransformCoordinates(glove.onOldTip, glove.oldTip.getWorldMatrix());
        glove.maximumWorldContactDrift = Math.max(glove.maximumWorldContactDrift, Vector3.Distance(expected, actual));
        glove.maximumOldSocketDrift = Math.max(glove.maximumOldSocketDrift, Vector3.Distance(expected, old));
        const relative = glove.root.getWorldMatrix().multiply(Matrix.Invert(glove.palm.getWorldMatrix()));
        glove.maximumRelativeMatrixError = Math.max(glove.maximumRelativeMatrixError,
          ...relative.asArray().map((value, i) => Math.abs(value - glove.bindRelative.asArray()[i])));
        assert.equal(glove.root.parent, glove.palm);
        assert.ok(glove.maximumWorldContactDrift < 2e-6 && glove.maximumRelativeMatrixError < 2e-6);
        const scales = glove.root.scaling.asArray().map(Math.abs);
        assert.ok(Math.max(...scales) - Math.min(...scales) < 1e-7);
      }
      for (const { node, parent, scale } of original) {
        assert.equal(node.parent, parent);
        node.scaling.asArray().forEach((value, i) => assert.ok(Math.abs(value - scale[i]) < 2e-6));
      }
    }
    const clips = [];
    for (const group of [walk, run]) {
      group.start(true, 1);
      for (let frame = 0; frame <= 120; frame++) { group.goToFrame(group.from + (group.to - group.from) * frame / 120); sample(); }
      group.stop(); clips.push({ clip: group.name, frames: 121 });
    }
    const skate = createShadowSkate(imported, { motion }); run.start(true, 1);
    for (let frame = 0; frame < 480; frame++) {
      skate.restoreBase(); run.goToFrame(run.from + (run.to - run.from) * (frame % 60) / 60);
      skate.update(1 / 60, { speed: 1.994, grounded: true, footwear: true, mounted: false, swimming: false, fresh: true });
      skate.apply(); sample();
    }
    assert.equal(skate.diagnostics.blend, 1);
    clips.push({ clip: 'ShadowSkate172-174', frames: 480 });
    for (const glove of gloves) assert.ok(glove.maximumOldSocketDrift > .005, 'Native fixture must reproduce the old independently twisting tip-socket regression.');
    forms.push({ form, originalHierarchyAndScalePreserved: true, clips,
      gloves: gloves.map(({ side, maximumWorldContactDrift, maximumOldSocketDrift, maximumRelativeMatrixError }) => ({ side,
        nativePalmSocket: side === 'right' ? '003' : '010', distalBindMarker: side === 'right' ? '004' : '011',
        maximumWorldContactDrift, maximumOldSocketDrift, maximumRelativeMatrixError })) });
    skate.restoreBase(); skate.dispose(); run.stop();
    for (const glove of gloves) { glove.prop.dispose(); glove.root.dispose(); }
    entries.dispose(); actor.dispose(); asset.dispose();
  }
  const proof = { result: 'PASS', sourceHashes, forms, frames: forms.reduce((n, f) => n + f.clips.reduce((v, c) => v + c.frames, 0), 0),
    limits: 'Actual native prop/rig rigid-contact proof using production capture/mount functions. Browser palm silhouette and service acceptance remain separate; no native joint, authoritative gameplay or source asset is modified.' };
  await mkdir(dirname(output), { recursive: true }); await writeFile(output, JSON.stringify(proof, null, 2));
  console.log(JSON.stringify({ result: proof.result, frames: proof.frames, forms: forms.map(form => ({ form: form.form, gloves: form.gloves })), output }));
} finally { engine.dispose(); }
