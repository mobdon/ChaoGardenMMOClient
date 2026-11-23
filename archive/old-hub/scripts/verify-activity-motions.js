import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const manifest = JSON.parse(await readFile('.local/extracted/activity-motions/manifest.json', 'utf8'));
const reportPath = '.local/imports/activity-motions/conversion-report.json';
const report = JSON.parse(await readFile(reportPath, 'utf8'));
assert.equal(manifest.commonStates.length, 622);
assert.equal(manifest.commonClips.length, 279);
let total = 0;
let bytes = 0;
for (const library of report.libraries) {
  const data = await readFile(library.output);
  const gltf = JSON.parse(data.subarray(20, 20 + data.readUInt32LE(12)));
  assert.equal(gltf.animations?.length, library.clips, library.name);
  const names = new Set(gltf.animations.map((clip) => clip.name));
  for (const clip of library.actions) assert.ok(names.has(clip.name), `${library.name}/${clip.name}`);
  for (const clip of gltf.animations) assert.ok(clip.channels.length > 0, clip.name);
  total += gltf.animations.length;
  bytes += data.length;
  library.sha256 = sha256(data);
  library.exportedAnimations = gltf.animations.length;
  library.animationChannels = gltf.animations.reduce((sum, clip) => sum + clip.channels.length, 0);
  console.log(`${library.name}: ${library.clips} native clips, ${library.nodeCount} nodes, ${data.length} bytes`);
}
assert.equal(total, 298);
for (const clip of [...manifest.commonClips, ...manifest.supportClips]) {
  const bytes = await readFile(`.local/extracted/activity-motions/${clip.relativePath}`);
  assert.equal(sha256(bytes), clip.sha256, clip.relativePath);
}
report.validation = { complete: true, exportedClips: total, totalGlbBytes: bytes, nativeClips: manifest.counts.totalNativeClips, allSourceClipsHashVerified: true };
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(`PASS: all ${total} original clips decoded and exported; ${bytes} total GLB bytes.`);
