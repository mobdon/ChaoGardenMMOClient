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
const output = path.resolve('.local/extracted/upgrades');
const textures = path.resolve('.local/extracted/textures');
await mkdir(output, { recursive: true });
await mkdir(textures, { recursive: true });
const hash = async file => createHash('sha256').update(await readFile(file)).digest('hex');
const options = { windowsHide: true, timeout: 60_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' } };
const sources = [];
for (const [name, utility, directory] of [
  ['teriosmdl', 'splitMDL', output], ['knuckmdl', 'splitMDL', output], ['sonicmdl', 'splitMDL', output],
  ['teriostex', 'ArchiveTool', textures], ['sonictex', 'ArchiveTool', textures], ['knucktex', 'ArchiveTool', textures],
]) {
  const relativePath = `resource/gd_PC/${name}.prs`;
  const original = path.join(game.gamePath, relativePath);
  const sha256 = await hash(original);
  const copy = path.join(directory, `${name}.prs`);
  // These tools write adjacent to their input: only pass ignored copies.
  await copyFile(original, copy);
  const result = await run('dotnet', [path.join(tools, `bin/lib/${utility}.dll`), ...(utility === 'ArchiveTool' ? ['-png'] : []), copy], options);
  await writeFile(path.join(output, `${name}-${utility}.log`), result.stdout + result.stderr);
  if (sha256 !== await hash(original)) throw new Error(`Installed source changed: ${relativePath}`);
  sources.push({ relativePath, sha256, unchanged: true });
}
const assets = [
  { name: 'shadow-air-shoes-left', itemId: 'shadow-air-shoes', side: 'left', slot: 'footwear', archive: 'teriosmdl', modelId: 79, textures: 'teriostex' },
  { name: 'shadow-air-shoes-right', itemId: 'shadow-air-shoes', side: 'right', slot: 'footwear', archive: 'teriosmdl', modelId: 78, textures: 'teriostex' },
  { name: 'sonic-light-shoes-left', itemId: 'sonic-light-shoes', side: 'left', slot: 'footwear', archive: 'sonicmdl', modelId: 14, textures: 'sonictex' },
  { name: 'sonic-light-shoes-right', itemId: 'sonic-light-shoes', side: 'right', slot: 'footwear', archive: 'sonicmdl', modelId: 13, textures: 'sonictex' },
  { name: 'knuckles-hammer-gloves-left', itemId: 'knuckles-hammer-gloves', side: 'left', slot: 'hands', archive: 'knuckmdl', modelId: 165, textures: 'knucktex' },
  { name: 'knuckles-hammer-gloves-right', itemId: 'knuckles-hammer-gloves', side: 'right', slot: 'hands', archive: 'knuckmdl', modelId: 164, textures: 'knucktex' },
];
for (const asset of assets) {
  asset.nativeModel = `.local/extracted/upgrades/${asset.archive}/${asset.modelId}.sa2mdl`;
  asset.nativeBytes = (await stat(path.resolve(asset.nativeModel))).size;
  asset.nativeSha256 = await hash(path.resolve(asset.nativeModel));
  asset.mapping = `X-Hax SA Tools786 PC_SA2/MDL/${asset.archive.toUpperCase()}.ini, model${asset.modelId}`;
  if (asset.itemId === 'sonic-light-shoes') {
    // Light Shoes13/14 are overlays like Air Shoes78/79; Sonic's own boot from sonicmdl/0 is still needed.
    // shortcut: heel/toe node numbers in sonicmdl/0 are unverified, so these stay out of the manifest until read from the model.
    asset.unverifiedBaseBoot = true;
    continue;
  }
  if (asset.slot === 'footwear') {
    asset.baseBoot = { nativeModel: '.local/extracted/upgrades/teriosmdl/65.sa2mdl', modelId: 65,
      footNode: asset.side === 'right' ? 13 : 23,
      evidence: 'X-Hax Character Model Structures: Shadow Right Foot Heel13/Toe14; Left Foot Heel23/Toe24. Air Shoes78/79 are separate upgrade overlays.' };
    asset.baseBoot.nativeSha256 = await hash(path.resolve(asset.baseBoot.nativeModel));
  }
}
const report = { version: 1, generatedAt: new Date().toISOString(), tools: 'Official X-Hax SA Tools release786 splitMDL/ArchiveTool', sources, assets: assets.filter(asset => !asset.unverifiedBaseBoot), note: 'Separate wearable props only. No original body or rig is altered. Conversion recenters geometry without changing native mesh proportions or original texture pixels.' };
await writeFile(path.join(output, 'manifest.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ sources: sources.length, assets: assets.map(({ name, modelId, nativeBytes }) => ({ name, modelId, nativeBytes })), sourceHashesUnchanged: sources.every(source => source.unchanged) }, null, 2));
