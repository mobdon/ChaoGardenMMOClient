// node --test asset-tools/vgmstream-wasm/web/
// The Steam check reads the owner's install (SA2_INSTALL, default Steam path) and skips when absent.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { Vgmstream } from './vgmstream.js';
import { Ahx } from './ahx.js';

const vgm = await Vgmstream.load(readFileSync(new URL('./vgmstream.wasm', import.meta.url)));
const install = process.env.SA2_INSTALL ?? 'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2';
const boss = `${install}/resource/gd_PC/ADX/BOSS_02A.ADX`;
const kart = `${install}/resource/gd_PC/MLT/se_kart/0.csb`;

test('rejects bytes that are no known format', () => {
  assert.equal(vgm.open(new Uint8Array(64).fill(7), 'junk.adx'), null);
  assert.equal(vgm.decode(new Uint8Array(0), 'empty.adx'), null);
});

test('decodes a Steam ADX with its loop points', { skip: !existsSync(boss) && 'no SA2 install' }, () => {
  const s = vgm.decode(readFileSync(boss), 'BOSS_02A.ADX');
  assert.equal(s.meta, 'CRI ADX header (type 03)');
  assert.equal(s.rate, 32000);
  assert.equal(s.channels, 2);
  assert.equal(s.samples[0].length, 2411787);
  assert.deepEqual(s.loop, { start: 61280, end: 2411320 });
});

test('decodes a Steam CSB sound-effect subsong', { skip: !existsSync(kart) && 'no SA2 install' }, () => {
  const s = vgm.decode(readFileSync(kart), '0.csb', 3);
  assert.equal(s.subsongs, 39);
  assert.equal(s.rate, 22050);
  assert.equal(s.samples[0].length, 22280);
});
const ahx = await Ahx.load(readFileSync(new URL('./ahx.wasm', import.meta.url)));

test('ahx rejects bytes that are not AHX', () => {
  assert.equal(ahx.decode(new Uint8Array(32)), null);
});

for (const name of ['event_adx.afs', 'event_adx_e.afs']) {
  const file = `${install}/resource/gd_PC/${name}`;
  test(`decodes every AHX voice in ${name}`, { skip: !existsSync(file) && 'no SA2 install' }, () => {
    const a = readFileSync(file);
    const v = new DataView(a.buffer, a.byteOffset, a.byteLength);
    const count = v.getUint32(4, true);
    let decoded = 0, empty = 0;
    const failed = [];
    for (let i = 0; i < count; i++) {
      const off = v.getUint32(8 + i * 8, true), len = v.getUint32(12 + i * 8, true);
      if (len === 0) { empty++; continue; }
      const d = ahx.decode(a.subarray(off, off + len));
      if (d && d.samples[0].length > 0) decoded++; else failed.push(i);
    }
    console.log(`${name}: ${count} entries, ${decoded} decoded, ${empty} empty, ${failed.length} failed ${failed.slice(0, 10)}`);
    assert.equal(count, 2727);
    assert.deepEqual(failed, []);
  });
}
