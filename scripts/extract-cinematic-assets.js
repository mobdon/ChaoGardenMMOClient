import { copyFile, mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed SA2 required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const eventSource = path.resolve('.local/extracted/cutscene-source');
const eventOutput = path.resolve('.local/extracted/cutscene-e0207');
const props = path.resolve('.local/extracted/native-props');
const textures = path.resolve('.local/extracted/textures');
for (const folder of [eventSource, eventOutput, props, textures]) await mkdir(folder, { recursive: true });
const hash = async (file) => createHash('sha256').update(await readFile(file)).digest('hex');
const inputs = ['resource/gd_PC/event/e0207.prs', 'resource/gd_PC/event/e0207motion.bin', 'resource/gd_PC/event/e0207texlist.prs', 'resource/gd_PC/event/e0207texture.prs', 'sonic2app.exe', 'resource/gd_PC/al_neut_obj_tex.prs'];
const sources = [];
for (const relativePath of inputs) sources.push({ relativePath, sha256: await hash(path.join(game.gamePath, relativePath)) });
for (const relativePath of inputs.slice(0, 4)) await copyFile(path.join(game.gamePath, relativePath), path.join(eventSource, path.basename(relativePath)));
const options = { windowsHide: true, timeout: 60_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
const split = await run('dotnet', [path.join(tools, 'bin/lib/splitEvent.dll'), path.join(eventSource, 'e0207.prs'), eventOutput], options);
await writeFile(path.resolve('.local/event-split.log'), split.stdout + split.stderr);
const ini = path.join(props, 'neutral-tree.ini');
await writeFile(ini, 'game=SA2B\ndatafile=sonic2app.exe\n\n[Neutral Garden Tree]\ntype=chunkmodel\naddress=F27C6C\nfilename=tree-neutral.sa2mdl\ntexture=AL_NEUT_OBJ_TEX\nchunktype=ChaoChunk\n');
await run('dotnet', [path.join(tools, 'bin/lib/split.dll'), 'binary', path.join(game.gamePath, 'sonic2app.exe'), ini, props], options);
for (const [original, localName] of [['resource/gd_PC/event/e0207texture.prs', 'e0207texture.prs'], ['resource/gd_PC/al_neut_obj_tex.prs', 'al_neut_obj_tex.prs']]) {
  const copy = path.join(textures, localName);
  await copyFile(path.join(game.gamePath, original), copy);
  await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', copy], options);
}
for (const source of sources) {
  source.unchanged = await hash(path.join(game.gamePath, source.relativePath)) === source.sha256;
  if (!source.unchanged) throw new Error(`Installed source changed: ${source.relativePath}`);
}
const tree = await stat(path.join(props, 'tree-neutral.sa2mdl'));
const report = {
  generatedAt: new Date().toISOString(), sources,
  tools: 'Official X-Hax SA Tools release 786 splitEvent / split / ArchiveTool',
  cinematic: { source: 'resource/gd_PC/event/e0207.prs', texlist: '.local/extracted/cutscene-e0207/e0207/InternalTexlist.satex', individualModels: 'Scene 2/Entity 2 through Entity 8 Model.sa2mdl', textureIds: { 2: 'blue', 3: 'green', 4: 'cyan', 5: 'purple', 6: 'red', 7: 'white', 8: 'yellow' }, combinedSevenEmeraldModel: 'Scene 3/Entity 3 Model.sa2mdl' },
  tree: { source: 'sonic2app.exe', mapping: 'PC_SA2/CHAOSTGNEUT.ini, address F27C6C, ChaoChunk', nativeModel: '.local/extracted/native-props/tree-neutral.sa2mdl', bytes: tree.size, textures: 'resource/gd_PC/al_neut_obj_tex.prs' },
};
await writeFile(path.resolve('.local/cinematic-extraction.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
