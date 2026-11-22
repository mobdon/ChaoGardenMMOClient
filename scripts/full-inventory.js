import { createReadStream } from 'node:fs';
import { mkdir, readdir, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { detectGameFiles } from './game-files.js';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const archiveExtensions = new Set(['.prs', '.pvm', '.pvmx', '.gvm', '.pak', '.afs', '.csb', '.cab', '.zip', '.7z']);
const textureExtensions = new Set(['.gvr', '.pvr', '.dds', '.png', '.bmp', '.tga', '.jpg', '.jpeg', '.pvm', '.pvmx', '.gvm']);
const audioExtensions = new Set(['.adx', '.ahx', '.wav', '.ogg', '.mp3', '.aax', '.mlt', '.mpb', '.sfd', '.hca', '.csb', '.mid', '.m1v', '.afs']);
const normalized = (value) => value.split(path.sep).join('/');

export function classifyFile(relativePath) {
  const lower = relativePath.toLowerCase();
  const name = path.posix.basename(lower);
  const extension = path.posix.extname(lower) || '(none)';
  let category = 'other';
  if (lower.startsWith('redist/')) category = 'redistributable';
  else if (lower.includes('/shader/')) category = 'shader';
  else if (audioExtensions.has(extension) || /(^|\/)(adx|sound|music|voice)(\/|_)/.test(lower)) category = 'audio-video';
  else if (extension === '.gvp') category = 'palette';
  else if (textureExtensions.has(extension) || /(tex|texture)/.test(name)) category = 'texture-candidate';
  else if (/^(al_|chao)|chao/i.test(name)) category = 'chao-data-candidate';
  else if (/^(landtable|stg\d|set\d|set_|objland)/.test(name) || extension === '.sa2lvl') category = 'level-candidate';
  else if (/motion|mtn/.test(name) || ['.saanim', '.motion'].includes(extension)) category = 'animation-candidate';
  else if (/^e\d{4}/.test(name) || /event/.test(name)) category = 'event-candidate';
  else if (['.exe', '.dll'].includes(extension)) category = 'executable-data';
  else if (['.ini', '.xml', '.vdf', '.txt', '.cfg', '.conf', '.inf', '.json'].includes(extension)) category = 'configuration-text';
  else if (['.prs', '.rel', '.bin', '.pak', '.afs'].includes(extension)) category = 'packed-data';

  let archiveCandidateGroup = null;
  if (archiveExtensions.has(extension) || extension === '.rel') {
    if (/^al_body\./.test(name)) archiveCandidateGroup = 'chao-body-textures';
    else if (/^al_eye\./.test(name)) archiveCandidateGroup = 'chao-eye-textures';
    else if (/^al_icon\./.test(name)) archiveCandidateGroup = 'chao-emotion-sprites';
    else if (/neut/.test(name)) archiveCandidateGroup = 'chao-neutral-garden';
    else if (/hero/.test(name)) archiveCandidateGroup = 'chao-hero-garden';
    else if (/dark/.test(name) && /^al_|chao/.test(name)) archiveCandidateGroup = 'chao-dark-garden';
    else if (/karate/.test(name)) archiveCandidateGroup = 'chao-karate';
    else if (/race/.test(name) && /^al_|chao/.test(name)) archiveCandidateGroup = 'chao-races';
    else if (/^(al_|chao)/.test(name)) archiveCandidateGroup = 'chao-other';
    else if (/^objtex_stg08\./.test(name)) archiveCandidateGroup = 'security-hall-objects-and-emerald-textures';
    else if (/^objtex_|^stg\d.*objtex/.test(name)) archiveCandidateGroup = 'stage-object-textures';
    else if (['.csb', '.afs'].includes(extension)) archiveCandidateGroup = 'audio-banks';
    else if (/^e\d{4}|event/.test(name)) archiveCandidateGroup = 'cutscene-events';
    else if (lower.startsWith('redist/')) archiveCandidateGroup = 'redistributables';
    else if (/tex/.test(name)) archiveCandidateGroup = 'other-textures';
    else archiveCandidateGroup = 'other-packed-data';
  }
  return { extension, category, archiveCandidateGroup };
}

async function hashFile(filePath) {
  const before = await stat(filePath);
  const hash = createHash('sha256');
  let bytesRead = 0;
  for await (const chunk of createReadStream(filePath)) {
    bytesRead += chunk.length;
    hash.update(chunk);
  }
  const after = await stat(filePath);
  if (bytesRead !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs) {
    throw new Error('File changed while being read; rerun when Steam has finished updating.');
  }
  return { size: before.size, bytesRead, sha256: hash.digest('hex') };
}

async function enumerate(root) {
  const files = [];
  const skipped = [];
  async function walk(directory) {
    let entries;
    try { entries = await readdir(directory, { withFileTypes: true }); }
    catch (error) { skipped.push({ path: normalized(path.relative(root, directory)), reason: error.message }); return; }
    entries.sort((a, b) => a.name.localeCompare(b.name, 'en'));
    for (const entry of entries) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) await walk(absolute);
      else if (entry.isFile()) files.push(normalized(path.relative(root, absolute)));
      else skipped.push({ path: normalized(path.relative(root, absolute)), reason: 'Link or non-regular file; not followed.' });
    }
  }
  await walk(root);
  return { files: files.sort(), skipped };
}

function group(records, key) {
  const result = {};
  for (const record of records) {
    const value = key(record);
    if (value === null) continue;
    const entry = result[value] ??= { count: 0, bytes: 0 };
    entry.count++;
    entry.bytes += record.size ?? 0;
  }
  return Object.fromEntries(Object.entries(result).sort(([a], [b]) => a.localeCompare(b, 'en')));
}

async function assetManifest(records) {
  const byPath = new Map(records.map((record) => [record.relativePath.toLowerCase(), record]));
  const source = (relativePath) => {
    const entry = byPath.get(relativePath.toLowerCase());
    return { relativePath, present: Boolean(entry), sha256: entry?.sha256 ?? null, size: entry?.size ?? null };
  };
  const output = async (relativePath) => {
    try { return { relativePath, present: true, ...await hashFile(path.join(projectRoot, relativePath)) }; }
    catch (error) { return { relativePath, present: false, reason: error.code === 'ENOENT' ? 'Not generated' : error.message }; }
  };
  const dll = 'resource/gd_PC/DLL/Win32/Data_DLL.dll';
  const body = 'resource/gd_PC/al_body.prs';
  const eyes = 'resource/gd_PC/al_eye.prs';
  const variants = [['chao', 'ncz', 0], ['neutral-chaos', 'nchaos', 108], ['hero-chaos', 'hchaos', 114], ['dark-chaos', 'dchaos', 120], ['tails', 'tails', 126], ['knuckles', 'knuckles', 132], ['amy', 'amy', 138]];
  const entries = [];
  for (const [name, nativeName, index] of variants) entries.push({
    name,
    sources: [source(dll), source(body), source(eyes), ...(name === 'chao' ? [source('resource/gd_PC/al_nc00.GVP')] : [])],
    mapping: `PC_SA2/Chao_DLL.ini: AL_RootObject[${index}]`,
    nativeModel: `.local/extracted/pc-chao/Chao/models/AL_RootObject/al_${nativeName}.sa2mdl`,
    textures: `.local/extracted/textures/al_body + al_eye${name === 'chao' ? '; original al_nc00.GVP banks 0/1/2' : ''}`,
    animations: ['alm_tati.saanim (Idle)', 'alm_aruki_ncz.saanim (Walk)'],
    adaptations: ['Omit two half-closed eyelid overlays', ['tails', 'knuckles', 'amy', 'chao'].includes(name) ? 'Preserve original white eye texture and dark pupils' : 'Preserve original Chaos Chao eye texture', 'Opaque body materials; no texture masks geometry', 'Omit alternative mouth overlays in default face'],
    outputs: [await output(`.local/imports/${name}.glb`)],
  });
  for (const [name, archive, exportName] of [['garden', 'neut', 'objLandTableNeut'], ['hero-garden', 'hero', 'objLandTableHero'], ['dark-garden', 'dark', 'objLandTableDark']]) entries.push({
    name,
    sources: [source(dll), source(`resource/gd_PC/al_stg_${archive}_tex.prs`)],
    mapping: `PC_SA2/Chao_DLL.ini: ${exportName}`,
    textures: `.local/extracted/textures/al_stg_${archive}_tex`,
    outputs: [await output(`.local/imports/${name}.glb`)],
  });
  entries.push({ name: 'original-chao-head-flame', sources: [source('resource/gd_PC/al_icon.prs')], textures: '.local/extracted/textures/al_icon/torch_blue00.png through torch_blue15.png', adaptation: 'Lossless 16-frame atlas, 512x32 RGBA', outputs: [await output('.local/imports/chao-flame.png')] });
  entries.push({ name: 'native-hub-grass', sources: [source('resource/gd_PC/al_stg_neut_tex.prs')], texture: '.local/extracted/textures/al_stg_neut_tex/gneut03.png', archiveGlobalIndex: 120003, outputs: [await output('.local/imports/nativehubgrass.png')] });
  entries.push({ name: 'security-hall-chaos-emerald', runtimeUse: false, note: 'Earlier pickup-model experiment; superseded by actual cinematic models for the orbiting emeralds.', sources: [source('sonic2app.exe'), source('resource/gd_PC/objtex_stg08.prs')], mapping: 'PC_SA2/STG08.ini; geometry A8CA7C; red texlist A8BC38; purple A8BC4C; cyan A8BC60', nativeModel: '.local/extracted/emerald/chaos-emerald.sa2mdl', texlist: '.local/extracted/emerald/emerald-red.satex', texture: '.local/extracted/textures/objtex_stg08/miu64_sh013.png', outputs: [await output('.local/imports/chaos-emerald.glb')] });
  try {
    const { readFile } = await import('node:fs/promises');
    const cinematic = JSON.parse(await readFile(path.join(projectRoot, '.local/imports/cinematic-conversion-report.json'), 'utf8'));
    for (const model of cinematic) entries.push({
      name: model.name, runtimeUse: true,
      sources: model.cinematic ? [source('resource/gd_PC/event/e0207.prs'), source('resource/gd_PC/event/e0207texture.prs')] : [source('sonic2app.exe'), source('resource/gd_PC/al_neut_obj_tex.prs')],
      mapping: model.cinematic ? 'Actual e0207 cinematic Scene 2 entities 2–8; original texture IDs2–8' : 'PC_SA2/CHAOSTGNEUT.ini; F27C6C ChaoChunk NeutralGardenTree',
      nativeModel: normalized(model.nativeModel), materials: model.materials,
      meshCount: model.meshCount, vertices: model.vertices,
      outputs: [await output(normalized(model.output))],
    });
  } catch { /* Cinematic/prop conversion can run after initial installation inventory. */ }
  try {
    const { readFile } = await import('node:fs/promises');
    const textureManifest = JSON.parse(await readFile(path.join(projectRoot, '.local/imports/native-textures.json'), 'utf8'));
    entries.push({ name: 'native-emerald-and-grass-textures', manifest: textureManifest, outputs: await Promise.all(['red', 'purple', 'cyan', 'green', 'blue', 'yellow', 'white'].map((color) => output(`.local/imports/emerald-${color}.png`))) });
  } catch { /* Texture audit can run independently; missing manifest is explicit in notes. */ }
  return entries;
}

async function main() {
  const startedAt = new Date().toISOString();
  const game = await detectGameFiles(process.argv[2] || process.env.SA2_GAME_PATH);
  if (!game.found) throw new Error('SA2 installation not found. Pass the installation folder or set SA2_GAME_PATH.');
  const root = path.resolve(game.gamePath);
  const outputPath = path.join(projectRoot, '.local/full-inventory.json');
  if (outputPath.startsWith(`${root}${path.sep}`)) throw new Error('Inventory output must be outside the source installation.');
  console.log(`Reading every regular file under ${root}`);
  const { files, skipped } = await enumerate(root);
  const records = new Array(files.length);
  const errors = [];
  let cursor = 0;
  let completed = 0;
  async function worker() {
    while (cursor < files.length) {
      const index = cursor++;
      const relativePath = files[index];
      try { records[index] = { relativePath, ...classifyFile(relativePath), ...await hashFile(path.join(root, relativePath)) }; }
      catch (error) { records[index] = { relativePath, ...classifyFile(relativePath), error: error.message }; errors.push({ relativePath, error: error.message }); }
      completed++;
      if (completed % 500 === 0) console.log(`Hashed ${completed}/${files.length} files`);
    }
  }
  await Promise.all(Array.from({ length: 4 }, worker));
  const second = await enumerate(root);
  const treeChanged = JSON.stringify(files) !== JSON.stringify(second.files);
  const totalBytes = records.reduce((total, record) => total + (record.size ?? 0), 0);
  const result = {
    schemaVersion: 1, startedAt, finishedAt: new Date().toISOString(), installRoot: root,
    scope: 'Every regular file recursively under the entire installed SA2 root, including executable, shaders, configuration, redist and all resource folders.',
    complete: errors.length === 0 && skipped.length === 0 && second.skipped.length === 0 && !treeChanged,
    sourceReadOnly: true, hashAlgorithm: 'SHA-256',
    totals: { discoveredFiles: files.length, hashedFiles: records.filter((record) => record.sha256).length, totalBytes, bytesRead: records.reduce((total, record) => total + (record.bytesRead ?? 0), 0) },
    grouped: { category: group(records, (record) => record.category), extension: group(records, (record) => record.extension), topLevel: group(records, (record) => record.relativePath.includes('/') ? record.relativePath.split('/')[0] : '(installation root)'), archiveCandidateGroup: group(records, (record) => record.archiveCandidateGroup) },
    battle: game.battle,
    notes: ['Filename categories and archive groups are candidates, not proof of decoded contents.', 'A complete byte inventory does not mean every archive has been decompressed or every asset converted.', 'Installed resources alone do not verify Steam or BATTLE entitlement.', 'Source installation is opened for reads only; hashes identify the local snapshot.', 'Original assets, derived files and this machine-specific inventory remain ignored under .local/.'],
    treeChangedDuringScan: treeChanged, skipped, skippedAtFinalEnumeration: second.skipped, errors,
    conversionManifest: await assetManifest(records), files: records,
  };
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ complete: result.complete, totals: result.totals, categories: result.grouped.category, output: outputPath }, null, 2));
  if (!result.complete) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
