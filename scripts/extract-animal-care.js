import { copyFile, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed Sonic Adventure2 required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const configs = path.join(tools, 'GameConfig/PC_SA2');
const output = path.resolve('.local/extracted/care'), dataOutput = path.resolve('.local/care'), textures = path.resolve('.local/extracted/textures');
for (const directory of [output, dataOutput, textures]) await mkdir(directory, { recursive: true });
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const hex = value => `0x${value.toString(16).padStart(8, '0')}`;
const slash = value => value.split(path.sep).join('/');
const sources = [];
async function original(relativePath) {
  const bytes = await readFile(path.join(game.gamePath, relativePath));
  sources.push({ relativePath, bytes: bytes.length, sha256: digest(bytes) });
  return bytes;
}
const exe = await original('sonic2app.exe');
const pe = exe.readUInt32LE(0x3c), optional = pe + 24, imageBase = exe.readUInt32LE(optional + 28), peSections = [];
if (exe.readUInt16LE(optional) !== 0x10b) throw new Error('Expected native PE32 executable.');
for (let i = 0; i < exe.readUInt16LE(pe + 6); i++) {
  const at = optional + exe.readUInt16LE(pe + 20) + i * 40;
  peSections.push({ rva: exe.readUInt32LE(at + 12), virtualSize: exe.readUInt32LE(at + 8), size: exe.readUInt32LE(at + 16), raw: exe.readUInt32LE(at + 20) });
}
// Uninitialized tail of a PE section: present at run time, zero before any code runs.
const zeroFillForVA = (va, length = 1) => { const rva = va - imageBase; return peSections.some(item => rva >= item.rva + item.size && rva + length <= item.rva + item.virtualSize); };
const fileOffsetForVA = (va, length = 1) => { const rva = va - imageBase, section = peSections.find(item => rva >= item.rva && rva + length <= item.rva + item.size); return section ? section.raw + rva - section.rva : null; };
const atVA = (va, length = 1) => { const offset = fileOffsetForVA(va, length); if (offset === null) throw new Error(`Unmapped native pointer:${hex(va)}`); return offset; };
function sections(text) {
  const result = []; let item;
  for (const line of text.split(/\r?\n/)) {
    const heading = line.match(/^\[([^\]]+)\]/);
    if (heading) { item = { name: heading[1], lines: [line], fields: {} }; result.push(item); }
    else if (item) { item.lines.push(line); const field = line.match(/^([^;=][^=]*)=(.*)$/); if (field) item.fields[field[1].trim()] = field[2].trim(); }
  }
  return result;
}
const chao = sections(await readFile(path.join(configs, 'ChaoCommon.ini'), 'utf8'));
const common = sections(await readFile(path.join(configs, 'CommonObjects.ini'), 'utf8'));
const animalSections = chao.filter(item => item.fields.filename?.startsWith('Chao/animals/'));
const partSections = chao.filter(item => item.fields.filename?.startsWith('Chao/models/AnimalParts/'));
const statsSections = chao.filter(item => ['Chao Animal and Chaos Drive Stats Array', 'Chao Fruit Stats Array'].includes(item.name));
const driveSections = common.filter(item => /^(?:Chaos Drive Texlist|(?:Green|Purple|Red|Yellow) Chaos Drive)$/.test(item.name));
const gardenSections = chao.filter(item => /^Chao\/(?:Trees|Seeds)\//.test(item.fields.filename || '')
  || /^Chao\/common\/models\/(?:\w*Fruit\w*|Mushroom|MintCandy|FruitCore)\.sa2mdl$/.test(item.fields.filename || ''));
const selected = [...animalSections, ...partSections, ...statsSections, ...driveSections, ...gardenSections];
// SA Tools786 maps this export 0x10 bytes past its wrapper object (into the
// wrapper's position.z). Every other child part's wrapper sits 0x38 bytes after
// the object its part list points to; the corrected wrapper's child pointer is
// that object (0x012d8c04), which validation below checks again.
const addressCorrections = [{ name: 'Child Chao Rabbit Right Ear', configured: 'ED8C4C', corrected: 'ED8C3C' }];
for (const correction of addressCorrections) {
  const section = partSections.find(item => item.name === correction.name);
  if (!section || section.fields.address !== correction.configured) throw new Error(`Unexpected SA Tools address for ${correction.name}`);
  section.fields.address = correction.corrected;
  section.lines = section.lines.map(line => line.startsWith('address=') ? `address=${correction.corrected}` : line);
}
const mapping = path.join(output, 'care.ini');
await writeFile(mapping, `game=SA2B\ndatafile=sonic2app.exe\n\n${selected.map(item => item.lines.join('\n')).join('\n')}\n`);
const options = { windowsHide: true, timeout: 120_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
const result = await run('dotnet', [path.join(tools, 'bin/lib/split.dll'), 'binary', path.join(game.gamePath, 'sonic2app.exe'), mapping, output], options);
await writeFile(path.join(output, 'extraction.log'), result.stdout + result.stderr);
for (const name of ['al_minimal_tex', 'al_mini_parts_tex', 'chaosdrive_tex', 'al_object']) {
  const relativePath = `resource/gd_PC/${name}.prs`; await original(relativePath);
  const copy = path.join(textures, `${name}.prs`); await copyFile(path.join(game.gamePath, relativePath), copy);
  await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', copy], options);
}
const modelSections = animalSections.filter(item => item.fields.type === 'chunkmodel');
const modelByPointer = new Map(modelSections.map(item => [Number.parseInt(item.fields.address, 16) + imageBase, item]));
const partByPointer = new Map(partSections.map(item => [Number.parseInt(item.fields.address, 16) + imageBase, item]));
// SA Tools exports attachment wrappers. The live part lists may point directly
// to a child object inside that mapped hierarchy rather than its export root.
const partByObjectPointer = new Map();
const partHierarchyGaps = [];
for (const section of partSections) {
  const root = Number.parseInt(section.fields.address, 16) + imageBase, seen = new Set();
  const visit = (pointer, hierarchyPath) => {
    if (!pointer || seen.has(pointer)) return;
    if (seen.size >= 512) throw new Error(`Unexpected animal-part hierarchy size:${section.name}`);
    const at = fileOffsetForVA(pointer, 52);
    if (at === null) {
      partHierarchyGaps.push({ model: section.name, rootAddress: hex(root), pointer: hex(pointer), hierarchyPath, status: 'object-not-file-backed' });
      return;
    }
    const scale = [32,36,40].map(offset => exe.readFloatLE(at + offset));
    if (exe.readUInt32LE(at) > 0xffff || scale.some(value => !Number.isFinite(value) || Math.abs(value) > 1000)) {
      partHierarchyGaps.push({ model: section.name, rootAddress: hex(root), pointer: hex(pointer), hierarchyPath, status: 'mapped-object-layout-unverified' });
      return;
    }
    seen.add(pointer);
    const references = partByObjectPointer.get(pointer) || [];
    references.push({ section, root, hierarchyPath }); partByObjectPointer.set(pointer, references);
    visit(exe.readUInt32LE(at + 44), `${hierarchyPath}/child`);
    visit(exe.readUInt32LE(at + 48), `${hierarchyPath}/sibling`);
  };
  visit(root, 'root');
}
const names = ['Penguin','Seal','Otter','Rabbit','Cheetah','Warthog','Bear','Tiger','Gorilla','Peacock','Parrot','Condor','Skunk','Sheep','Raccoon','Half Fish','Skeleton Dog','Bat','Dragon','Unicorn','Phoenix'];
const statFields = ['mood','belly','swim','fly','run','power','stamina','luck','intelligence','unknown18'];
const readStats = (address, count) => Array.from({ length: count }, (_, index) => {
  const va = address + index * 20, at = atVA(va);
  return { index, address: hex(va), values: Object.fromEntries(statFields.map((field, i) => [field, exe.readInt16LE(at + i * 2)])) };
});
const stats = readStats(0x008a6240, 26), fruitStats = readStats(0x008a6448, 24);
const partSlotNames = ['leftArm','rightArm','leftEar','rightEar','forehead','leftHorn','rightHorn','leftLeg','rightLeg','tail','leftWing','rightWing','tongue'];
function readParts(index, tableAddress) {
  const pointer = exe.readUInt32LE(atVA(tableAddress + index * 4));
  if (!pointer) return { pointer: null, status: 'absent', slots: [], mapped: true };
  const at = fileOffsetForVA(pointer, partSlotNames.length * 4);
  if (at === null) {
    // Half Fish, Skeleton Dog and Bat share one 13-slot list in zero-filled .data.
    // GameCube ChaoMain.rel (GSNE8P) has the same shared list at .data 0x8FD60:
    // 0x34 zero bytes, no relocations, referenced only by the six array entries.
    if (zeroFillForVA(pointer, partSlotNames.length * 4)) return { pointer: hex(pointer), status: 'zero-initialized-empty-list', slots: partSlotNames.map(slot => ({ slot, pointer: null, model: null, candidates: [], mapped: true })), mapped: true };
    return { pointer: hex(pointer), status: 'runtime-list-not-file-backed', slots: [], mapped: false };
  }
  const slots = partSlotNames.map((slot, i) => {
    const nativePointer = exe.readUInt32LE(at + i * 4), direct = partByPointer.get(nativePointer);
    const references = direct ? [{ section: direct, root: nativePointer, hierarchyPath: 'root' }] : partByObjectPointer.get(nativePointer) || [];
    const models = references.map(({section,root,hierarchyPath}) => ({ name: section.name, path: `.local/extracted/care/${section.fields.filename}`, texture: section.fields.texture, rootAddress: hex(root), nativeObjectAddress: hex(nativePointer), hierarchyPath }));
    return { slot, pointer: nativePointer ? hex(nativePointer) : null, model: models.length === 1 ? models[0] : null, candidates: models.length > 1 ? models : [], mapped: nativePointer === 0 || models.length > 0 };
  });
  return { pointer: hex(pointer), status: slots.every(slot => slot.mapped) ? 'file-backed-models-mapped' : 'file-backed-model-mappings-incomplete', slots, mapped: slots.every(slot => slot.mapped) };
}
const animals = [];
for (const [index, name] of names.entries()) {
  const pointer = exe.readUInt32LE(atVA(0x012966f0 + index * 4)), model = modelByPointer.get(pointer);
  if (!model) throw new Error(`Animal${index} does not match original mapped model pointer:${hex(pointer)}`);
  const animations = (model.fields.animations || '').split(',').filter(Boolean).map(filename => {
    const source = animalSections.find(item => item.fields.filename === `Chao/animals/${filename}`);
    if (!source) throw new Error(`Missing mapped animal action:${filename}`);
    return { name: source.name, path: `.local/extracted/care/${source.fields.filename}`, nodeCount: Number(source.fields.numparts), address: hex(Number.parseInt(source.fields.address,16) + imageBase) };
  });
  const stageResource = `resource/gd_PC/${path.parse(model.fields.filename).name.toLowerCase()}.prs`;
  await original(stageResource);
  animals.push({ id: index, key: name.toLowerCase().replaceAll(' ','-'), name, sourceEnum: `SA2BAnimal_${name.replaceAll(' ','')}`, nativeHistoryBitHex: hex(2 ** index),
    source: { binary: 'sonic2app.exe', modelPointerArray: hex(0x012966f0), modelPointer: hex(pointer), statsAddress: stats[index].address, stageResourceCandidate: stageResource },
    nativeItemStats: stats[index].values, model: { name: model.name, path: `.local/extracted/care/${model.fields.filename}`, textureArchive: 'resource/gd_PC/al_minimal_tex.prs' }, animations,
    parts: { child: readParts(index,0x012e59a0), adult: readParts(index,0x012e5938) },
    nativeEffectStatus: 'Raw original ChaoItemStats and mapped model resources decoded; runtime part lists, consumption multipliers, random part choice, learned behavior and lifecycle conditions require separate validation.' });
}
const driveDefs = [['Yellow','swim'],['Green','run'],['Red','power'],['Purple','fly']];
const drives = driveDefs.map(([color, skill], index) => {
  const id = 21 + index, model = driveSections.find(item => item.name === `${color} Chaos Drive`);
  return { id, key: `${color.toLowerCase()}-chaos-drive`, name: `${color} Chaos Drive`, primarySkill: skill, sourceEnum: `SA2BAnimal_${color}ChaosDrive`,
    nativeItemStats: stats[id].values, source: { binary: 'sonic2app.exe', statsAddress: stats[id].address, modelAddress: hex(Number.parseInt(model.fields.address,16) + imageBase) },
    model: { path: `.local/extracted/care/${model.fields.filename}`, textureArchive: 'resource/gd_PC/chaosdrive_tex.prs', texlist: '.local/extracted/care/OBJECT/tls/ChaosDrive.satex' },
    grantsAnimalHistoryBit: false, animalParts: false, nativeEffectStatus: 'Independent native drive rows; not an animal species or automatic animal-history/part unlock.' };
});
// Fruit kind -> model: 24 loose-fruit pointers at 0x013340c8, then on-tree models
// for kinds 0-9 (GameCube ChaoMain .data 0xA4470 has the same layout, read by
// ALO_FruitDisplayer). Tree kind 1-7 -> {young, mature, dying} at 0x01329148,
// stride 12 (GameCube .data 0xB6358 + kind*12, ALO_GrowTreeExecutor).
const gardenByPointer = new Map(gardenSections.map(item => [Number.parseInt(item.fields.address, 16) + imageBase, item]));
const gardenModel = pointer => {
  const section = gardenByPointer.get(pointer);
  if (!section) throw new Error(`Garden model pointer does not match a mapped model:${hex(pointer)}`);
  return { name: section.name, path: `.local/extracted/care/${section.fields.filename}`, texture: section.fields.texture, address: hex(pointer) };
};
const fruitKeys = ['chao-garden','hero-garden','dark-garden','strong','tasty','hero','dark','round','triangle','square','heart','chao','smart','orange','blue','pink','green','purple','yellow','red','mushroom','mushroom-alt','mint-candy','grapes'];
const fruitModels = fruitKeys.map((key, id) => {
  const record = { id, key, stats: fruitStats[id].address, model: gardenModel(exe.readUInt32LE(atVA(0x013340c8 + id * 4))) };
  if (id < 10) record.onTreeModel = gardenModel(exe.readUInt32LE(atVA(0x013340c8 + (24 + id) * 4)));
  return record;
});
const treeKeys = ['strong','tasty','hero','dark','round','triangle','square'];
const treeModels = treeKeys.map((key, index) => ({ kind: index + 1, key, fruitId: index + 3,
  stages: Object.fromEntries(['young','mature','dying'].map((stage, i) => [stage, gardenModel(exe.readUInt32LE(atVA(0x01329148 + index * 12 + i * 4)))])) }));
for (const source of sources) {
  source.unchanged = digest(await readFile(path.join(game.gamePath,source.relativePath))) === source.sha256;
  if (!source.unchanged) throw new Error(`Installed source changed:${source.relativePath}`);
}
const catalog = { version: 1, addressCorrections, animals, drives, reservedStatRows: [stats[25]], fruitStats, fruitModels, treeModels, partHierarchyGaps,
  units: 'Original signed-short ChaoItemStats fields. Do not equate them to character levels, S grades or skill points without validating the native application path.',
  sourceSchema: { itemStats: { address:hex(0x008a6240), stride:20, count:26, fields:statFields }, fruitStats:{ address:hex(0x008a6448), stride:20, count:24 },
    history: 'AL_PARTS.MinimalFlag species bitmask is separate from eight MinimalParts appearance slots; grade, experience, skill, mood and personality fields are separate.' },
  implementationBoundary: { stagePickups: 'Server-owned spawn/drop IDs, source availability and collision/proximity.', carriedInventory: 'Store owned animal/drive counts separately from learned species history and active body parts.', careUse: 'Atomic server validation/decrement, signed stat effects with explicit units/clamps, history update and approved part/behavior change.', visual: 'Render approved parts on native skeleton sockets; do not mutate rig proportions.', evolution: 'Reincarnation, adulthood, alignment and qualifying animal history require source validation; no direct animal-to-Chaos transformation inferred.' },
  gaps: ['Exact native item-to-experience/skill application and grade/level-up curves', 'Stage-specific spawn/drop tables and carry limits', 'Animal behavior acquisition probability and persistence', 'Random part selection/removal and rare-animal special handling (traced from GameCube ChaoMain ALBHV_Capture, not yet applied here)', 'Native morph/alignment influence magnitudes and saturation', 'Native animal-history reset timing and complete Chaos evolution criteria', 'Breeding/genetics/personality rules remain separate systems'],
  evidence: ['Official X-Hax SA Tools786 PC_SA2/ChaoCommon.ini AnimalAndDriveStats/FruitStats, garden animal/motion/part models', 'Official X-Hax SA Tools786 PC_SA2/CommonObjects.ini four original Chaos Drive models/texlist', 'X-Hax SA2Enums.h SA2BAnimal0–24 and SA2BAnimalFlags', 'Exant64/CWE al_minimal.cpp native model/stat addresses; only original mapped rows read, not added SADX/mod animal values', 'Exant64/CWE al_parts.h native child/adult part pointer arrays; source model pointers checked against original mapped geometry', 'GameCube GSNE8P ChaoMain.rel .data 0x8FD94 (adult) / 0x8FDE8 (child) part-list arrays: same filled slots as PC for all 42 lists; Half Fish, Skeleton Dog and Bat share the all-null list at 0x8FD60'] };
await writeFile(path.join(dataOutput,'animals.json'),`${JSON.stringify(catalog,null,2)}\n`);
const manifest = { version:1, sources, animalCount:animals.length, driveCount:drives.length, sourceStatRows:stats.length, fruitStatRows:fruitStats.length,
  animalModels:modelSections.length, animalMotionSections:animalSections.filter(item=>item.fields.type==='animation').length, animalPartModels:partSections.length,
  modelFiles: await Promise.all([...animals.map(item=>item.model.path),...drives.map(item=>item.model.path)].map(async file=>({path:file,bytes:(await stat(path.resolve(file))).size}))),
  partListStatus: Object.fromEntries(['child','adult'].map(age => [age, Object.fromEntries([...new Set(animals.map(animal => animal.parts[age].status))].map(status => [status, animals.filter(animal => animal.parts[age].status === status).length]))])),
  partHierarchyGaps,
  note:'Extraction and care data audit; no live server rules or existing Chao geometry changed.' };
await writeFile(path.join(output,'manifest.json'),`${JSON.stringify(manifest,null,2)}\n`);
console.log(JSON.stringify({sourcesUnchanged:sources.length,animals:animals.length,drives:drives.length,nativeStatRows:stats.length,fruitStatRows:fruitStats.length,animalModels:manifest.animalModels,animalMotionSections:manifest.animalMotionSections,animalPartModels:manifest.animalPartModels,driveDeltas:drives.map(({name,nativeItemStats})=>({name,fields:nativeItemStats}))},null,2));
