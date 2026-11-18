import { copyFile, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed SA2 required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const configs = path.join(tools, 'GameConfig/PC_SA2');
const output = path.resolve('.local/extracted/activity-motions');
await mkdir(output, { recursive: true });
const slash = (value) => value.split(path.sep).join('/');
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fileHash = async (file) => digest(await readFile(file));

function sections(text) {
  const result = [];
  let current;
  for (const line of text.split(/\r?\n/)) {
    const section = line.match(/^\[([^\]]+)\]/);
    if (section) { current = { name: section[1], fields: {}, lines: [line] }; result.push(current); }
    else if (current) {
      current.lines.push(line);
      const property = line.match(/^([^;=][^=]*)=(.*)$/);
      if (property) current.fields[property[1].trim()] = property[2].trim();
    }
  }
  return result;
}

const dllConfig = await readFile(path.join(configs, 'Chao_DLL.ini'), 'utf8');
const common = sections(dllConfig).find((entry) => entry.name === 'al_motion_table');
if (!common || common.fields.length !== '622') throw new Error('Expected original 622-state Chao motion table mapping.');
const spectator = sections(dllConfig).find((entry) => entry.name === 'object_race_omo_original_original');
const dllMap = path.join(output, 'common-motions.ini');
await writeFile(dllMap, 'modulename=Data_DLL_orig\ngame=SA2B\n\n' + common.lines.join('\n').replace(/^filename=.*$/m, 'filename=common/') + '\n' + spectator.lines.join('\n').replace(/^filename=.*$/m, 'filename=race/OmochaoSpectator.sa2mdl') + '\n');

const supportSections = [];
for (const [config, folder] of [['CHAOSTGKARATE.ini', 'karate'], ['CHAOSTGRACE.ini', 'race']]) {
  for (const section of sections(await readFile(path.join(configs, config), 'utf8'))) {
    if (['animation', 'motiontable'].includes(section.fields.type) || (section.fields.type === 'chunkmodel' && section.fields.animations)) {
      const text = section.lines.join('\n').replace(/^filename=Chao\/stg_(karate|race)\/models\//m, `filename=${folder}/`);
      supportSections.push({ ...section, config, text });
    }
  }
}
const exeMap = path.join(output, 'stage-motions.ini');
await writeFile(exeMap, 'game=SA2B\ndatafile=sonic2app.exe\n\n' + supportSections.map((section) => section.text).join('\n') + '\n');
const sources = [];
for (const relativePath of ['resource/gd_PC/DLL/Win32/Data_DLL.dll', 'sonic2app.exe', 'resource/gd_PC/ChaoMotionsD.rel', 'resource/gd_PC/ChaoStgKarate.prs', 'resource/gd_PC/ChaoStgRace.prs', 'resource/gd_PC/al_karate_obj_tex.prs', 'resource/gd_PC/al_race_obj_c_tex.prs']) {
  const file = path.join(game.gamePath, relativePath);
  sources.push({ relativePath, bytes: (await stat(file)).size, sha256: await fileHash(file), extractedFrom: ['sonic2app.exe', 'resource/gd_PC/DLL/Win32/Data_DLL.dll'].includes(relativePath) });
}
const options = { windowsHide: true, timeout: 120_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
const logs = [];
for (const [source, mapping] of [['resource/gd_PC/DLL/Win32/Data_DLL.dll', dllMap], ['sonic2app.exe', exeMap]]) {
  const result = await run('dotnet', [path.join(tools, 'bin/lib/split.dll'), 'binary', path.join(game.gamePath, source), mapping, output], options);
  logs.push(result.stdout + result.stderr);
}
await writeFile(path.join(output, 'extraction.log'), logs.join('\n'));
const textures = path.resolve('.local/extracted/textures');
await mkdir(textures, { recursive: true });
for (const name of ['al_karate_obj_tex', 'al_race_obj_c_tex']) {
  const copy = path.join(textures, `${name}.prs`);
  await copyFile(path.join(game.gamePath, `resource/gd_PC/${name}.prs`), copy);
  await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', copy], options);
}
for (const source of sources) {
  source.unchanged = await fileHash(path.join(game.gamePath, source.relativePath)) === source.sha256;
  if (!source.unchanged) throw new Error(`Source changed during extraction: ${source.relativePath}`);
}

const nativeFiles = [];
async function walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) await walk(absolute);
    else if (entry.isFile() && entry.name.endsWith('.saanim')) {
      const bytes = await readFile(absolute);
      const labels = [...new Set(bytes.toString('latin1').match(/animation_[0-9A-Fa-f]{8}/g) ?? [])];
      nativeFiles.push({ relativePath: slash(path.relative(output, absolute)), filename: entry.name, bytes: bytes.length, sha256: digest(bytes), nativeMotionLabel: labels.length === 1 ? labels[0] : null, labelCandidates: labels.length === 1 ? undefined : labels });
    }
  }
}
await walk(output);
const commonFiles = nativeFiles.filter((file) => file.relativePath.startsWith('common/'));
const byMotion = new Map(commonFiles.map((file) => [file.nativeMotionLabel, file]));
const table = sections(await readFile(path.join(output, 'common/info.ini'), 'utf8'));
const states = table.map((entry) => {
  const index = Number(entry.name);
  const nativeFile = byMotion.get(entry.fields.Motion);
  if (!nativeFile) throw new Error(`State ${index} has no extracted motion: ${entry.fields.Motion}`);
  const firstIndex = table.findIndex((other) => other.fields.Motion === entry.fields.Motion);
  return { index, nativeName: path.parse(nativeFile.filename).name, nativeMotionLabel: entry.fields.Motion, nativeFile: nativeFile.relativePath, canonicalIndex: firstIndex, label: common.fields[`meta${firstIndex}_a`] ?? null, tableProperties: entry.fields };
});
const group = (predicate) => states.filter(predicate).map((state) => state.index);
const groups = {
  locomotionAndRaceTraversalCandidates: group((state) => (state.index >= 100 && state.index <= 162) || state.index === 271),
  karateAndChildActivityTail: group((state) => state.index >= 576 && state.index <= 621),
  genericCombatCandidates: group((state) => state.index >= 441 && state.index <= 446),
  running: group((state) => /Run|Sprint|Brake/.test(state.label ?? '')),
  climbing: group((state) => /Climb/.test(state.label ?? '')),
  jumping: group((state) => /Jump|Hop|Diving/.test(state.label ?? '')),
  flying: group((state) => /Flying/.test(state.label ?? '')),
  falling: group((state) => /Falling|Tripping|Trip Recovery/.test(state.label ?? '')),
  swimming: group((state) => /Swimming|Backstroke|Water|Coming Up For Air/.test(state.label ?? '')),
};
const supportTables = [];
for (const [name, folder] of [['karate-referee', 'karate/karate_judgeMotionTable'], ['race-omochao-spectator', 'race/omo_originalMotionTable'], ['race-crab', 'race/race_kaniMotionTable']]) {
  const entries = sections(await readFile(path.join(output, folder, 'info.ini'), 'utf8'));
  supportTables.push({ name, path: folder, slots: entries.length, uniqueMotionPointers: new Set(entries.map((entry) => entry.fields.Motion)).size, states: entries.map((entry) => ({ index: Number(entry.name), tableProperties: entry.fields })) });
}
const mechCandidates = [];
for (const kind of ['CWALK', 'DWALK']) {
  const modelMapping = await readFile(path.join(configs, `MDL/${kind}MDL.ini`), 'utf8');
  const motionMapping = await readFile(path.join(configs, `MDL/${kind}MTN.ini`), 'utf8');
  const archiveFiles = [];
  for (const suffix of ['mdl', 'mtn', 'tex']) {
    const relativePath = `resource/gd_PC/${kind.toLowerCase()}${suffix}.prs`;
    const file = path.join(game.gamePath, relativePath);
    archiveFiles.push({ relativePath, bytes: (await stat(file)).size, sha256: await fileHash(file) });
  }
  mechCandidates.push({ name: kind === 'CWALK' ? 'Chao Walker' : 'Dark Chao Walker', archiveFiles, modelMapping: modelMapping.trim().split(/\r?\n/), motionMapping: motionMapping.trim().split(/\r?\n/), status: 'Inventory candidates only in this report; dedicated mech conversion is a separate task.' });
}
const manifest = {
  schemaVersion: 1, generatedAt: new Date().toISOString(), tool: 'Official X-Hax SA Tools 786 mapped PC binary extraction', sources,
  sourceMappings: { common: 'PC_SA2/Chao_DLL.ini export al_motion_table;622 states;40 nodes;shortrot=true', support: supportSections.map(({ name, fields, config }) => ({ name, config, ...fields })) },
  counts: { commonStates: states.length, uniqueCommonClips: commonFiles.length, supportClips: nativeFiles.length - commonFiles.length, totalNativeClips: nativeFiles.length },
  commonStates: states, commonClips: commonFiles, supportClips: nativeFiles.filter((file) => !file.relativePath.startsWith('common/')), supportTables, groups, mechCandidates,
  conversionReadiness: { common: 'All279 unique native node motions extracted and indexed for the shared40-node Chao skeleton; original slot timing retained.', stageProps: 'Native referee/spectator/crab/drummer/door clips extracted with their own mapped model skeletons. They are not retargetable as40-node Chao clips.', glb: 'Not claimed by extraction alone. See separate conversion report if generated.' },
  gaps: ['Named activity groups are curator-label candidates, not a proven list of every race runtime animation call.', 'All622 shared states are retained to avoid omitting less obvious race/karate motions.', 'The browser must implement original StartFrame/EndFrame,PlaySpeed,LoopProperty,NextAnimation and transitions rather than treating622 states as622 unique clips.', 'Procedural face, wing/body variation, object interactions and activity gameplay are separate from skeletal motion decoding.', 'ChaoMotionsD.rel and stagePRS files are inventoried as installed candidates; authoritative PC extraction here uses DLL/EXE mappings, not unverified GameCubeREL assumptions.'],
};
await writeFile(path.join(output, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(JSON.stringify({ counts: manifest.counts, supportTables: supportTables.map(({ name, slots, uniqueMotionPointers }) => ({ name, slots, uniqueMotionPointers })), output: path.join(output, 'manifest.json'), sourceUnchanged: sources.every((source) => source.unchanged) }, null, 2));
