import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync, openSync, readSync, closeSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { detectGameFiles } from './game-files.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const work = join(root, '.local', 'audio-work');
const output = join(root, '.local', 'imports');
mkdirSync(work, { recursive: true }); mkdirSync(output, { recursive: true });

function findFfmpeg() {
  if (process.env.FFMPEG_PATH) return process.env.FFMPEG_PATH;
  const directory = join(root, '.local', 'tools', 'ffmpeg');
  if (!existsSync(directory)) throw new Error('Set FFMPEG_PATH or install an official FFmpeg Windows build under .local/tools/ffmpeg.');
  for (const item of readdirSync(directory, { withFileTypes: true })) {
    const candidate = item.isDirectory() ? join(directory, item.name, 'bin', 'ffmpeg.exe') : join(directory, item.name);
    if (candidate.endsWith('ffmpeg.exe') && existsSync(candidate)) return candidate;
  }
  throw new Error('FFmpeg executable not found.');
}

const ffmpeg = findFfmpeg();
const decoder = process.env.VGMSTREAM_PATH || join(root, '.local', 'tools', 'vgmstream', 'vgmstream-cli.exe');
if (!existsSync(decoder)) throw new Error('Install the maintainer vgmstream CLI under .local/tools/vgmstream or set VGMSTREAM_PATH.');
const detected = await detectGameFiles(process.argv[2] || process.env.SA2_GAME_PATH);
if (!detected.found) throw new Error('The local Steam Sonic Adventure 2 installation was not found.');
const resource = join(detected.gamePath, 'resource', 'gd_PC');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

function readAfsEntry(filename, index) {
  const descriptor = openSync(filename, 'r');
  try {
    const header = Buffer.alloc(8); readSync(descriptor, header, 0, 8, 0);
    if (header.toString('ascii', 0, 4) !== 'AFS\0' || index >= header.readUInt32LE(4)) throw new Error('Invalid AFS archive or entry index.');
    const entry = Buffer.alloc(8); readSync(descriptor, entry, 0, 8, 8 + index * 8);
    const offset = entry.readUInt32LE(0); const length = entry.readUInt32LE(4);
    if (!length || length > 32 * 1024 * 1024) throw new Error('Unexpected voice entry size.');
    const bytes = Buffer.alloc(length);
    if (readSync(descriptor, bytes, 0, length, offset) !== length) throw new Error('Incomplete AFS voice entry.');
    return { bytes, offset, length };
  } finally { closeSync(descriptor); }
}

const jobs = [
  ['music-hub', 'ADX/chao_hall.adx'],
  ['music-garden', 'ADX/chao_g_bgm_n.adx'],
  ['music-hero', 'ADX/chao_g_bgm_h.adx'],
  ['music-dark', 'ADX/chao_g_bgm_d.adx'],
  // The breeding jingle (fn_60_40130: Jingle_Play("chao_g_dance.adx")).
  ['jingle-chao-g-dance', 'ADX/chao_g_dance.adx'],
].map(([name, source]) => ({ name, source, input: join(resource, source), sourceSha256: hash(readFileSync(join(resource, source))) }));

// These exact English AFS entries are identified in X-Hax's original dialogue map.
// Character-bank subsong guesses are deliberately unnecessary.
for (const [name, index, character, line] of [
  ['voice-knuckles', 2654, 'Knuckles', "I'm Knuckles."],
  ['voice-tails', 2662, 'Tails', "I'm Tails."],
  ['voice-amy', 2673, 'Amy Rose', 'My name is Amy Rose.'],
]) {
  const source = 'event_adx_e.afs';
  const entry = readAfsEntry(join(resource, source), index);
  const input = join(work, `${name}.ahx`); writeFileSync(input, entry.bytes);
  jobs.push({ name, source, input, afsIndex: index, offset: entry.offset, length: entry.length,
    character, line, sourceSha256: hash(entry.bytes),
    mapping: 'https://github.com/X-Hax/SA2BModdingGuide/wiki/AFS-Voice-List' });
}

const records = [];
for (const job of jobs) {
  const metadata = execFileSync(decoder, ['-m', job.input], { encoding: 'utf8', windowsHide: true });
  const wave = join(work, `${job.name}.wav`);
  execFileSync(decoder, ['-i', '-o', wave, job.input], { stdio: 'pipe', windowsHide: true });
  const destination = join(output, `${job.name}.mp3`);
  execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y', '-i', wave,
    '-vn', '-c:a', 'libmp3lame', '-q:a', '3', '-map_metadata', '-1', destination], { stdio: 'pipe', windowsHide: true });
  const duration = metadata.match(/stream total samples: \d+ \([^)]*?([\d:.]+) seconds\)/)?.[1] || null;
  const { input, ...record } = job;
  records.push({ ...record, output: `${job.name}.mp3`, outputSha256: hash(readFileSync(destination)), duration, decoderMetadata: metadata.trim() });
  console.log(`${job.name}.mp3 — ${duration || 'see metadata'} seconds — ${job.source}${job.afsIndex === undefined ? '' : ` [${job.afsIndex}]`}`);
}
writeFileSync(join(output, 'audio-provenance.json'), JSON.stringify({
  generatedAt: new Date().toISOString(), originalInstallationReadOnly: true,
  decoding: 'vgmstream CLI; -i ignores loop repetition and decodes each original stream once.',
  encoding: 'FFmpeg libmp3lame quality 3; no synthetic voices or replacement music.',
  tools: { ffmpeg: 'https://ffmpeg.org/download.html', windowsBuild: 'https://www.gyan.dev/ffmpeg/builds/', vgmstream: 'https://github.com/vgmstream/vgmstream/releases' },
  records,
}, null, 2));
