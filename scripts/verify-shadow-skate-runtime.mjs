// Opt-in verification against this user's locally converted original assets.
// The files remain under ignored .local/ and are never bundled by this script.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { Matrix, NullEngine, Scene, LoadAssetContainerAsync, Quaternion, TransformNode, Vector3 } from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import { createShadowSkate, SKATE_ROTATION_LIMITS } from '../src/shadow-skate.js';

const motionBytes = await readFile('.local/imports/shadow-skate-motion.json');
const motion = JSON.parse(motionBytes);
const engine = new NullEngine(), scene = new Scene(engine), forms = [];
try {
  for (const name of ['neutral-chaos', 'hero-chaos', 'dark-chaos']) {
    const source = await readFile(`.local/imports/${name}.glb`);
    const asset = await LoadAssetContainerAsync(new Uint8Array(source), scene, { pluginExtension: '.glb', pluginOptions: { gltf: { skipMaterials: true } } });
    const entries = asset.instantiateModelsToScene(value => `${name}-${value}`, false, { doNotInstantiate: true });
    const visual = new TransformNode(`${name}-visual`, scene); visual.scaling.setAll(.1);
    for (const node of entries.rootNodes) node.parent = visual;
    for (const group of entries.animationGroups) group.stop();
    const nodes = visual.getDescendants(), parents = nodes.map(node => node.parent);
    const modelRoot = nodes.find(node => /(?:^|-)000_object_/.test(node.name)).parent;
    const modelRotation = node => {
      node.computeWorldMatrix(true); modelRoot.computeWorldMatrix(true);
      const rotation = Quaternion.Identity();
      node.getWorldMatrix().multiply(Matrix.Invert(modelRoot.getWorldMatrix())).decompose(new Vector3(), rotation, new Vector3());
      return rotation.normalize();
    };
    const upperBody = [['body', '001'], ['head', '014']].map(([semantic, index]) => {
      const node = nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));
      return { semantic, node, rest: modelRotation(node), peak: 0 };
    });
    const controller = createShadowSkate({ visual }, { motion });
    let sampled = 0, skatingFrames = 0;
    const poseSignatures = new Set();
    for (const speed of [1.4, 1.6, 1.8, 1.994, 1.4, 1.29]) {
      const group = entries.animationGroups.find(group => /walk/i.test(group.name));
      group?.start(true, 1);
      for (let frame = 0; frame < 36; frame++) {
        controller.restoreBase();
        group?.goToFrame(group.from + (frame / 36) * (group.to - group.from));
        const transforms = nodes.map(node => ({ p: node.position?.asArray(), s: node.scaling?.asArray(), q: node.rotationQuaternion?.clone() }));
        controller.update(1 / 60, { speed, grounded: true, footwear: true, mounted: false, swimming: false, fresh: true });
        controller.apply();
        nodes.forEach((node, index) => {
          assert.equal(node.parent, parents[index], 'native hierarchy changed');
          assert.deepEqual(node.position?.asArray(), transforms[index].p, 'overlay changed native position');
          assert.deepEqual(node.scaling?.asArray(), transforms[index].s, 'overlay changed native scale');
          node.computeWorldMatrix(true);
          assert.ok(node.getWorldMatrix().asArray().every(Number.isFinite), 'nonfinite native transform');
          if (node.rotationQuaternion) assert.ok(Math.abs(node.rotationQuaternion.length() - 1) < .0001);
        });
        if (controller.diagnostics.active) {
          skatingFrames++;
          // Measure the overlay itself on the actual animated feet006/013;
          // pivots005/012 are static, and underlying Walk cannot prove skating.
          poseSignatures.add(nodes.flatMap((node, index) => /(?:^|-)006_object_|(?:^|-)013_object_/.test(node.name)
            ? [transforms[index].q.conjugate().multiply(node.rotationQuaternion).normalize().asArray().map(value => value.toFixed(4)).join(',')]
            : []).join(';'));
          if (controller.diagnostics.blend === 1) for (const target of upperBody) {
            const delta = modelRotation(target.node).multiply(target.rest.conjugate()).normalize();
            const degrees = 2 * Math.acos(Math.min(1, Math.abs(delta.w))) * 180 / Math.PI;
            target.peak = Math.max(target.peak, degrees);
            assert.ok(degrees <= SKATE_ROTATION_LIMITS[target.semantic] + .001, `${target.semantic} exceeds anatomy adaptation limit`);
          }
        }
        sampled++;
      }
      group?.stop();
    }
    assert.ok(poseSignatures.size > 10, 'source-derived skating is static');
    assert.equal(controller.diagnostics.active, false, 'low-speed exit failed');
    for (const block of [{ mounted: true }, { grounded: false }, { footwear: false }, { fresh: false }]) {
      controller.restoreBase();
      controller.update(.1, { speed: 1.994, grounded: true, footwear: true, mounted: false, fresh: true });
      controller.apply(); controller.restoreBase();
      controller.update(.01, { speed: 1.994, grounded: true, footwear: true, mounted: false, fresh: true, ...block });
      controller.apply(); assert.equal(controller.diagnostics.active, false);
    }
    controller.dispose();
    forms.push({ name, sourceSha256: createHash('sha256').update(source).digest('hex'),
      originalRigNodes: nodes.length, sampledFrames: sampled, skatingFrames, distinctFootPoseSignatures: poseSignatures.size,
      maximumFullSkateRotationDegrees: Object.fromEntries(upperBody.map(target => [target.semantic, target.peak])),
      originalClips: entries.animationGroups.map(group => group.name), positionsScalesAndHierarchyPreserved: true });
    entries.dispose(); visual.dispose(); asset.dispose();
  }
  const report = { result: 'PASS', sourceMotionSha256: createHash('sha256').update(motionBytes).digest('hex'),
    nativeMotionIds: [172, 173, 174], forms,
    limitation: 'CPU transform/authority checks; visual presentation requires live browser QA.' };
  await writeFile('.local/imports/shadow-skate-runtime-proof.json', `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report));
} finally { engine.dispose(); }
