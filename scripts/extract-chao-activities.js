import { copyFile, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
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
const output = path.resolve('.local/extracted/activities');
const dataOutput = path.resolve('.local/activities');
const textures = path.resolve('.local/extracted/textures');
for (const directory of [output, dataOutput, textures]) await mkdir(directory, { recursive: true });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const slash = value => value.split(path.sep).join('/');
const sources = new Map();
async function original(relativePath) {
  const bytes = await readFile(path.join(game.gamePath, relativePath));
  sources.set(relativePath, { relativePath, bytes: bytes.length, sha256: hash(bytes) });
  return bytes;
}
function peReader(bytes) {
  const pe = bytes.readUInt32LE(0x3c), optional = pe + 24;
  if (bytes.toString('ascii', pe, pe + 4) !== 'PE\0\0' || bytes.readUInt16LE(optional) !== 0x10b) throw new Error('Expected original PE32 binary.');
  const imageBase = bytes.readUInt32LE(optional + 28), sections = [];
  for (let i = 0; i < bytes.readUInt16LE(pe + 6); i++) {
    const at = optional + bytes.readUInt16LE(pe + 20) + i * 40;
    sections.push({ rva: bytes.readUInt32LE(at + 12), size: bytes.readUInt32LE(at + 16), raw: bytes.readUInt32LE(at + 20) });
  }
  return va => {
    const rva = va - imageBase, section = sections.find(item => rva >= item.rva && rva < item.rva + item.size);
    if (!section) throw new Error(`Native address outside file-backed sections:0x${va.toString(16)}`);
    return section.raw + rva - section.rva;
  };
}
const exe = await original('sonic2app.exe'), atVA = peReader(exe);
const hex = value => `0x${value.toString(16).padStart(8, '0')}`;
function sections(text) {
  const result = []; let item;
  for (const line of text.split(/\r?\n/)) {
    const heading = line.match(/^\[([^\]]+)\]/);
    if (heading) { item = { name: heading[1], lines: [line], fields: {} }; result.push(item); }
    else if (item) { item.lines.push(line); const field = line.match(/^([^;=][^=]*)=(.*)$/); if (field) item.fields[field[1].trim()] = field[2].trim(); }
  }
  return result;
}
const decodeName = bytes => [...bytes.subarray(0, bytes.indexOf(0) < 0 ? bytes.length : bytes.indexOf(0))].map(value => {
  if ((value >= 0x21 && value <= 0x3a) || (value >= 0x41 && value <= 0x5a)) return String.fromCharCode(value + 32);
  if (value >= 0x10 && value <= 0x19) return String.fromCharCode(value + 32);
  if (value === 0x5f) return ' ';
  return `<${value.toString(16).padStart(2, '0')}>`;
}).join('');

// Join original appearance, independent skill, descriptor, and localized
// name tables. The appearance template's own skill slots are initially zero.
const nameFile = await original('resource/gd_PC/MsgChaoKarateName_e.bin');
const names = [];
for (let at = 0; nameFile.readUInt32BE(at) !== 0xffffffff; at += 4) {
  const start = nameFile.readUInt32BE(at); if (start >= nameFile.length) throw new Error('Invalid Karate name offset.');
  names.push(decodeName(nameFile.subarray(start, nameFile.indexOf(0, start))));
}
if (names.length !== 27) throw new Error('Expected27 localized Karate names.');
const statNames = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
const appearanceRows = Array.from({ length: 28 }, (_, index) => {
  const address = 0x01296c10 + index * 52, at = atVA(address), statsAt = atVA(0x01296a50 + index * 16);
  const skills = Array.from({ length: 8 }, (_, i) => exe.readInt16LE(statsAt + i * 2));
  return { index, address: hex(address), statAddress: hex(0x01296a50 + index * 16), skills,
    stats: Object.fromEntries(statNames.map((name, i) => [name, skills[i]])), specialSkill: skills[7],
    appearance: { rawBytes0to18: [...exe.subarray(at, at + 19)], type: exe[at], eye: exe[at + 1], mouth: exe[at + 2], icon: exe[at + 3], headgear: exe[at + 4], hideFeet: exe[at + 5], medal: exe[at + 6], color: exe[at + 7], monotone: exe[at + 8], texture: exe[at + 9], shiny: exe[at + 10], animalParts: Array.from({ length: 8 }, (_, i) => exe.readInt8(at + 11 + i)), morph: Object.fromEntries(['powerRun', 'flySwim', 'alignment', 'growth'].map((name, i) => [name, exe.readInt16LE(at + 20 + i * 2)])), originalNameBytes: [...exe.subarray(at + 28, at + 36)], originalInlineSkills: Array.from({ length: 8 }, (_, i) => exe.readUInt16LE(at + 36 + i * 2)) } };
});
const opponents = Array.from({ length: 27 }, (_, index) => {
  const address = 0x01296940 + index * 10, at = atVA(address);
  const appearanceIndex = exe.readUInt16LE(at), nameIndex = exe.readUInt16LE(at + 2), statsIndex = exe.readUInt16LE(at + 4);
  if (!appearanceRows[appearanceIndex] || !appearanceRows[statsIndex] || !names[nameIndex]) throw new Error('Invalid original Karate descriptor relation.');
  return { id: `native-karate-opponent-${index}`, nativeIndex: index, name: names[nameIndex], appearanceIndex, nameIndex, statsIndex,
    skills: appearanceRows[statsIndex].skills, stats: appearanceRows[statsIndex].stats, appearance: appearanceRows[appearanceIndex].appearance,
    source: { binary: 'sonic2app.exe', descriptorAddress: hex(address), appearanceAddress: appearanceRows[appearanceIndex].address, statAddress: appearanceRows[statsIndex].statAddress, nameFile: 'resource/gd_PC/MsgChaoKarateName_e.bin', extraWord6: exe.readInt16LE(at + 6), extraByte8: exe.readInt8(at + 8), extraByte9: exe[at + 9] } };
});
const tiers = ['Beginner', 'Standard', 'Expert', 'Super'].map((name, index) => ({ id: `karate-${name.toLowerCase()}`, name,
  source: { rosterStartDescriptor: 2 + index * 5, rosterCount: 5, nativeTablesJoined: true, tierSelectionCallsiteValidated: false,
    groupingEvidence: 'Five-opponent retail tier grouping inferred from the ordered localized/native descriptor sequence; selection-code and unlock-condition trace remains open.' },
  opponents: opponents.slice(2 + index * 5, 7 + index * 5), rewardRings: 50,
  rewardNote: 'Proposed MMO reward, not an original ring payout.', nativeRulesValidated: false }));
const karate = { version: 1, tiers, nativeOpponents: opponents, appearanceRows, names,
  rules: { retailMatchBehaviorReimplemented: false, gaps: ['Exact tier-selection arithmetic and unlock checks', 'Hit/avoidance/stamina/zeal formulas and native clock', 'Ring-out, timeout, draw and tournament recovery details', 'Original medal/reward assignment'] },
  evidence: ['X-Hax sa2-mod-loader SA2Variables.h: ChaoKarateOpponents0x01296c10,count28', 'Ngolinvaux/SA2Randomizer SA2Rando.h: ChaoKarateOpponentsStats0x01296a50,224shorts', 'Exant64/CWE SA2Structs.h: AL_SHAPE_ELEMENT stride0x34', 'Original adjacent descriptor sequence0x01296940,count27,stride10; pointers/indices join verified against localized name file'] };
await writeFile(path.join(dataOutput, 'karate.json'), `${JSON.stringify(karate, null, 2)}\n`);

const objectHead = atVA(0x013533f8), objectCount = exe.readUInt32LE(objectHead), objectList = exe.readUInt32LE(objectHead + 4);
if (objectCount !== 12) throw new Error('Unexpected native race object list.');
const objectTypes = Array.from({ length: objectCount }, (_, index) => {
  const at = atVA(objectList + index * 16), nameAt = atVA(exe.readUInt32LE(at + 12));
  return { index, name: exe.toString('ascii', nameAt, exe.indexOf(0, nameAt)), functionAddress: hex(exe.readUInt32LE(at + 8)), loadFlags: exe[at], list: exe[at + 1], objectFlags: exe.readUInt16LE(at + 2), distance: exe.readFloatLE(at + 4) };
});
const setFiles = (await readdir(path.join(game.gamePath, 'resource/gd_PC'))).filter(name => /^ChaoSetRace.*\.bin$/i.test(name));
const sets = [];
for (const name of setFiles) {
  const relativePath = `resource/gd_PC/${name}`, bytes = await original(relativePath), count = bytes.readUInt32BE(0);
  if (32 + count * 32 !== bytes.length) throw new Error(`Unexpected native SET length: ${name}`);
  const entries = Array.from({ length: count }, (_, index) => {
    const at = 32 + index * 32, rawType = bytes.readUInt16BE(at), type = rawType & 0xfff;
    return { index, rawType, type, clipLevel: rawType >>> 12, objectName: objectTypes[type]?.name ?? null,
      rotationBams: [2,4,6].map(offset => bytes.readUInt16BE(at + offset)),
      position: [8,12,16].map(offset => bytes.readFloatBE(at + offset)), parameters: [20,24,28].map(offset => bytes.readFloatBE(at + offset)),
      canonicalPosition: Object.fromEntries(['x','y','z'].map((axis, i) => [axis, bytes.readFloatBE(at + 8 + i * 4) * 0.1])) };
  });
  sets.push({ filename: name, source: relativePath, count, entries, semanticStatus: 'Native object placements/controllers, not validated checkpoint gates.' });
}
await writeFile(path.join(dataOutput, 'race-sets.json'), `${JSON.stringify({ version: 1, objectTypes, sets }, null, 2)}\n`);
const cameraFiles = (await readdir(path.join(game.gamePath, 'resource/gd_PC'))).filter(name => /^ChaoRaceCam.*\.bin$/i.test(name));
const cameras = [];
for (const name of cameraFiles) {
  const relativePath = `resource/gd_PC/${name}`, bytes = await original(relativePath);
  if (bytes.length !== 15360) throw new Error(`Unexpected native race camera size: ${name}`);
  const slots = Array.from({ length: 80 }, (_, index) => {
    const at = index * 192, data = bytes.subarray(at, at + 192);
    return { index, nonzero: data.some(value => value !== 0), flags: [...data.subarray(0, 8)],
      rawHex: data.toString('hex'), bigEndianFloats: Array.from({ length: 48 }, (_, i) => { const value = data.readFloatBE(i * 4); return Number.isFinite(value) ? value : null; }),
      candidatePointBlocks: [8,40,52,104,136,148].map(offset => ({ byteOffset: offset, native: [0,4,8].map(i => data.readFloatBE(offset + i)) })), finalFloat188: data.readFloatBE(188) };
  });
  cameras.push({ filename: name, source: relativePath, bytes: bytes.length, candidateStride: 192, slotCount: 80,
    activeCandidateSlots: slots.filter(slot => slot.nonzero).length, slots,
    semanticStatus: 'Repeated192-byte segments inferred from original layout; point roles, mode enum, interpolation and follow/fixed/rail behavior remain unverified. Preserve as reference rather than execute as browser code.' });
}
await writeFile(path.join(dataOutput, 'race-cameras.json'), `${JSON.stringify({ version: 1, cameras }, null, 2)}\n`);

const courseDefs = [
  ['Crab Pool','BegSwm'], ['Stump Valley','BegFly'], ['Mushroom Forest','BegRun'], ['Block Canyon','BegPow'],
  ['Aquamarine','JwlSwm'], ['Topaz','JwlFly'], ['Peridot','JwlRun'], ['Garnet','JwlPow'], ['Onyx','JwlIL'], ['Diamond','JwlLng'],
];
const rawConfigs = Array.from({ length: 62 }, (_, index) => {
  const address = 0x01354828 + index * 20, at = atVA(address), raw = Array.from({ length: 5 }, (_, i) => exe.readInt32LE(at + i * 4));
  return { index, address: hex(address), raw, rivalSetIndex: raw[0], landtableIndex: raw[1], courseIndex: raw[2], pairedCourseIndex: raw[3], toyUnlock: raw[4] };
});
const courses = rawConfigs.map((config, index) => {
  const group = index < 12 ? 'beginner' : index < 42 ? 'jewel' : index < 54 ? 'challenge' : index < 58 ? 'hero' : 'dark';
  const groupIndex = group === 'beginner' ? Math.floor(index / 3) : group === 'jewel' ? 4 + Math.floor((index - 12) / 5) : config.courseIndex;
  const number = group === 'beginner' ? index % 3 + 1 : group === 'jewel' ? (index - 12) % 5 + 1 : group === 'challenge' ? index - 41 : group === 'hero' ? index - 53 : index - 57;
  const nativeCourse = courseDefs[groupIndex], id = `${group}-${group === 'beginner' || group === 'jewel' ? `${groupIndex}-` : ''}${number}`;
  return { id, name: group === 'beginner' || group === 'jewel' ? `${nativeCourse[0]} / Level${number}` : `${group[0].toUpperCase() + group.slice(1)} Race${number}`, group, level: number,
    source: { binary: 'sonic2app.exe', tableIndex: index, ...config, objectSet: `ChaoSetRace${nativeCourse[1]}.bin`, cameraSet: `ChaoRaceCam${nativeCourse[1]}.bin`, landtable: ['objLandTableRace','objLandTableRaceDark','objLandTableRaceHero'][config.landtableIndex] },
    available: false, geometryValidated: false, spawn: null, checkpoints: [], rewardRings: 25, rewardNote: 'Proposed MMO reward; native table toyUnlock is preserved separately.',
    gaps: ['Original route/path trigger order and start/finish gates', 'Per-course rival generation and native unlock rules', 'Native geometry/collision play validation'] };
});
const raceBots = Array.from({ length: 115 }, (_, index) => {
  const address = 0x011ce2a0 + index * 84, at = atVA(address);
  return { index, address: hex(address), appearanceBytes: [...exe.subarray(at, at + 16)], morphParameters: Array.from({ length: 8 }, (_, i) => exe.readFloatLE(at + 16 + i * 4)), statMode: exe.readInt16LE(at + 48), field50: exe.readInt16LE(at + 50), sharedStat: exe.readInt32LE(at + 52), perStatPairs: Object.fromEntries(['swim','fly','run','power','stamina','intelligence','luck'].map((name, i) => [name, [exe.readInt16LE(at + 56 + i * 4), exe.readInt16LE(at + 58 + i * 4)]])) };
});
await writeFile(path.join(dataOutput, 'races.json'), `${JSON.stringify({ version: 1, coordinateSpace: 'native coordinates ×0.1; no origin recentering', courses, raceBots, rawConfigs, evidence: ['Exant64/CWE al_race.h native Hero/Dark config addresses and20-byte schema', 'Contiguous original PC tables0x01354828–0x01354d00:12Beginner,30Jewel,12Challenge,4Hero,4Dark; repeated native course IDs and toy reward rows verified', 'Ngolinvaux/SA2Randomizer SA2Rando.h and SA2RStructs.h:115RaceBotData records at0x011ce2a0,stride84', 'Course display aliases follow known retail names; source file IDs retained independently.'], rulesGaps: ['Stamina/cheer/speed/obstacle equations', 'Bot selection/random stat ranges and difficulty curves', 'Native reward eligibility/unlock/medal rules', 'Authoritative source-derived path/gate chain'] }, null, 2)}\n`);

const landNames = ['objLandTableRace','objLandTableRaceDark','objLandTableRaceHero','objLandTableChaoKarate'];
const dllSections = sections(await readFile(path.join(configs, 'Chao_DLL.ini'), 'utf8')).filter(item => landNames.includes(item.name));
if (dllSections.length !== 4) throw new Error('Missing activity landtable maps.');
const dllMap = path.join(output, 'landtables.ini');
await writeFile(dllMap, `modulename=Data_DLL_orig\ngame=SA2B\n\n${dllSections.map(item => item.lines.join('\n')).join('\n')}\n`);
const support = [];
for (const config of ['CHAOSTGRACE.ini','CHAOSTGKARATE.ini']) support.push(...sections(await readFile(path.join(configs, config), 'utf8')).filter(item => ['chunkmodel','basicmodel','gcmodel','stagelightdatalist'].includes(item.fields.type)));
const exeMap = path.join(output, 'stage-props.ini');
await writeFile(exeMap, `game=SA2B\ndatafile=sonic2app.exe\n\n${support.map(item => item.lines.join('\n').replace(/^animations=.*$/gm, '')).join('\n')}\n`);
const options = { windowsHide: true, timeout: 120_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
await original('resource/gd_PC/DLL/Win32/Data_DLL.dll');
for (const [source, mapping, log] of [['resource/gd_PC/DLL/Win32/Data_DLL.dll',dllMap,'landtables'],['sonic2app.exe',exeMap,'props']]) {
  const result = await run('dotnet', [path.join(tools, 'bin/lib/split.dll'),'binary',path.join(game.gamePath,source),mapping,output], options);
  await writeFile(path.join(output, `${log}.log`), result.stdout + result.stderr);
}
for (const name of ['al_stg_race_tex','al_stg_race_dark_tex','al_stg_race_hero_tex','al_stg_karate_tex','al_race_obj_n_tex','al_race_obj_h_tex','al_race_obj_d_tex','al_karate_obj_tex','al_race_water_tex','al_race_water_dark_tex','al_race_water_hero_tex','al_race_map_tex']) {
  const source = `resource/gd_PC/${name}.prs`; await original(source);
  const copy = path.join(textures, `${name}.prs`); await copyFile(path.join(game.gamePath, source),copy);
  await run('dotnet',[path.join(tools,'bin/lib/ArchiveTool.dll'),'-png',copy],options);
}
for (const source of sources.values()) {
  source.unchanged = hash(await readFile(path.join(game.gamePath,source.relativePath))) === source.sha256;
  if (!source.unchanged) throw new Error(`Original source changed:${source.relativePath}`);
}
const manifest = { version: 1, sources: [...sources.values()], nativeLandtables: dllSections.map(item => ({ name:item.name,path:slash(path.relative(process.cwd(),path.join(output,item.fields.filename))) })), mappedPropSections: support.length, raceSets: sets.length, cameraFiles: cameras.length, raceConfigs: courses.length, raceBots: raceBots.length, karateAppearanceRows: appearanceRows.length, karateOpponents: opponents.length, note: 'Native data and geometry extraction only. No original garden exports changed. Unvalidated routes remain unavailable; original camera semantics and native gameplay formulas require further work.' };
await writeFile(path.join(output,'manifest.json'),`${JSON.stringify(manifest,null,2)}\n`);
console.log(JSON.stringify({ ...manifest, sources: `${sources.size}original files SHA-256 checked unchanged`, nativeLandtables: manifest.nativeLandtables.map(item=>item.name) },null,2));
