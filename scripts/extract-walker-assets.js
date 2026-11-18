import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('An installed Sonic Adventure 2 is required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const output = path.resolve('.local/extracted/dark-walker');
const textures = path.resolve('.local/extracted/textures');
await mkdir(output, { recursive: true });
await mkdir(textures, { recursive: true });
const hash = async file => createHash('sha256').update(await readFile(file)).digest('hex');
const sources = [];
const options = {
  windowsHide: true, timeout: 60_000, maxBuffer: 8 * 1024 * 1024,
  env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' },
};
for (const [filename, utility, directory] of [
  ['dwalkmdl.prs', 'splitMDL', output],
  ['dwalkmtn.prs', 'splitMTN', output],
  ['dwalktex.prs', 'ArchiveTool', textures],
]) {
  const relativePath = `resource/gd_PC/${filename}`;
  const original = path.join(game.gamePath, relativePath);
  const before = await hash(original);
  const copy = path.join(directory, filename);
  await copyFile(original, copy);
  const args = [path.join(tools, `bin/lib/${utility}.dll`), ...(utility === 'ArchiveTool' ? ['-png'] : []), copy];
  const result = await run('dotnet', args, options);
  await writeFile(path.join(output, `${utility}.log`), result.stdout + result.stderr);
  const after = await hash(original);
  if (before !== after) throw new Error(`Installed source changed: ${relativePath}`);
  sources.push({ relativePath, sha256: before, unchanged: true });
}
const report = {
  generatedAt: new Date().toISOString(), sources,
  tools: 'Official X-Hax SA Tools release 786 splitMDL/splitMTN/ArchiveTool',
  modelMapping: 'GameConfig/PC_SA2/MDL/DWALKMDL.ini',
  motionMapping: 'GameConfig/PC_SA2/MDL/DWALKMTN.ini',
  mainModelId: 465, cockpitWindowId: 461,
  selectedAnimations: { 89: 'Idle', 199: 'Walk' },
  outputFiles: await readdir(output, { recursive: true }),
};
await writeFile(path.resolve('.local/walker-extraction.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
