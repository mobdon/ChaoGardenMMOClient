// Prepare genuine GNU Windows import-library tools without a global install.
import { mkdir, readFile, writeFile, readdir, copyFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import * as zlib from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const local = path.join(root, '.local/tools');
const destination = path.join(local, 'mingw-binutils/mingw64/bin');
if (typeof zlib.zstdDecompressSync !== 'function') throw new Error('Local binutils setup requires Node22.15+ with built-in Zstandard support.');
await mkdir(destination, { recursive: true });
await mkdir(path.join(local, 'downloads'), { recursive: true });
const sources = [];
for (const name of ['mingw-w64-x86_64-binutils', 'mingw-w64-x86_64-gettext-runtime', 'mingw-w64-x86_64-libwinpthread', 'mingw-w64-x86_64-zlib', 'mingw-w64-x86_64-zstd', 'mingw-w64-x86_64-libiconv']) {
  const pageUrl = `https://packages.msys2.org/packages/${name}`;
  const page = await fetch(pageUrl);
  if (!page.ok) throw new Error(`Official package metadata failed (${page.status}): ${name}`);
  const html = await page.text();
  const pattern = new RegExp(`${name.replaceAll('-', '\\-')}-[^"<>\\s/]+\\.pkg\\.tar\\.zst`);
  const filename = html.match(pattern)?.[0];
  if (!filename) throw new Error(`Official package filename missing: ${name}`);
  const url = `https://repo.msys2.org/mingw/mingw64/${filename}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Official package download failed (${response.status}): ${name}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  if (!html.includes(sha256)) throw new Error(`Official package SHA-256 mismatch: ${name}`);
  const tar = path.join(local, `downloads/${name}.tar`);
  await writeFile(tar, zlib.zstdDecompressSync(bytes));
  const output = path.join(local, `msys-packages/${name}`);
  await mkdir(output, { recursive: true });
  execFileSync('tar', ['-xf', tar, '-C', output], { windowsHide: true });
  const binaries = (await readdir(path.join(output, 'mingw64/bin'))).filter((file) => /\.(exe|dll)$/i.test(file));
  for (const binary of binaries) await copyFile(path.join(output, 'mingw64/bin', binary), path.join(destination, binary));
  const pkginfo = await readFile(path.join(output, '.PKGINFO'), 'utf8');
  const dependencies = [...pkginfo.matchAll(/^depend = (.*)$/gm)].map((match) => match[1]);
  sources.push({ name, pageUrl, url, sha256, bytes: bytes.length, binaries, dependencies });
  console.log(`Verified official MSYS2 package: ${filename}`);
}
await writeFile(path.join(local, 'mingw-binutils/package-sources.json'), `${JSON.stringify(sources, null, 2)}\n`);
for (const tool of ['as.exe', 'dlltool.exe']) {
  const firstLine = execFileSync(path.join(destination, tool), ['--version'], { encoding: 'utf8', windowsHide: true }).split(/\r?\n/)[0];
  console.log(firstLine);
}
