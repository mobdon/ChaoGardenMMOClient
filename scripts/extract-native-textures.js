import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const install = await detectGameFiles(process.argv[2]);
if (!install.found) throw new Error('Installed SA2 required.');
const toolRoot = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const texturesRoot = path.resolve('.local/extracted/textures');
const imports = path.resolve('.local/imports');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
await mkdir(texturesRoot, { recursive: true });
await mkdir(imports, { recursive: true });
const archives = [];
for (const sourcePath of ['resource/gd_PC/event/e0207texture.prs', 'resource/gd_PC/al_stg_neut_tex.prs']) {
  const source = path.join(install.gamePath, sourcePath);
  const bytes = await readFile(source);
  const sourceSha256 = hash(bytes);
  const workingCopy = path.join(texturesRoot, path.basename(sourcePath));
  // ArchiveTool writes beside its argument. Always work on an ignored copy.
  await copyFile(source, workingCopy);
  await run('dotnet', [path.join(toolRoot, 'bin/lib/ArchiveTool.dll'), '-png', workingCopy], {
    windowsHide: true, timeout: 30_000, maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' },
  });
  if (hash(await readFile(source)) !== sourceSha256) throw new Error(`Source changed during extraction: ${sourcePath}`);
  archives.push({ sourcePath, sourceBytes: bytes.length, sourceSha256, unchanged: true });
}
const assets = [];
for (const color of ['red', 'purple', 'cyan', 'green', 'blue', 'yellow', 'white']) {
  const originalColor = color === 'cyan' ? 'lightblue' : color;
  const originalName = `chaoseme_${originalColor}.png`;
  const source = path.join(texturesRoot, 'e0207texture', originalName);
  const target = path.join(imports, `emerald-${color}.png`);
  const bytes = await readFile(source);
  await copyFile(source, target);
  assets.push({
    name: `emerald-${color}`, sourceArchive: 'resource/gd_PC/event/e0207texture.prs',
    texture: originalName, output: `.local/imports/emerald-${color}.png`,
    width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), bytes: bytes.length,
    sha256: hash(bytes), pixelChanges: false,
  });
}
const grass = await readFile(path.join(texturesRoot, 'al_stg_neut_tex/gneut03.png'));
await copyFile(path.join(texturesRoot, 'al_stg_neut_tex/gneut03.png'), path.join(imports, 'nativehubgrass.png'));
assets.push({ name: 'nativehubgrass', sourceArchive: 'resource/gd_PC/al_stg_neut_tex.prs', texture: 'gneut03.png', archiveGlobalIndex: 120003, output: '.local/imports/nativehubgrass.png', width: grass.readUInt32BE(16), height: grass.readUInt32BE(20), bytes: grass.length, sha256: hash(grass), pixelChanges: false });
const manifest = { generatedAt: new Date().toISOString(), tool: 'X-Hax SA Tools ArchiveTool 786', archives, assets, note: 'All seven original Chaos Emerald textures come from the native event archive, with their original facet pixels. Chao jewel textures and generated recolors are not used.' };
await writeFile(path.join(imports, 'native-textures.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
console.log(JSON.stringify(manifest, null, 2));
