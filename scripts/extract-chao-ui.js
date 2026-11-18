import { copyFile, mkdir, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const install = await detectGameFiles(process.argv[2]);
if (!install.found) throw new Error('An installed Sonic Adventure 2 is required. Pass its folder or set SA2_GAME_PATH.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const output = path.resolve('.local/extracted/chao-ui');
const imports = path.resolve('.local/imports');
await mkdir(output, { recursive: true });
await mkdir(imports, { recursive: true });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const archives = ['al_tex_common', 'al_tex_chao', 'al_tex_ent_common_en', 'al_tex_odekake_menu_en', 'al_tex_cave_menu_en', 'al_common_tex'];
const report = {
  generatedAt: new Date().toISOString(),
  tool: 'X-Hax SA Tools release 786 ArchiveTool -png; original installed archives read only',
  sources: [], selected: [],
};
for (const name of archives) {
  const relativePath = `resource/gd_PC/${name}.prs`;
  const source = path.join(install.gamePath, relativePath);
  const bytes = await readFile(source); const before = hash(bytes);
  const copy = path.join(output, `${name}.prs`);
  await copyFile(source, copy);
  const result = await run('dotnet', [path.join(tools, 'bin/lib/ArchiveTool.dll'), '-png', copy], {
    windowsHide: true, timeout: 60_000, maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, DOTNET_ROLL_FORWARD: 'Major' },
  });
  await writeFile(path.join(output, `${name}.log`), result.stdout + result.stderr);
  const after = hash(await readFile(source));
  if (after !== before) throw new Error(`Installed source changed during extraction: ${relativePath}`);
  const directory = path.join(output, name);
  const index = await readFile(path.join(directory, 'index.txt'), 'utf8');
  const sprites = [];
  for (const [indexLine, row] of index.split(/\r?\n/).entries()) {
    const columns = row.split(',');
    const filename = columns[1]?.trim();
    if (!filename) continue;
    if (!filename.endsWith('.png')) continue;
    const file = path.join(directory, filename); const png = await readFile(file);
    if (png.toString('ascii', 1, 4) !== 'PNG') throw new Error(`Not a PNG: ${file}`);
    sprites.push({ archiveIndex: indexLine, globalTextureId: Number(columns[0]), row, filename, width: png.readUInt32BE(16), height: png.readUInt32BE(20), bytes: (await stat(file)).size, sha256: hash(png) });
  }
  // Some ArchiveTool versions place the index in a different format. Never
  // omit output images silently; retain filenames even if ordering is unknown.
  const indexed = new Set(sprites.map(sprite => sprite.filename));
  for (const filename of (await readdir(directory)).filter(filename => filename.endsWith('.png') && !indexed.has(filename))) {
    const png = await readFile(path.join(directory, filename));
    sprites.push({ archiveIndex: null, globalTextureId: null, filename, width: png.readUInt32BE(16), height: png.readUInt32BE(20), bytes: png.length, sha256: hash(png) });
  }
  report.sources.push({ relativePath, sha256: before, unchanged: true, outputDirectory: path.relative(process.cwd(), directory).replaceAll('\\', '/'), sprites });
}

// These whole PNG atlases were visually inspected. Files are exact ArchiveTool
// outputs; CSS clips the original pixels rather than repainting a substitute.
const selectedSprites = [
  { archive: 'al_tex_common', filename: 'level00.png', alias: 'chao-stat-panel.png', role: 'Original Chao stat/level HUD atlas',
    evidence: 'Visible Swim/Fly/Run/Power/Stamina labels, Lv. marker, digits, LEVEL UP text and blue window/bubble graphics.' },
  { archive: 'al_tex_common', filename: 'ad_22ascii.png', alias: 'chao-stat-font.png', role: 'Original common 22px bitmap glyph atlas',
    evidence: 'Native ASCII and extended glyphs. A common font asset, not a complete stat window.' },
  { archive: 'al_tex_ent_common_en', filename: 'add_parameter_race_en.png', alias: 'chao-race-parameter.png', role: 'Original English race-entry parameter atlas',
    evidence: 'Visible Swim/Fly/Run/Power/Stamina, type/alignment labels, digits, Lv. and green meter segments. Race-entry context, not the garden stat panel.' },
];
for (const { archive, filename, alias, role, evidence } of selectedSprites) {
  const source = report.sources.find(source => source.relativePath.endsWith(`/${archive}.prs`));
  const sprite = source?.sprites.find(sprite => sprite.filename === filename);
  if (!sprite) throw new Error(`Selected native sprite missing: ${archive}/${filename}`);
  await copyFile(path.join(output, archive, filename), path.join(imports, alias));
  report.selected.push({ archive: source.relativePath, archiveSha256: source.sha256, ...sprite, alias, role, evidence });
}
report.statPanelRegions = {
  source: 'chao-stat-panel.png', units: 'unscaled source pixels, origin top-left',
  frame: { x: 0, y: 0, width: 165, height: 76 },
  labels: {
    swim: { x: 0, y: 78, width: 46, height: 16 }, fly: { x: 49, y: 78, width: 29, height: 17 },
    run: { x: 81, y: 78, width: 36, height: 16 }, power: { x: 121, y: 78, width: 54, height: 16 },
    stamina: { x: 176, y: 78, width: 71, height: 16 },
  },
  levelMarker: { x: 180, y: 58, width: 27, height: 14 },
  bubbleDecoration: { x: 0, y: 96, width: 86, height: 26 },
  digitCells: { firstX: 166, firstY: 24, width: 16, height: 16, columns: 5, values: '0123456789' },
};
report.fontGlyphs = {
  source: 'chao-stat-font.png', width: 22, height: 22, columns: 23, firstAscii: 33,
  formula: 'index=codePoint-33; x=(index%23)*22; y=floor(index/23)*22',
  grades: Object.fromEntries([...'SABCDE'].map(letter => {
    const index = letter.codePointAt(0) - 33; return [letter, { x: index % 23 * 22, y: Math.floor(index / 23) * 22, width: 22, height: 22 }];
  })),
};
await writeFile(path.join(imports, 'chao-ui-provenance.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ archives: report.sources.map(source => ({ archive: source.relativePath, sprites: source.sprites.length })), selected: report.selected }, null, 2));
