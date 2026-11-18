import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const json = async name => JSON.parse(await readFile(name, 'utf8'));
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed game required to verify original input hashes.');
const manifest = await json('.local/extracted/activities/manifest.json');
const races = await json('.local/activities/races.json');
const karate = await json('.local/activities/karate.json');
const sets = await json('.local/activities/race-sets.json');
const cameras = await json('.local/activities/race-cameras.json');
const geometry = await json('.local/imports/activities/conversion-report.json');
for (const source of manifest.sources) if (hash(await readFile(path.join(game.gamePath, source.relativePath))) !== source.sha256) throw new Error(`Original source hash differs:${source.relativePath}`);
if (races.courses.length !== 62 || races.raceBots.length !== 115 || new Set(races.courses.map(item => item.id)).size !== 62) throw new Error('Incomplete native race catalog.');
for (const course of races.courses) {
  if (course.available !== false || course.geometryValidated !== false || course.checkpoints.length) throw new Error('Unvalidated native route must remain unavailable.');
  if (!course.model || !course.worldId || !Object.values(course.bounds).every(Number.isFinite)) throw new Error(`Missing source world transform:${course.id}`);
}
if (karate.nativeOpponents.length !== 27 || karate.appearanceRows.length !== 28 || karate.names.length !== 27 || karate.tiers.length !== 4 || karate.tiers.some(item => item.opponents.length !== 5)) throw new Error('Incomplete native Karate tables.');
for (const opponent of karate.nativeOpponents) {
  if (opponent.name !== karate.names[opponent.nameIndex] || JSON.stringify(opponent.skills) !== JSON.stringify(karate.appearanceRows[opponent.statsIndex].skills)) throw new Error(`Incorrect joined native opponent:${opponent.id}`);
}
if (sets.sets.length !== 14 || cameras.cameras.length !== 10 || geometry.assets.length !== 4) throw new Error('Missing activity resources.');
for (const camera of cameras.cameras) {
  const rebuilt = Buffer.concat(camera.slots.map(slot => Buffer.from(slot.rawHex, 'hex')));
  if (camera.slots.length !== 80 || !rebuilt.equals(await readFile(path.join(game.gamePath, camera.source)))) throw new Error(`Camera byte coverage differs:${camera.filename}`);
}
const assets = [];
for (const asset of geometry.assets) {
  const bytes = await readFile(asset.output);
  if (bytes.readUInt32LE(0) !== 0x46546c67 || bytes.readUInt32LE(8) !== bytes.length || hash(bytes) !== asset.sha256) throw new Error(`Invalid activity GLB:${asset.name}`);
  const document = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)));
  if (!document.meshes?.length || !document.images?.length || document.images.some(image => image.uri || !Number.isInteger(image.bufferView))) throw new Error(`Missing native activity geometry/embedded textures:${asset.name}`);
  const collision = await json(asset.collisionOutput);
  for (const mesh of collision.meshes) for (const triangle of mesh.triangles) if (triangle.length !== 3 || triangle.some(index => !Number.isInteger(index) || index < 0 || index >= mesh.vertices.length)) throw new Error(`Invalid native collision triangle:${asset.name}`);
  if (document.asset.extras.sa2ActivitySource.originRecentered !== false || collision.scale !== 0.1) throw new Error('Native activity coordinate origin/scale changed.');
  assets.push({ name: asset.name, meshes: asset.meshCount, originalVertices: asset.vertices, collisionTriangles: asset.collisionTriangles, bytes: asset.bytes });
}
const result = { sourceFilesUnchanged: manifest.sources.length, raceConfigurations: races.courses.length, raceBots: races.raceBots.length, karateNamedDescriptors: karate.nativeOpponents.length, karateAppearanceAndSkillRows: karate.appearanceRows.length, courseSetFiles: sets.sets.length, setPlacements: sets.sets.reduce((sum, item) => sum + item.count, 0), cameraFiles: cameras.cameras.length, cameraCandidateSegments: cameras.cameras.reduce((sum, item) => sum + item.activeCandidateSlots, 0), cameraCoverageExact: true, assets, totalGeometryBytes: assets.reduce((sum, item) => sum + item.bytes, 0), nativeRoutesPlayable: 0, note: 'Structural/source verification only; race gates, camera semantics, tier selection code and full native gameplay rules remain unvalidated.' };
await writeFile('.local/activities/verification-report.json', `${JSON.stringify(result, null, 2)}\n`);
console.log(JSON.stringify(result, null, 2));
