import { copyFile, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('An installed Sonic Adventure 2 is required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const output = path.resolve('.local/extracted/transporter');
await mkdir(output, { recursive: true });
const hash = async file => createHash('sha256').update(await readFile(file)).digest('hex');
const mappingFile = path.join(tools, 'GameConfig/PC_SA2/ChaoCommon.ini');
const mapping = await readFile(mappingFile, 'utf8');
const parts = [
  ['Chao Transporter Button', 'button'],
  ['Chao Transporter GBA Version Shutter', 'shutter'],
  ['Chao Transporter GBA Version Casing', 'casing'],
  ['Chao Transporter GBA Version Base', 'base'],
];
let ini = 'game=SA2B\ndatafile=sonic2app.exe\n\n';
const models = [];
for (const [name, id] of parts) {
  const section = mapping.split(`[${name}]`)[1]?.split(/\r?\n\[/)[0];
  const address = section?.match(/^address=([A-Fa-f0-9]+)\s*$/m)?.[1];
  if (!address || !section.includes('type=chunkmodel') || !section.includes('texture=AL_OBJECT')) throw new Error(`Transporter mapping changed: ${name}`);
  const filename = `transporter-${id}.sa2mdl`;
  ini += `[${name}]\ntype=chunkmodel\naddress=${address}\nfilename=${filename}\ntexture=AL_OBJECT\n\n`;
  models.push({ id, name, address, filename });
}
const sources = await Promise.all(['sonic2app.exe', 'resource/gd_PC/al_object.prs'].map(async relativePath => ({ relativePath, sha256: await hash(path.join(game.gamePath, relativePath)) })));
const localIni = path.join(output, 'transporter.ini');
await writeFile(localIni, ini);
const options = { windowsHide: true, timeout: 60000, maxBuffer: 4 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
const split = await run('dotnet', [path.join(tools, 'bin/lib/split.dll'), 'binary', path.join(game.gamePath, 'sonic2app.exe'), localIni, output], options);
await writeFile(path.join(output, 'split.log'), split.stdout + split.stderr);
const textureCopy = path.join(output, 'al_object.prs');
await copyFile(path.join(game.gamePath, 'resource/gd_PC/al_object.prs'), textureCopy);
const textures = await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', textureCopy], options);
await writeFile(path.join(output, 'textures.log'), textures.stdout + textures.stderr);
for (const model of models) {
  const file = path.join(output, model.filename);
  model.bytes = (await stat(file)).size;
  model.sha256 = await hash(file);
}
for (const source of sources) {
  source.unchanged = source.sha256 === await hash(path.join(game.gamePath, source.relativePath));
  if (!source.unchanged) throw new Error(`Installed input changed: ${source.relativePath}`);
}
const report = { schemaVersion: 1, createdUtc: new Date().toISOString(), mapping: 'Official X-Hax SA Tools786 PC_SA2/ChaoCommon.ini', sources, models, textureArchive: 'al_object.prs', status: 'Native mapped components and texture archive extracted. Assembly, GLB conversion, collision, world placement and Tiny handoff interaction remain separate work.' };
await writeFile(path.join(output, 'manifest.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ parts: models.map(({ id, address, bytes }) => ({ id, address, bytes })), sourcesUnchanged: sources.every(source => source.unchanged) }, null, 2));
