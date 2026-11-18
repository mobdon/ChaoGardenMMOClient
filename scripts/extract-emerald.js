import { copyFile, mkdir, writeFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles();
if (!game.found) throw new Error('Installed SA2 required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const output = path.resolve('.local/extracted/emerald');
await mkdir(output, { recursive: true });
const ini = path.join(output, 'emerald.ini');
// Exact PC_SA2/STG08.ini Security Hall Chaos Emeralds mapping from SA Tools.
await writeFile(ini, 'game=SA2B\ndatafile=sonic2app.exe\n\n[Security Hall Red Emerald Texlist]\ntype=texlist\naddress=A8BC38\nfilename=emerald-red.satex\n\n[Security Hall Chaos Emeralds]\ntype=chunkmodel\naddress=A8CA7C\nfilename=chaos-emerald.sa2mdl\ntexture=OBJTEX_STG08\ntexnames=emerald-red.satex\n');
const options = { windowsHide: true, timeout: 30_000, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
await run('dotnet', [path.join(tools, 'bin/lib/split.dll'), 'binary', path.join(game.gamePath, 'sonic2app.exe'), ini, output], options);
const texture = path.resolve('.local/extracted/textures/objtex_stg08.prs');
await copyFile(path.join(game.gamePath, 'resource/gd_PC/objtex_stg08.prs'), texture);
await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', texture], options);
const model = await stat(path.join(output, 'chaos-emerald.sa2mdl'));
console.log(`Extracted original Security Hall Chaos Emerald (${model.size} bytes) and original object texture pack.`);
