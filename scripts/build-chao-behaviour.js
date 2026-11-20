// Build the local native Chao motion library for server resident behaviour.
// The server runs the ChaoMain behaviour port and publishes the native motion
// state number (0..621), frame and link; the client plays that state's clip.
// Every manifest state whose clip is in the local 279-clip library is kept,
// with its table row, so no state is chosen by hand. The original hierarchy
// and every retained accessor byte is preserved.
// Output stays local: .local/imports/chao-behaviour.glb (+ proof JSON).
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const assetRoot = resolve(process.env.CHAO_ASSET_ROOT || '.');
const sourceFile = '.local/imports/activity-motions/chao-actions.glb';
const outputFile = '.local/imports/chao-behaviour.glb';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const STATES = 622;

const source = await readFile(resolve(assetRoot, sourceFile));
assert.equal(source.readUInt32LE(0), 0x46546c67);
const jsonSize = source.readUInt32LE(12), document = JSON.parse(source.subarray(20, 20 + jsonSize));
assert.equal(source.readUInt32LE(24 + jsonSize), 0x004e4942);
const binary = source.subarray(28 + jsonSize);
const manifest = JSON.parse(await readFile(resolve(assetRoot, '.local/extracted/activity-motions/manifest.json'), 'utf8'));
const library = JSON.parse(await readFile(resolve(assetRoot, '.local/imports/activity-motions/conversion-report.json'), 'utf8'))
  .libraries.find(row => row.name === 'chao-actions');

// Manifest table values are strings. Frames and speeds are decimal; the
// transition speed is a signed 32-bit hex value. A missing field is 0 in the
// native table (AL_motion_table), and a missing NextAnimation is -1.
export function numericTable(raw = {}) {
  const number = (value, fallback) => value === undefined ? fallback : Number.parseFloat(value);
  const table = { StartFrame: number(raw.StartFrame, 0), EndFrame: number(raw.EndFrame, 0),
    PlaySpeed: number(raw.PlaySpeed, 0), LoopProperty: number(raw.LoopProperty, 0), NextAnimation: number(raw.NextAnimation, -1),
    Pose: number(raw.Pose, 0), TransitionSpeed: raw.TransitionSpeed === undefined ? 0 : (Number.parseInt(raw.TransitionSpeed, 16) | 0) };
  for (const [key, value] of Object.entries(table)) assert.ok(Number.isFinite(value), `${key} is not numeric.`);
  return table;
}
const animationNames = new Set(document.animations.map(animation => animation.name));
const clips = [], missing = [];
for (const state of manifest.commonStates) {
  assert.ok(Number.isInteger(state.index) && state.index >= 0 && state.index < STATES, `Bad state ${state.index}`);
  const evidence = library.actions.find(action => action.name === state.nativeName);
  if (!evidence || !animationNames.has(state.nativeName)) { missing.push(state.index); continue; }
  const nativeFile = `.local/extracted/activity-motions/${state.nativeFile}`;
  assert.equal(digest(await readFile(resolve(assetRoot, nativeFile))), evidence.sourceSha256);
  clips.push({ nativeState: state.index, nativeName: state.nativeName, nativeMotionLabel: state.nativeMotionLabel, nativeFile,
    sourceSha256: evidence.sourceSha256, frameRange: evidence.frameRange,
    nativeTableProperties: state.tableProperties, tableProperties: numericTable(state.tableProperties) });
}
assert.ok(clips.length, 'No native motion state has a clip in chao-actions.glb.');

const names = [...new Set(clips.map(clip => clip.nativeName))];
const selected = names.map(name => document.animations.find(animation => animation.name === name));
assert.ok(selected.every(Boolean), 'A selected clip is missing from chao-actions.glb.');

// Compact to the selected animations while keeping every mesh, skin and image.
const retained = structuredClone(document); retained.animations = structuredClone(selected);
const accessorIds = new Set();
for (const mesh of retained.meshes) for (const primitive of mesh.primitives) {
  for (const accessor of Object.values(primitive.attributes)) accessorIds.add(accessor);
  if (primitive.indices !== undefined) accessorIds.add(primitive.indices);
  for (const target of primitive.targets || []) for (const accessor of Object.values(target)) accessorIds.add(accessor);
  assert.ok(!primitive.extensions, 'Unexpected compressed/extension geometry requires explicit remapping.');
}
for (const skin of retained.skins || []) if (skin.inverseBindMatrices !== undefined) accessorIds.add(skin.inverseBindMatrices);
for (const animation of retained.animations) for (const sampler of animation.samplers) { accessorIds.add(sampler.input); accessorIds.add(sampler.output); }
const accessors = [...accessorIds].sort((a, b) => a - b), accessorMap = new Map(accessors.map((old, index) => [old, index]));
const viewIds = new Set();
for (const old of accessors) {
  const accessor = retained.accessors[old];
  if (accessor.bufferView !== undefined) viewIds.add(accessor.bufferView);
  if (accessor.sparse) { viewIds.add(accessor.sparse.indices.bufferView); viewIds.add(accessor.sparse.values.bufferView); }
}
for (const image of retained.images || []) if (image.bufferView !== undefined) viewIds.add(image.bufferView);
const views = [...viewIds].sort((a, b) => a - b), viewMap = new Map(views.map((old, index) => [old, index]));
const parts = []; let length = 0;
retained.bufferViews = views.map(old => {
  const view = structuredClone(document.bufferViews[old]); assert.equal(view.buffer, 0);
  const padding = (-length) & 3; if (padding) { parts.push(Buffer.alloc(padding)); length += padding; }
  const start = view.byteOffset || 0, bytes = binary.subarray(start, start + view.byteLength);
  assert.equal(bytes.length, view.byteLength); parts.push(bytes); view.byteOffset = length; length += bytes.length; return view;
});
retained.accessors = accessors.map(old => {
  const accessor = structuredClone(document.accessors[old]);
  if (accessor.bufferView !== undefined) accessor.bufferView = viewMap.get(accessor.bufferView);
  if (accessor.sparse) { accessor.sparse.indices.bufferView = viewMap.get(accessor.sparse.indices.bufferView); accessor.sparse.values.bufferView = viewMap.get(accessor.sparse.values.bufferView); }
  return accessor;
});
for (const mesh of retained.meshes) for (const primitive of mesh.primitives) {
  for (const key of Object.keys(primitive.attributes)) primitive.attributes[key] = accessorMap.get(primitive.attributes[key]);
  if (primitive.indices !== undefined) primitive.indices = accessorMap.get(primitive.indices);
  for (const target of primitive.targets || []) for (const key of Object.keys(target)) target[key] = accessorMap.get(target[key]);
}
for (const skin of retained.skins || []) if (skin.inverseBindMatrices !== undefined) skin.inverseBindMatrices = accessorMap.get(skin.inverseBindMatrices);
for (const animation of retained.animations) for (const sampler of animation.samplers) { sampler.input = accessorMap.get(sampler.input); sampler.output = accessorMap.get(sampler.output); }
for (const image of retained.images || []) if (image.bufferView !== undefined) image.bufferView = viewMap.get(image.bufferView);
retained.buffers = [{ byteLength: length }];
retained.asset.extras = { ...retained.asset.extras, chaoBehaviour: { version: 2, clips, framesPerSecond: 60000 / 1001,
  note: 'Native Chao motion states for the server ChaoMain behaviour port, keyed by native state number.' } };

let encoded = Buffer.from(JSON.stringify(retained)); encoded = Buffer.concat([encoded, Buffer.alloc((-encoded.length) & 3, 0x20)]);
let compact = Buffer.concat(parts); compact = Buffer.concat([compact, Buffer.alloc((-compact.length) & 3)]);
const header = Buffer.alloc(20); header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4);
header.writeUInt32LE(28 + encoded.length + compact.length, 8); header.writeUInt32LE(encoded.length, 12); header.writeUInt32LE(0x4e4f534a, 16);
const binHeader = Buffer.alloc(8); binHeader.writeUInt32LE(compact.length, 0); binHeader.writeUInt32LE(0x004e4942, 4);
const output = Buffer.concat([header, encoded, binHeader, compact]);

assert.deepEqual(retained.nodes, document.nodes); assert.deepEqual(retained.scenes, document.scenes);
for (const [old, index] of viewMap) {
  const from = document.bufferViews[old], to = retained.bufferViews[index];
  assert.ok(binary.subarray(from.byteOffset || 0, (from.byteOffset || 0) + from.byteLength).equals(compact.subarray(to.byteOffset, to.byteOffset + to.byteLength)));
}
await mkdir(resolve(assetRoot, '.local/imports'), { recursive: true });
await writeFile(resolve(assetRoot, outputFile), output);
const report = { version: 1, passed: true, source: sourceFile, sourceSha256: digest(source), output: outputFile, sha256: digest(output),
  bytes: output.length, stateCount: clips.length, statesWithoutClip: missing, animations: names,
  hierarchyRestAndGeometryPreserved: true, allRetainedBinaryViewsExact: true };
await writeFile(resolve(assetRoot, '.local/imports/chao-behaviour-proof.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
