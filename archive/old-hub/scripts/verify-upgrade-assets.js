import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed game required for source hash verification.');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const manifest = JSON.parse(await readFile('.local/extracted/upgrades/manifest.json', 'utf8'));
const report = JSON.parse(await readFile('.local/imports/upgrade-conversion-report.json', 'utf8'));
if (manifest.assets.length !== 4 || report.assets.length !== 4) throw new Error('Expected four separate wearable props.');
for (const source of manifest.sources) if (hash(await readFile(path.join(game.gamePath, source.relativePath))) !== source.sha256) throw new Error(`Installed source differs: ${source.relativePath}`);
const results = [];
for (const asset of report.assets) {
  const bytes = await readFile(asset.output);
  if (bytes.readUInt32LE(0) !== 0x46546c67 || bytes.readUInt32LE(4) !== 2 || bytes.readUInt32LE(8) !== bytes.length) throw new Error(`Invalid GLB: ${asset.name}`);
  if (hash(bytes) !== asset.sha256) throw new Error(`Output hash differs: ${asset.name}`);
  const document = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)));
  if (document.skins?.length || document.animations?.length) throw new Error(`Rigid prop unexpectedly changes a rig: ${asset.name}`);
  if (!document.images?.length || document.images.some(image => image.uri || !Number.isInteger(image.bufferView))) throw new Error(`Missing embedded original texture: ${asset.name}`);
  const positions = (document.meshes || []).flatMap(mesh => mesh.primitives).map(primitive => document.accessors[primitive.attributes.POSITION]);
  if (!positions.length || positions.some(accessor => !accessor.count || !accessor.min || !accessor.max)) throw new Error(`Empty wearable geometry: ${asset.name}`);
  const min = [0,1,2].map(axis => Math.min(...positions.map(accessor => accessor.min[axis])));
  const max = [0,1,2].map(axis => Math.max(...positions.map(accessor => accessor.max[axis])));
  if (min.some((value, axis) => Math.abs(value + max[axis]) > 0.0001)) throw new Error(`Prop not centered: ${asset.name}`);
  const extras = document.asset.extras.sa2Source;
  if (extras.modelId !== asset.modelId || extras.side !== asset.side) throw new Error(`Lost native provenance: ${asset.name}`);
  if (asset.slot === 'footwear' && (!extras.baseBoot || asset.vertices !== 95)) throw new Error(`Full native jetboot omitted: ${asset.name}`);
  if (asset.slot === 'hands' && asset.vertices !== 42) throw new Error(`Original Hammer Glove geometry differs: ${asset.name}`);
  results.push({ name: asset.name, originalVertices: asset.vertices, glTFBounds: { min, max }, embeddedImages: document.images.length, sha256: asset.sha256 });
}
report.validation = { sourcesUnchanged: true, checkedAssets: results.length, rigidCenteredTexturedProps: true, results };
await writeFile('.local/imports/upgrade-conversion-report.json', `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ passed: results.length, sourcesUnchanged: true, totalBytes: report.assets.reduce((sum, asset) => sum + asset.bytes, 0), assets: results.map(({ name, originalVertices, embeddedImages }) => ({ name, originalVertices, embeddedImages })) }, null, 2));
