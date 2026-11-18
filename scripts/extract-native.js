import { copyFile, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { detectGameFiles } from './game-files.js';

const runFile = promisify(execFile);
const toolsPath = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const splitDll = path.join(toolsPath, 'bin', 'lib', 'split.dll');
const config = path.join(toolsPath, 'GameConfig', 'PC_SA2', 'Chao_DLL.ini');
const output = path.resolve('.local/extracted/pc-chao');
const install = await detectGameFiles(process.argv[2]);
if (!install.found) throw new Error('SA2 installation not found. Pass its folder or set SA2_GAME_PATH.');
await stat(splitDll).catch(() => { throw new Error('SA Tools 786 is not prepared. See docs/ASSET-PIPELINE.md; set SA_TOOLS_DIR if installed elsewhere.'); });
await stat(config);
const source = path.join(install.gamePath, 'resource', 'gd_PC', 'DLL', 'Win32', 'Data_DLL.dll');
const sha256 = async (file) => createHash('sha256').update(await readFile(file)).digest('hex');
const before = await sha256(source);
await mkdir(output, { recursive: true });
const { stdout, stderr } = await runFile('dotnet', [splitDll, 'binary', source, config, output], {
  windowsHide: true,
  timeout: 120_000,
  maxBuffer: 4 * 1024 * 1024,
  // The tool targets Desktop 8; this machine has Desktop 9/10. Limit policy
  // to this subprocess, rather than changing machine-wide runtime settings.
  env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' },
});
const after = await sha256(source);
if (before !== after) throw new Error('Source DLL hash changed during extraction. Inspect before continuing.');
const models = [];
for (const filename of ['al_ncz', 'al_nchaos', 'al_hchaos', 'al_dchaos', 'al_tails', 'al_knuckles', 'al_amy']) {
  const file = path.join(output, 'Chao', 'models', 'AL_RootObject', `${filename}.sa2mdl`);
  const info = await stat(file);
  models.push({ filename: `${filename}.sa2mdl`, bytes: info.size, path: file });
}
const textures = [];
const textureDirectory = path.resolve('.local/extracted/textures');
await mkdir(textureDirectory, { recursive: true });
for (const name of ['al_body', 'al_eye', 'al_stg_neut_tex', 'al_neut_obj_tex', 'al_stg_hero_tex', 'al_stg_dark_tex', 'al_icon']) {
  // ArchiveTool writes alongside its input. Give it a local working copy so
  // all converted output stays outside the installed game directory.
  const archive = path.join(textureDirectory, `${name}.prs`);
  await copyFile(path.join(install.gamePath, 'resource', 'gd_PC', `${name}.prs`), archive);
  await runFile('dotnet', [path.join(toolsPath, 'bin', 'lib', 'ArchiveTool.dll'), '-png', archive], {
    windowsHide: true, timeout: 30_000, maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' },
  });
  const folder = path.join(textureDirectory, name);
  const files = await readdir(folder);
  textures.push({ archive: `${name}.prs`, pngCount: files.filter((file) => file.endsWith('.png')).length, path: folder });
}
const report = {
  tool: 'X-Hax SA Tools', version: '786', sourceHashSha256: after,
  sourceUnchanged: true, output, models, textures,
  note: 'Native model/animation intermediates and selected original PNG textures extracted. This report covers extraction; scripts/convert-assets.py performs browser glTF conversion.',
};
await writeFile(path.resolve('.local/extraction-report.json'), `${JSON.stringify(report, null, 2)}\n`, 'utf8');
await writeFile(path.resolve('.local/extraction.log'), `${stdout}${stderr}`, 'utf8');
console.log(`Extracted ${models.length} checked Chao variants and the mapped garden/animation data.`);
console.log('Original Data_DLL.dll SHA-256 is unchanged.');
console.log(`Native intermediate files: ${output}`);
console.log(`PNG textures: ${textures.reduce((sum, pack) => sum + pack.pngCount, 0)} from ${textures.length} selected archives.`);
console.log(report.note);
