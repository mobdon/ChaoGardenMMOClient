import { access, readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const runFile = promisify(execFile);
const MAX_ASSET_SAMPLES = 500;
const BATTLE_CONTENT = [
  'ChaoStgKarate.prs',
  'al_stg_karate_tex.prs',
  'al_karate_obj_tex.prs',
];
const CHAO_EXPORTS = ['AL_RootObject', 'al_motion_table', 'objLandTableNeut', 'objLandTableHero', 'objLandTableDark', 'objLandTableChaoKarate'];
const SPECIAL_MODELS = [
  { index: 0, name: 'Neutral Child Chao', filename: 'al_ncz' },
  { index: 108, name: 'Neutral Chaos Chao', filename: 'al_nchaos' },
  { index: 114, name: 'Hero Chaos Chao', filename: 'al_hchaos' },
  { index: 120, name: 'Dark Chaos Chao', filename: 'al_dchaos' },
  { index: 126, name: 'Tails Chao', filename: 'al_tails' },
  { index: 132, name: 'Knuckles Chao', filename: 'al_knuckles' },
  { index: 138, name: 'Amy Chao', filename: 'al_amy' },
];

async function exists(filePath) {
  try { await access(filePath); return true; } catch { return false; }
}

/** Steam resource discovery only. This does not authenticate a Steam account. */
async function steamRoots() {
  const roots = new Set();
  const add = (value) => { if (value) roots.add(path.resolve(value)); };
  add(process.env.STEAM_PATH);
  if (process.platform === 'win32') {
    try {
      const { stdout } = await runFile('reg.exe', ['query', 'HKCU\\Software\\Valve\\Steam', '/v', 'SteamPath'], {
        windowsHide: true, timeout: 3000,
      });
      add(stdout.match(/SteamPath\s+REG_SZ\s+([^\r\n]+)/i)?.[1]?.trim());
    } catch { /* Standard installation paths remain available. */ }
    add(path.join(process.env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)', 'Steam'));
    add(path.join(process.env.ProgramFiles || 'C:\\Program Files', 'Steam'));
  } else if (process.platform === 'darwin') {
    add(path.join(os.homedir(), 'Library', 'Application Support', 'Steam'));
  } else {
    add(path.join(os.homedir(), '.steam', 'steam'));
    add(path.join(os.homedir(), '.local', 'share', 'Steam'));
  }
  for (const root of [...roots]) {
    try {
      const vdf = await readFile(path.join(root, 'steamapps', 'libraryfolders.vdf'), 'utf8');
      for (const match of vdf.matchAll(/"path"\s+"([^"\r\n]+)"/gi)) {
        add(match[1].replace(/\\\\/g, '\\'));
      }
    } catch { /* A Steam root may not have a library manifest yet. */ }
  }
  return [...roots];
}

async function resolveInstall(requestedPath) {
  if (requestedPath) return path.resolve(requestedPath);
  const roots = await steamRoots();
  for (const root of roots) {
    let installName = 'Sonic Adventure 2';
    try {
      const manifest = await readFile(path.join(root, 'steamapps', 'appmanifest_213610.acf'), 'utf8');
      const declared = manifest.match(/"installdir"\s+"([^"\r\n]+)"/i)?.[1];
      // A manifest is data, never an arbitrary path to traverse.
      if (declared && !/[\\/]/.test(declared) && declared !== '..') installName = declared;
    } catch { /* An installation can be discovered without a manifest. */ }
    const candidate = path.join(root, 'steamapps', 'common', installName);
    if (await exists(path.join(candidate, 'sonic2app.exe'))) return candidate;
  }
  return null;
}

function categoryFor(relativePath) {
  const name = path.basename(relativePath).toLowerCase();
  const ext = path.extname(name);
  if (/^(al_|chao|set_chao)|chao/.test(name)) return 'chao';
  if (/^(landtable|stg\d|set\d|set_|objland)/.test(name) || /\.sa2lvl$/.test(name)) return 'levels';
  if (/^(.*mtn|motion)|motion/.test(name) || ['.saanim', '.motion'].includes(ext)) return 'animations';
  if (['.adx', '.ahx', '.wav', '.ogg', '.mp3', '.aax', '.mlt', '.mpb'].includes(ext)) return 'audio';
  if (/(tex|texture)/.test(name) || ['.gvr', '.gvp', '.pvr', '.pvm', '.pvmx', '.dds', '.png'].includes(ext)) return 'textures';
  return 'other';
}

async function inventoryResources(resourcePath) {
  const inventory = { chao: 0, levels: 0, textures: 0, audio: 0, animations: 0, other: 0, total: 0, totalBytes: 0 };
  const assets = [];
  const walk = async (directory) => {
    let entries;
    try { entries = await readdir(directory, { withFileTypes: true }); } catch { return; }
    for (const entry of entries) {
      const filePath = path.join(directory, entry.name);
      // Do not follow links outside the selected installation.
      if (entry.isDirectory()) await walk(filePath);
      else if (entry.isFile()) {
        try {
          const info = await stat(filePath);
          const relativePath = path.relative(resourcePath, filePath).split(path.sep).join('/');
          const category = categoryFor(relativePath);
          inventory[category]++;
          inventory.total++;
          inventory.totalBytes += info.size;
          assets.push({ relativePath: `resource/gd_PC/${relativePath}`, size: info.size, category });
        } catch { /* Files can disappear while Steam updates them. */ }
      }
    }
  };
  await walk(resourcePath);
  const priority = { chao: 0, levels: 1, animations: 2, textures: 3, audio: 4, other: 5 };
  assets.sort((a, b) => priority[a.category] - priority[b.category] || a.relativePath.localeCompare(b.relativePath));
  return { inventory, assets: assets.slice(0, MAX_ASSET_SAMPLES), assetsTruncated: assets.length > MAX_ASSET_SAMPLES };
}

async function inspectChaoData(resourcePath) {
  const source = 'resource/gd_PC/DLL/Win32/Data_DLL.dll';
  const empty = { source, found: false, exports: [], specialModels: [] };
  if (!resourcePath) return empty;
  try {
    const data = await readFile(path.join(resourcePath, 'DLL', 'Win32', 'Data_DLL.dll'));
    const pe = data.readUInt32LE(0x3c);
    if (data.toString('ascii', pe, pe + 4) !== 'PE\0\0') return empty;
    const optional = pe + 24;
    // SA2's supported Windows executable format is PE32. Never load the DLL.
    if (data.readUInt16LE(optional) !== 0x10b) return { ...empty, found: true, note: 'Unsupported PE format.' };
    const imageBase = data.readUInt32LE(optional + 28);
    const sections = [];
    const sectionCount = data.readUInt16LE(pe + 6);
    const sectionStart = optional + data.readUInt16LE(pe + 20);
    for (let i = 0; i < sectionCount; i++) {
      const at = sectionStart + i * 40;
      sections.push({ rva: data.readUInt32LE(at + 12), size: data.readUInt32LE(at + 16), raw: data.readUInt32LE(at + 20) });
    }
    const offsetFor = (rva) => {
      const section = sections.find((entry) => rva >= entry.rva && rva < entry.rva + entry.size);
      return section ? section.raw + rva - section.rva : null;
    };
    const directory = offsetFor(data.readUInt32LE(optional + 96));
    if (directory === null) return { ...empty, found: true, note: 'No PE export directory.' };
    const nameCount = data.readUInt32LE(directory + 24);
    if (nameCount > 100_000) return { ...empty, found: true, note: 'Unexpected export count.' };
    const names = offsetFor(data.readUInt32LE(directory + 32));
    const ordinals = offsetFor(data.readUInt32LE(directory + 36));
    const functions = offsetFor(data.readUInt32LE(directory + 28));
    if ([names, ordinals, functions].some((value) => value === null)) return { ...empty, found: true };
    const exports = [];
    for (let i = 0; i < nameCount; i++) {
      const nameOffset = offsetFor(data.readUInt32LE(names + i * 4));
      if (nameOffset === null) continue;
      const end = data.indexOf(0, nameOffset);
      if (end < nameOffset || end - nameOffset > 512) continue;
      const name = data.toString('ascii', nameOffset, end);
      if (!CHAO_EXPORTS.includes(name)) continue;
      const ordinal = data.readUInt16LE(ordinals + i * 2);
      const rva = data.readUInt32LE(functions + ordinal * 4);
      exports.push({ name, rva, fileOffset: offsetFor(rva) });
    }
    const root = exports.find((entry) => entry.name === 'AL_RootObject');
    const specialModels = root?.fileOffset !== null && root?.fileOffset !== undefined
      ? SPECIAL_MODELS.map((entry) => {
        const pointer = data.readUInt32LE(root.fileOffset + entry.index * 4);
        const fileOffset = offsetFor(pointer - imageBase);
        return { ...entry, pointerPresent: pointer !== 0 && fileOffset !== null, fileOffset };
      }) : [];
    return { source, found: true, format: 'PE32', exports, specialModels,
      note: 'Named export tables and model pointers inspected as bytes. DLL code was not loaded or executed; models are not converted yet.' };
  } catch {
    return empty;
  }
}

/**
 * Read-only discovery of a local SA2 PC installation and browser-ready imports.
 * Game files and saves are never changed or copied by this function.
 */
export async function detectGameFiles(gamePath = process.env.SA2_GAME_PATH) {
  const selectedPath = await resolveInstall(gamePath);
  const executable = selectedPath ? path.join(selectedPath, 'sonic2app.exe') : null;
  const resourcePath = selectedPath ? path.join(selectedPath, 'resource', 'gd_PC') : null;
  const found = Boolean(executable && await exists(executable) && await exists(resourcePath));
  const importDirectory = path.resolve(process.env.CHAO_IMPORT_PATH || '.local/imports');
  const imports = {
    directory: importDirectory,
    chao: await exists(path.join(importDirectory, 'chao.glb')),
    garden: await exists(path.join(importDirectory, 'garden.glb')),
  };
  const evidence = [];
  if (found) {
    for (const name of BATTLE_CONTENT) {
      if (await exists(path.join(resourcePath, name))) evidence.push(`resource/gd_PC/${name}`);
    }
  }
  const result = found ? await inventoryResources(resourcePath) : {
    inventory: { chao: 0, levels: 0, textures: 0, audio: 0, animations: 0, other: 0, total: 0, totalBytes: 0 },
    assets: [], assetsTruncated: false,
  };
  return {
    found,
    gamePath: selectedPath,
    baseGame: { status: found ? 'present' : 'missing', steamAppId: '213610', executable: found ? executable : null },
    battle: {
      status: evidence.length ? 'unverified' : 'missing',
      steamAppId: '217900',
      required: true,
      contentPresent: evidence.length === BATTLE_CONTENT.length,
      evidence,
      note: evidence.length
        ? 'Karate resource files are present. Their presence does not verify Steam BATTLE DLC ownership. Confirm BATTLE is enabled in Steam > SA2 > Properties > DLC.'
        : 'Karate resource files were not found. BATTLE DLC is required; install SA2 and enable BATTLE in Steam.',
    },
    ...result,
    imports,
    nativeData: await inspectChaoData(found ? resourcePath : null),
    note: 'Read-only local inventory. Original game resources have not been converted to browser assets.',
  };
}
