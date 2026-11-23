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
const groundStart = sceneSource.indexOf('  function groundVisualFeet('), groundEnd = sceneSource.indexOf('\n  function captureSkateHeight(', groundStart);
assert.ok(groundStart >= 0 && groundEnd > groundStart, 'Production foot grounding function unavailable');
const ground = new Function(`return (${sceneSource.slice(groundStart, groundEnd).trim()});`)();

const helpers=sceneSource.slice(sceneSource.indexOf('  function captureSkateHeight('),sceneSource.indexOf('  function groundImportedFeet('));
const {captureSkateHeight,groundOnFoot}=new Function('groundVisualFeet',helpers+';return {captureSkateHeight,groundOnFoot}')(ground);
const flameCode=sceneSource.slice(sceneSource.indexOf('  function skullInActor('),sceneSource.indexOf('  async function attachNativeFlame('));
const positionFlame=new Function('Vector3','Matrix',flameCode+';return positionFlame')(Vector3,Matrix);
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
      imported.height=.4;imported.skate=skate;imported.groundFitReady=false;const player={root:actor,imported};captureSkateHeight(player,pose);
      const flame={root:actor,nativeEmotion:{position:new Vector3()},flameAnchor:{skull:head,gap:.13}};
      let group=null,low=Infinity,high=-Infinity;
      assert.equal(head.parent,pose.get('015').node);
      assert.equal(head.parent.parent,pose.get('014').node);
      assert.equal(head.parent.parent.parent,pose.get('001').node);
      for (let frame = 0; frame < frames; frame++) {
        const frameMode = spec.transition ? frame < 30 ? 'Run119' : frame < 60 ? 'Walk' : 'Idle' : mode;
        const speed = spec.transition ? modes.find(([name]) => name === frameMode)[1] : spec.speed;
        const gait = frameMode === 'Idle' ? 'Idle' : frameMode === 'Walk' ? 'Walk' : 'Run';
        const skating = frameMode.startsWith('Skate');
        steady.restoreBase(); skate.restoreBase();
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
        const base=head.position.clone(), q=head.rotationQuaternion.clone(), parent=head.parent;
        const neckBefore=localPosition(head,parent);
        steady.update(dt,{gait,skateBlend:skate.diagnostics.blend,headStroke:skate.headStroke});steady.apply();
        groundOnFoot(player,dt);
        if(skate.diagnostics.blend===1 && frame>80){
          const span=run.to-run.from;
          const phase=(run.getCurrentFrame()-run.from)/span;
          assert.ok(Math.abs(phase-skate.diagnostics.phase)<1e-5,'shoe and Run119 pivot stride clocks separated');
          const y=localPosition(pose.get('001').node,actor).y;
          low=Math.min(low,y); high=Math.max(high,y);
        }
        positionFlame(flame);
        assert.ok(Math.abs(flame.nativeEmotion.position.y-localPosition(head,actor).y-.13)<1e-6,'flame height drifted from skull');
        const observed=steady.diagnostics;
        assert.equal(observed.available,true);
        assert.ok(Vector3.Distance(Vector3.FromArray(observed.position),localPosition(head,actor))<1e-5);
        assert.ok(Vector3.Distance(localPosition(head,parent),neckBefore)<1e-5);
        assert.deepEqual(head.position.asArray(),base.asArray());
        steady.restoreBase();assert.deepEqual(head.rotationQuaternion.asArray(),q.asArray());
      }
      if(mode==="Skate174" && actorScale===1)console.log(form,"sprint torso range",high-low);
      if(low!==Infinity && mode.startsWith('Skate') && frames>100)assert.ok(high-low<.001,mode+' skate torso bounces: '+(high-low));
      trials.push({mode,frames,actorScale});skate.dispose();group.stop();
    }
    for (const { node, parent, scale } of original) {
      assert.equal(node.parent, parent);
      node.scaling.asArray().forEach((value, index) => assert.ok(Math.abs(value - scale[index]) < 2e-6, 'Only original native scale-channel roundoff is allowed'));
    }
    forms.push({ form, originalNodes: nodes.length, originalHierarchyAndScalesPreserved: true, trials });
    for (const shoe of shoes) { shoe.prop.dispose(); shoe.root.dispose(); }
    entries.dispose(); visual.dispose(); actor.dispose();
  }
  const report={result:'PASS',forms,sourceHashes,policy:'016 remains on its animated native neck through locomotion and grounding'};
  await mkdir(dirname(output),{recursive:true});await writeFile(output,JSON.stringify(report,null,2));
  console.log(forms.map(f=>f.form+': '+f.trials.reduce((n,t)=>n+t.frames,0)+' frames').join('\n'));
} finally {scene.dispose();engine.dispose();}
