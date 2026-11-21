// The player's GameCube disc image (.iso/.gcm, or Dolphin's .rvz), read on a
// game thread: its files by name, each as { size, read(start, end) } bytes.
// Used by worker.js (importScripts); reads are synchronous (FileReaderSync).
// RVZ layout and its filler ("junk") generator follow Dolphin's published
// format notes (docs/WiaAndRvz.md); zstd comes from fzstd (vendor/fzstd.js).
'use strict';

function discReader(file) {
  const rd = new FileReaderSync();
  const raw = (a, b) => new Uint8Array(rd.readAsArrayBuffer(file.slice(a, b)));
  const head = raw(0, 4);
  const magic = String.fromCharCode(...head);
  return magic === 'RVZ\x01' ? rvzReader(file, raw) : { size: file.size, read: raw };
}

// ---- RVZ ------------------------------------------------------------------
const BE = (u8) => new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
const u64 = (d, o) => d.getUint32(o) * 2 ** 32 + d.getUint32(o + 4);

function rvzReader(file, raw) {
  const h1 = BE(raw(0, 0x48));
  const isoSize = u64(h1, 0x24);
  const h2u8 = raw(0x48, 0x48 + h1.getUint32(0x0c)), h2 = BE(h2u8);
  const [discType, compression] = [h2.getUint32(0), h2.getUint32(4)];
  if (discType !== 1) throw new Error('only GameCube disc images are supported');
  if (compression !== 5 && compression !== 0) throw new Error(`RVZ compression ${compression} isn't supported (zstd only)`);
  const unz = (u8) => (compression === 5 ? fzstd.decompress(u8) : u8);
  const chunk = h2.getUint32(0x0c);
  const discHeader = h2u8.slice(0x10, 0x90);
  const rawN = h2.getUint32(0xb4), rawOff = u64(h2, 0xb8), rawSz = h2.getUint32(0xc0);
  const grpN = h2.getUint32(0xc4), grpOff = u64(h2, 0xc8), grpSz = h2.getUint32(0xd0);
  const rawT = BE(unz(raw(rawOff, rawOff + rawSz)));
  const grpT = BE(unz(raw(grpOff, grpOff + grpSz)));
  // Raw data areas, each starting at a 0x8000 boundary on the disc.
  const areas = [];
  for (let i = 0; i < rawN; i++) {
    const o = i * 0x18;
    let start = u64(rawT, o), size = u64(rawT, o + 8);
    const skip = start % 0x8000;
    start -= skip;
    size += skip;
    areas.push({ start, end: start + size, group: rawT.getUint32(o + 16), groups: rawT.getUint32(o + 20) });
  }
  if (grpT.byteLength < grpN * 12) throw new Error('RVZ group table is short');
  const cache = new Map(); // group -> bytes, oldest first
  function group(a, g) {
    const key = a.group + g;
    let b = cache.get(key);
    if (b) return b;
    const o = key * 12;
    const at = grpT.getUint32(o) * 4, sz = grpT.getUint32(o + 4), packed = grpT.getUint32(o + 8);
    const start = a.start + g * chunk, len = Math.min(chunk, a.end - start);
    const size = sz & 0x7fffffff;
    if (size === 0) b = new Uint8Array(len);
    else {
      b = raw(at, at + size);
      if (sz & 0x80000000) b = unz(b);
      if (packed) b = unpack(b.subarray(0, packed), start, len);
    }
    cache.set(key, b);
    if (cache.size > 24) cache.delete(cache.keys().next().value);
    return b;
  }
  function read(a, b) {
    b = Math.min(b, isoSize);
    const out = new Uint8Array(Math.max(0, b - a));
    for (let p = a; p < b; ) {
      if (p < 0x80) {
        const n = Math.min(0x80, b) - p;
        out.set(discHeader.subarray(p, p + n), p - a);
        p += n;
        continue;
      }
      const ar = areas.find((x) => p >= x.start && p < x.end);
      if (!ar) {
        // Not stored: zeros (already in out). Skip to the next area or the end.
        const next = areas.filter((x) => x.start > p).reduce((m, x) => Math.min(m, x.start), b);
        p = Math.min(next, b);
        continue;
      }
      const g = Math.floor((p - ar.start) / chunk), gs = ar.start + g * chunk;
      const data = group(ar, g);
      const n = Math.min(b, gs + data.length, ar.end) - p;
      out.set(data.subarray(p - gs, p - gs + n), p - a);
      p += n;
    }
    return out;
  }
  return { size: isoSize, read };
}

// RVZ packing: runs of stored bytes and of filler regenerated from a seed.
function unpack(src, discOffset, len) {
  const out = new Uint8Array(len), d = BE(src);
  let i = 0, o = 0;
  while (i < src.length && o < len) {
    const word = d.getUint32(i);
    i += 4;
    const n = Math.min(word & 0x7fffffff, len - o);
    if (word & 0x80000000) {
      const lfg = new Junk(src.subarray(i, i + 68));
      i += 68;
      lfg.skip((discOffset + o) % 0x8000);
      lfg.bytes(out.subarray(o, o + n));
    } else {
      out.set(src.subarray(i, i + n), o);
      i += word & 0x7fffffff;
    }
    o += n;
  }
  return out;
}

// The disc's filler generator (a lagged Fibonacci generator, k = 521, j = 32).
const K = 521, J = 32;
class Junk {
  constructor(seed) {
    const s = BE(seed), b = (this.buf = new Uint32Array(K));
    for (let i = 0; i < 17; i++) b[i] = s.getUint32(i * 4);
    for (let i = 17; i < K; i++) b[i] = (b[i - 17] << 23) ^ (b[i - 16] >>> 9) ^ b[i - 1];
    for (let i = 0; i < K; i++) b[i] = (b[i] & 0xff00ffff) | ((b[i] >>> 2) & 0x00ff0000);
    this.pos = 0;
    for (let i = 0; i < 4; i++) this.forward();
  }
  forward() {
    const b = this.buf;
    for (let i = 0; i < J; i++) b[i] ^= b[i + K - J];
    for (let i = J; i < K; i++) b[i] ^= b[i - J];
  }
  skip(n) {
    this.pos += n;
    while (this.pos >= K * 4) {
      this.forward();
      this.pos -= K * 4;
    }
  }
  bytes(out) {
    for (let o = 0; o < out.length; o++) {
      const w = this.buf[this.pos >> 2];
      out[o] = w >>> (24 - 8 * (this.pos & 3));
      if (++this.pos === K * 4) {
        this.forward();
        this.pos = 0;
      }
    }
  }
}

// ---- the disc's file table ---------------------------------------------------
// Every file by name (files in folders too, unless the top folder has the name).
function discFiles(disc) {
  const h = BE(disc.read(0x420, 0x430));
  const fst = disc.read(h.getUint32(4), h.getUint32(4) + h.getUint32(8)), d = BE(fst);
  const n = d.getUint32(8), names = n * 12;
  const name = (o) => {
    let e = names + o;
    while (fst[e]) e++;
    return String.fromCharCode(...fst.subarray(names + o, e));
  };
  const top = new Map(), deeper = new Map();
  // Folders end at entry `next`; a file is in the top folder when no folder holds it.
  const ends = [];
  for (let i = 1; i < n; i++) {
    while (ends.length && i >= ends[ends.length - 1]) ends.pop();
    const o = i * 12, isDir = fst[o] === 1, nm = name(d.getUint32(o) & 0xffffff);
    if (isDir) {
      ends.push(d.getUint32(o + 8));
      continue;
    }
    const off = d.getUint32(o + 4), size = d.getUint32(o + 8);
    const f = { size, read: (a, b) => disc.read(off + a, off + Math.min(b, size)) };
    (ends.length ? deeper : top).set(nm, f);
  }
  for (const [k, v] of deeper) if (!top.has(k)) top.set(k, v);
  return top;
}
