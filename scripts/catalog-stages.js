import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tools = path.resolve(process.env.SA_TOOLS_DIR || path.join(root, '.local/tools/sa-tools-786'));
const mappingPath = path.join(tools, 'GameConfig/PC_SA2/sa2lvl.ini');
const mapping = await readFile(mappingPath, 'utf8');
const inventory = JSON.parse(await readFile(path.join(root, '.local/full-inventory.json'), 'utf8'));
const files = inventory.files.map((file) => file.relativePath);
const sections = [];
let current;
for (const line of mapping.split(/\r?\n/)) {
  const heading = line.match(/^\[([^\]]+)\]/);
  if (heading) { current = { section: heading[1], fields: {} }; sections.push(current); }
  else if (current) {
    const property = line.match(/^([^;=][^=]*)=(.*)$/);
    if (property) current.fields[property[1].trim()] = property[2].trim();
  }
}
const slug = (name) => name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
const basename = (file) => path.posix.basename(file).toLowerCase();
const charName = (value) => ({ MechTails: 'Tails', MechEggman: 'Eggman' }[value] || value || 'Chao');
const familyFor = (group, character) => group === 'Chao World' ? 'chao' : group === 'Bosses' ? 'boss' : group === 'Extra Stages' && character === 'Sonic' ? 'speed' : ['MechTails', 'MechEggman'].includes(character) ? 'mech' : ['Knuckles', 'Rouge'].includes(character) ? 'hunt' : ['Sonic', 'Shadow'].includes(character) ? 'speed' : 'other';

function stageResources(id, fields) {
  const short = String(id).padStart(2, '0');
  const long = String(id).padStart(4, '0');
  const declared = new Set([fields.Textures, fields.SecondaryTextures].filter(Boolean).flatMap((value) => value.split(',')).map((value) => value.toLowerCase()));
  return files.filter((file) => {
    const name = basename(file);
    const stem = name.replace(/\.[^.]+$/, '');
    return declared.has(stem) || new RegExp(`^(?:stg${short}(?:[^0-9]|$)|objtex_stg${short}(?:[^0-9]|$)|bgtex${short}(?:[^0-9]|$)|landtx${short}(?:[^0-9]|$)|stg_title${short}(?:[^0-9]|$)|set${long}(?:[^0-9]|$)|camset${long}(?:[^0-9]|$))`, 'i').test(name);
  });
}

const chaoResourcePatterns = {
  'Chao Lobby': /chaostglobby|al_stg_lobby/i,
  'Neutral Garden': /chaostgneut|al_stg_neut|al_neut_obj/i,
  'Hero Garden': /chaostghero|al_stg_hero|al_hero_obj/i,
  'Dark Garden': /chaostgdark|al_stg_dark|al_dark_obj/i,
  'Chao Race Entrance': /chaostgentrance|al_stg_entrance|al_ent_/i,
  'Normal Chao Race': /chaostgrace\.|al_stg_race_tex|al_race_obj_n|al_race_water_tex/i,
  'Hero Chao Race': /chaostgracehero|al_stg_race_hero|al_race_obj_h|al_race_water_hero/i,
  'Dark Chao Race': /chaostgracedark|al_stg_race_dark|al_race_obj_d|al_race_water_dark/i,
  'Chao Stadium': /chaostgstadium|al_stg_stadium/i,
  'Chao Karate': /chaostgkarate|al_stg_karate|al_karate_obj/i,
};
const stages = [];
for (const { section, fields } of sections.filter((entry) => entry.fields.LevelID)) {
  const parts = section.split(/\\+/);
  const group = parts[0];
  const originalName = parts.at(-1);
  const nativeStageId = Number.parseInt(fields.LevelID, 10);
  const chao = group === 'Chao World';
  const family = nativeStageId === 0 ? 'other' : familyFor(group, fields.DefaultCharacter);
  let resources = stageResources(nativeStageId, fields);
  if (chao) {
    const matcher = chaoResourcePatterns[originalName] || /chaostgkinder|al_stg_kinder/i;
    resources = files.filter((file) => matcher.test(basename(file)));
    for (const pack of (fields.Textures || '').split(',')) resources.push(...files.filter((file) => basename(file) === `${pack.toLowerCase()}.prs`));
  }
  let character = charName(fields.DefaultCharacter);
  if (/Sonic vs Shadow/.test(originalName)) character = 'Sonic / Shadow';
  else if (/Tails vs Eggman/.test(originalName)) character = 'Tails / Eggman';
  else if (/Knuckles vs Rouge/.test(originalName)) character = 'Knuckles / Rouge';
  else if (originalName === 'Final Hazard') character = 'Super Sonic / Super Shadow';
  const id = chao ? `chao-${slug(originalName)}` : nativeStageId;
  const name = group.startsWith('2P ') ? `${originalName} (2P)` : originalName;
  stages.push({ id, name, character, family, resources: [...new Set(resources)].sort(), available: false, nativeStageId, group, sourceDefaultCharacter: fields.DefaultCharacter || null, adaptationCandidates: family === 'mech' ? ['Chao Walker', 'Dark Chao Walker'] : family === 'hunt' ? ['Chao climbing/gliding/digging adaptation'] : family === 'speed' ? ['Chao running/jumping/flying adaptation'] : [], evidence: [`X-Hax SA Tools786: PC_SA2/sa2lvl.ini [${section.replaceAll('\\\\', '/')}]`, fields.LevelGeometry ? `Native mapping: ${fields.LevelGeometry}` : 'No editor landtable path provided; native runtime index only.'] });
}
// These internal test IDs are named by the primary mod-loader native enum.
for (const [id, name, character] of [[1, 'Knuckles Test (internal)', 'Knuckles'], [2, 'Sonic Test (internal)', 'Sonic']]) {
  stages.push({ id, name, character, family: 'other', resources: stageResources(id, {}), available: false, nativeStageId: id, group: 'Internal tests', evidence: [`X-Hax sa2-mod-loader SA2Enums.h: LevelIDs_${id === 1 ? 'KnucklesTest' : 'SonicTest'}`] });
}
const kartResources = files.filter((file) => /^(cart|kart|bgcart|setcart|resultcart|course)/i.test(basename(file)));
for (const [id, name] of [[70, 'Route 101 / Route 280 (shared mode)'], [71, 'Kart Race']]) stages.push({ id, name, character: id === 70 ? 'Tails / Rouge' : 'Kart roster', family: 'kart', resources: kartResources, available: false, nativeStageId: id, group: 'Kart', evidence: ['X-Hax sa2-mod-loader SA2Enums.h: LevelIDs_Route101280=70; LevelIDs_KartRace=71', 'X-Hax SA Tools786: PC_SA2/CART.ini and CART_DLL.ini'] });
for (const [courseId, name] of ['Route 101', 'Route 280', 'Beginner', 'Standard', 'Expert', 'Download Course (internal)'].entries()) stages.push({ id: `kart-course-${courseId}`, name, character: courseId === 0 ? 'Tails' : courseId === 1 ? 'Rouge' : 'Kart roster', family: 'kart', resources: kartResources, available: false, nativeStageId: courseId < 2 ? 70 : 71, nativeCourseId: courseId, group: 'Kart courses', evidence: ['X-Hax sa2-mod-loader SA2Enums.h: KartCourseIDs0–5; download slot presence does not verify an installed downloadable course.'] });
for (const file of files.filter((file) => /^ChaoSetRace(?:Beg|Jwl).*\.bin$/i.test(path.posix.basename(file)))) {
  const internal = path.posix.basename(file, '.bin');
  const cameraName = internal.replace(/^ChaoSetRace/i, 'ChaoRaceCam');
  const camera = files.find((candidate) => path.posix.basename(candidate).toLowerCase() === `${cameraName.toLowerCase()}.bin`);
  stages.push({ id: `chao-course-${slug(internal)}`, name: internal, character: 'Chao', family: 'chao', resources: [file, ...(camera ? [camera] : [])], available: false, nativeStageId: 90, group: 'Chao race course sets', internalLabel: true, evidence: ['Installed original race set/camera filenames. Retail course display-name mapping is not inferred from abbreviations.'] });
}
const ids = stages.map((stage) => String(stage.id));
if (new Set(ids).size !== ids.length) throw new Error('Catalog IDs must be unique.');
if (!stages.every((stage) => stage.available === false)) throw new Error('Indexed native catalog must not imply implemented missions.');
for (const stage of stages) for (const resource of stage.resources) if (!files.includes(resource)) throw new Error(`Unverified resource path: ${resource}`);
const catalog = { version: 1, evidence: { mapping: 'Official X-Hax SA Tools release786 PC_SA2/sa2lvl.ini', nativeIds: 'https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Enums.h', inventory: 'Every listed resource path verified against the installed-file inventory; no original binaries are included.', status: 'All native missions/arenas/courses are catalogued for future adaptation. The currently playable prototype routes are separate.' }, stages };
await mkdir(path.join(root, 'shared'), { recursive: true });
await writeFile(path.join(root, 'shared/stage-catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);
const counts = stages.reduce((result, stage) => { result[stage.family] = (result[stage.family] || 0) + 1; return result; }, {});
console.log(JSON.stringify({ stages: stages.length, byFamily: counts, installedResourceReferences: stages.reduce((sum, stage) => sum + stage.resources.length, 0), output: 'shared/stage-catalog.json' }, null, 2));
