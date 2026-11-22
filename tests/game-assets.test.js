import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { inflateSync } from 'node:zlib';
import { convertTexture, TEXTURES } from '../src/game-assets.js';

// Synthetic test input (never game data): a literal-only PRS stream of a one-texture GVM
// holding a 4x4 ARGB8888 GVR named `name`.
function syntheticArchive(name, pixels) {
  const gvm = Buffer.concat([
    Buffer.from('GVMH'), Buffer.from([34, 0, 0, 0, 0, 1, 0, 1, 0, 0]),
    Buffer.concat([Buffer.from(name), Buffer.alloc(28 - name.length)]),
    Buffer.from('GVRT'), Buffer.from([pixels.length + 8, 0, 0, 0, 0, 0, 0, 6, 0, 4, 0, 4]), pixels,
  ]);
  const out = [];
  let flagAt = 0, bit = 8;
  for (const [flag, byte] of [...[...gvm].map(b => [1, b]), [0], [1]]) {
    if (bit === 8) { flagAt = out.length; bit = 0; out.push(0); }
    out[flagAt] |= flag << bit++;
    if (byte !== undefined) out.push(byte);
  }
  return Uint8Array.from([...out, 0, 0]);
}

test('the wasm converter turns a named archive texture into a PNG with its pixels', async () => {
  const { instance } = await WebAssembly.instantiate(await readFile(new URL('../public/game-texture.wasm', import.meta.url)), {});
  const pixels = Buffer.alloc(64); pixels[0] = 17; pixels[1] = 22; pixels[32] = 33; pixels[33] = 44;
  const prs = syntheticArchive('level00', pixels);
  const png = Buffer.from(convertTexture(instance.exports, prs, 'level00'));
  assert.equal(png.toString('latin1', 1, 4), 'PNG');
  assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [4, 4]);
  // First scanline: filter byte, then the first pixel as RGBA.
  let at = 8; const idat = [];
  while (at < png.length) { const size = png.readUInt32BE(at); if (png.toString('latin1', at + 4, at + 8) === 'IDAT') idat.push(png.subarray(at + 8, at + 8 + size)); at += size + 12; }
  const rows = inflateSync(Buffer.concat(idat));
  assert.equal(rows[0], 0, 'unfiltered first row');
  assert.deepEqual([...rows.subarray(1, 5)], [22, 33, 44, 17]);
  assert.equal(convertTexture(instance.exports, prs, 'level01'), null);
  assert.equal(convertTexture(instance.exports, new Uint8Array(16), 'level00'), null);
});

test('every menu picture names a game archive and texture; all 25 item panels are listed', () => {
  for (let id = 0; id < 25; id++) assert.ok(TEXTURES[`item-panel-${id}.png`], `item-panel-${id}`);
  for (const [alias, [archives, name]] of Object.entries(TEXTURES)) {
    assert.ok(archives.length > 0 && archives.every(a => /^al_[a-z_]+$/.test(a)), alias);
    assert.match(name, /^[a-z0-9_]+$/, alias);
  }
});
