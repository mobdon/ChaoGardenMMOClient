// One game thread: an instance of the game build (wasm32-wasip1-threads) on
// the shared memory, with the WASI calls std makes answered here. Files come
// from folders the player picked in the page, else from the local server
// (/fs/...), both read synchronously, which workers may do; /dev/gx, /dev/pad and /dev/audio are the page's devices.
'use strict';

const E = { SUCCESS: 0, BADF: 8, INVAL: 28, IO: 29, ISDIR: 31, NOENT: 44, NOSYS: 52, NOTDIR: 54, SPIPE: 70 };
const FT = { DIR: 3, FILE: 4, CHAR: 2 };
const BLOCK = 2 << 20; // file read cache block (fewer round trips to the server)
const MAX_BLOCKS = 12;
// Files inside a compressed disc image: a chunk at a time, so a read never
// holds a game thread up unpacking megabytes.
const DISC_BLOCK = 128 << 10;
const blockSize = (path) => (isPicked(path) && picked.get(path.toLowerCase())?.read ? DISC_BLOCK : BLOCK);
// Fixed numbers for the devices, the same in every thread (a file opened on
// one game thread may be written on another, as the sound file is).
const PREOPEN = 3, DEV_AUDIO = 10, DEV_GX = 11, DEV_PAD = 12, DEV_CARD = 13, DEV_AREA = 14, DEV_GXPAGES = 15, DEV_EGG = 16, DEV_EXTRAS = 17;
const DEVICES = { 'dev/audio': DEV_AUDIO, 'dev/gx': DEV_GX, 'dev/pad': DEV_PAD, 'dev/card': DEV_CARD, 'dev/area': DEV_AREA, 'dev/gxpages': DEV_GXPAGES, 'dev/egg': DEV_EGG, 'dev/extras': DEV_EXTRAS };
const isDevice = (fd) => fd >= DEV_AUDIO && fd <= DEV_EXTRAS;
// /dev/gxpages as read since it was opened: the decoder's page bits (main.js sh.pages).
const gxPages = { bytes: new Uint8Array(0), pos: 0 };

let mem, sh, tid, args, env, instance;
const enc = new TextEncoder(), dec = new TextDecoder();
const fds = new Map(); // this thread's open files: fd -> {path, pos, size, dir}
let nextFd;
const statCache = new Map();
const blocks = new Map(); // `${path}@${n}` -> Uint8Array, oldest first
const out = { 1: '', 2: '' };
const sleeper = new Int32Array(new SharedArrayBuffer(4));

// Views on the shared memory. Another thread's memory.grow doesn't reach this
// thread's buffer by itself: grow(0) refreshes it.
let U8, DV;
function views() {
  U8 = new Uint8Array(mem.buffer);
  DV = new DataView(mem.buffer);
}
function need(end) {
  if (end > U8.length) {
    mem.grow(0);
    views();
  }
}
const dv = () => DV;
const u8 = () => U8;
// (slice() past the end returns less instead of failing: look first.)
const str = (p, n) => (need(p + n), dec.decode(u8().slice(p, p + n)));

function xhr(url, range) {
  const r = new XMLHttpRequest();
  r.open('GET', url, false);
  r.responseType = 'arraybuffer';
  if (range) r.setRequestHeader('Range', range);
  r.send();
  return r;
}

// Folders the player picked in the page (main.js): files read straight from
// their computer, no server. Names match without case, as on Windows.
let picked = null, pickedDirs, pickedRoots;
function pick(list) {
  picked = new Map();
  pickedDirs = new Map();
  pickedRoots = new Set();
  const all = [];
  for (const [path, file] of list) {
    // A disc image: its files, read from inside it (disc.js).
    if (path === '@disc') {
      importScripts('vendor/fzstd.js', 'disc.js');
      for (const [name, f] of discFiles(discReader(file))) all.push([`disc/${name}`, f]);
    } else all.push([path, file]);
  }
  for (const [path, file] of all) {
    picked.set(path.toLowerCase(), file);
    const parts = path.split('/');
    pickedRoots.add(parts[0].toLowerCase());
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join('/').toLowerCase();
      if (!pickedDirs.has(dir)) pickedDirs.set(dir, new Map());
      const last = i === parts.length - 1;
      pickedDirs.get(dir).set(parts[i].toLowerCase(), [parts[i], !last, last ? file.size : 0]);
    }
  }
}
const isPicked = (path) => picked !== null && pickedRoots.has(path.split('/')[0].toLowerCase());

// The hosted build has no serve.py: its fs-index.json lists the files under
// fs/ ({path: [is_dir, size]}), and stat and ls answer from it.
let fsIndex;
function hosted() {
  if (fsIndex === undefined) {
    // Never from the browser's cache: a stale list hid files added later (the hub shoe).
    const r = xhr('fs-index.json?t=' + Date.now());
    fsIndex = r.status === 200 ? JSON.parse(dec.decode(new Uint8Array(r.response))) : null;
  }
  return fsIndex;
}

function stat(path) {
  if (DEVICES[path] !== undefined) return { dir: false, size: 0, dev: true };
  if (isPicked(path)) {
    const k = path.toLowerCase(), f = picked.get(k);
    return f ? { dir: false, size: f.size } : pickedDirs.has(k) ? { dir: true, size: 0 } : null;
  }
  if (hosted()) {
    const e = path === '' ? [true, 0] : fsIndex[path];
    return e ? { dir: e[0], size: e[1] } : null;
  }
  if (statCache.has(path)) return statCache.get(path);
  const r = xhr('fs/' + encodeURI(path) + '?stat');
  const s = r.status === 200 ? JSON.parse(dec.decode(new Uint8Array(r.response))) : null;
  statCache.set(path, s);
  return s;
}

function block(path, n) {
  const key = path + '@' + n;
  let b = blocks.get(key);
  if (b) {
    blocks.delete(key);
    blocks.set(key, b);
    return b;
  }
  if (isPicked(path)) {
    const f = picked.get(path.toLowerCase());
    if (!f) return null;
    b = f.read ? f.read(n * DISC_BLOCK, (n + 1) * DISC_BLOCK) : new Uint8Array(new FileReaderSync().readAsArrayBuffer(f.slice(n * BLOCK, (n + 1) * BLOCK)));
  } else {
    const r = xhr('fs/' + encodeURI(path), `bytes=${n * BLOCK}-${(n + 1) * BLOCK - 1}`);
    if (r.status !== 200 && r.status !== 206) return null;
    b = new Uint8Array(r.response);
    if (r.status === 200) b = b.subarray(n * BLOCK, (n + 1) * BLOCK);
  }
  blocks.set(key, b);
  if (blocks.size > MAX_BLOCKS) blocks.delete(blocks.keys().next().value);
  return b;
}

function flushOut(fd, text) {
  out[fd] += text;
  let i;
  while ((i = out[fd].indexOf('\n')) >= 0) {
    const line = out[fd].slice(0, i);
    out[fd] = out[fd].slice(i + 1);
    postMessage({ log: line, err: fd === 2, tid });
  }
}

// ---- devices ------------------------------------------------------------

// /dev/gx: a job is 16 header bytes (magic, XFB, pipe length, RAM length),
// the pipe bytes, then RAM; it goes to the page's free slot.
const gx = { slot: -1, need: 0, at: 0, view: null };
const ctl = () => sh.ctl; // Int32Array: see main.js
function gxWrite(bytes) {
  let off = 0;
  while (off < bytes.length) {
    if (gx.slot < 0) {
      if (bytes.length - off < 20) return; // a broken job: drop it
      const h = new DataView(bytes.buffer, bytes.byteOffset + off, 20);
      const magic = h.getUint32(0, true);
      // GXJB: whole RAM; GXJD: only the changed 4 KiB pages (index + bytes each).
      if (magic === 0x47584a42) gx.need = 16 + h.getUint32(8, true) + h.getUint32(12, true);
      else if (magic === 0x47584a44) gx.need = 20 + h.getUint32(8, true) + h.getUint32(16, true) * 4100;
      else return;
      const c = ctl();
      const s = Atomics.load(c, 5);
      // Wait while the page still has both slots (it is two frames behind).
      while (Atomics.load(c, 1 + s) !== 0) Atomics.wait(c, 1 + s, 1, 100);
      gx.slot = s;
      gx.at = 0;
      gx.view = sh.slots[s];
      if (gx.need > gx.view.length) {
        postMessage({ log: `gx: a frame of ${gx.need} bytes is bigger than a slot`, err: true, tid });
        gx.need = -1;
      }
    }
    const n = Math.min(bytes.length - off, Math.abs(gx.need) - gx.at);
    if (gx.need > 0) gx.view.set(bytes.subarray(off, off + n), gx.at);
    gx.at += n;
    off += n;
    if (gx.at >= Math.abs(gx.need)) {
      const c = ctl();
      if (gx.need > 0) {
        c[3 + gx.slot] = gx.need;
        Atomics.store(c, 1 + gx.slot, 1);
        Atomics.notify(c, 1 + gx.slot); // wake decode.js
        Atomics.store(c, 5, gx.slot ^ 1);
      }
      gx.slot = -1;
    }
  }
}

// /dev/audio: the WAV the mix is written to; its 44-byte headers are skipped,
// its samples (16-bit stereo, 32 kHz) go to the page's ring.
function audioWrite(bytes) {
  if (bytes.length === 44 && bytes[0] === 0x52 && bytes[1] === 0x49) return;
  const ring = sh.audio, c = ctl(), cap = ring.length;
  const n = bytes.length >> 1;
  let w = Atomics.load(c, 6);
  const s = new Int16Array(n);
  new Uint8Array(s.buffer).set(bytes.subarray(0, n * 2));
  for (let i = 0; i < n; i++) ring[(w + i) % cap] = s[i];
  w = (w + n) >>> 0;
  Atomics.store(c, 6, w);
}

// ---- the MMO server memory card (sa2-boot card.rs, /dev/card) -------------
// Reading checks the whole card out of the server (GET /api/game-save); a
// write gathers the packed card and sync sends it (PUT). The lease and the
// revision sit in memory every game thread shares, as the page renews the
// lease from there: [0..8) revision, [8..) lease id.
const card = { read: null, pos: 0, out: [] };
function cardToken() {
  const line = (env || []).find((e) => e.startsWith('SA2_MMO_TOKEN='));
  return line ? line.slice('SA2_MMO_TOKEN='.length) : '';
}
function cardRequest(method, body) {
  const r = new XMLHttpRequest();
  r.open(method, '/api/game-save', false);
  r.responseType = 'arraybuffer';
  r.setRequestHeader('Authorization', `Bearer ${cardToken()}`);
  if (method === 'PUT') {
    const lease = dec.decode(sh.card.slice(8)).replace(/\0+$/, '');
    r.setRequestHeader('x-save-lease', lease);
    r.setRequestHeader('x-save-revision', String(new DataView(sh.card.buffer).getBigUint64(0, true)));
  }
  r.send(body);
  return r;
}
function cardCheckOut() {
  const r = cardRequest('GET');
  if (r.status !== 200) {
    postMessage({ log: `card: the server didn't check the card out (${r.status})`, err: true, tid });
    return false;
  }
  const lease = enc.encode(r.getResponseHeader('x-save-lease') || '');
  new DataView(sh.card.buffer).setBigUint64(0, BigInt(r.getResponseHeader('x-save-revision') || 0), true);
  sh.card.fill(0, 8);
  sh.card.set(lease.subarray(0, sh.card.length - 8), 8);
  card.read = new Uint8Array(r.response);
  card.pos = 0;
  return true;
}
function cardSend() {
  const all = new Uint8Array(card.out.reduce((n, b) => n + b.length, 0));
  let at = 0;
  for (const b of card.out) all.set(b, at), (at += b.length);
  card.out = [];
  const r = cardRequest('PUT', all);
  if (r.status !== 200) {
    postMessage({ log: `card: the server didn't take the save (${r.status})`, err: true, tid });
    return false;
  }
  const v = new DataView(sh.card.buffer);
  v.setBigUint64(0, v.getBigUint64(0, true) + 1n, true);
  return true;
}

// ---- MMO files (sa2-boot chao_player.rs) ----------------------------------
// The other players' Chao: the game writes its own pose and record to
// /mmo/pose-out and /mmo/record-out (and its Chao's name tag spots to
// /mmo/tags-out, the Tiny Garden tree icon to /mmo/tree-icon) and reads the others' from /mmo/pose-in
// and /mmo/rec/<id>. They live in a buffer the page shares (main.js, MMO):
// the page sends and fetches them over the MMO server.
const MMO = { POSE_OUT: 0, POSE_OUT_MAX: 256, REC_OUT: 264, REC: 2048, POSE_IN: 2320, POSE_IN_MAX: 65536, SLOTS: 67864, SLOT: 2064, NSLOT: 32, KARATE: 133912, TAGS: 133920, TAGS_MAX: 4096, TREE: 138024, TREE_MAX: 16384, CARD: 154416, CARD_MAX: 256, PICK: 154680, PICK_MAX: 32, NAME: 154720, BREED: 154728, BREED_MAX: 512, BREED_OUT: 155248, BREED_OUT_MAX: 32, STORY: 155288, STORY_MAX: 128, STORY_OUT: 155424, STORY_OUT_MAX: 8, GEAR: 155440, GEAR_MAX: 1024, RINGS: 156472, SKILLS: 156476, INFO: 156508, INFO_MAX: 256, OWN: 156772, GOTO: 158824, GOTO_MAX: 64, PVP: 158896, PVP_MAX: 72, RES_OUT: 158968, RES_IN: 159488, RES_MAX: 512 };
function mmoI32(at) { return new Int32Array(sh.mmo.buffer, at, 2); }
// Bytes of a file the game reads, or null when it doesn't exist.
function mmoRead(path) {
  const b = sh.mmo;
  if (path === 'mmo/pose-in') {
    const len = Math.min(Atomics.load(mmoI32(MMO.POSE_IN), 1), MMO.POSE_IN_MAX);
    return { bytes: b.slice(MMO.POSE_IN + 8, MMO.POSE_IN + 8 + len), ver: 0 };
  }
  // Chao Karate (sa2-boot karate.rs): the page's [press counter, button].
  if (path === 'mmo/karate-in') return { bytes: b.slice(MMO.KARATE, MMO.KARATE + 2), ver: 0 };
  // The queued match as the server has it (sa2-boot karate.rs Pvp, written by main.js pvpBout).
  if (path === 'mmo/pvp-in') return { bytes: b.slice(MMO.PVP, MMO.PVP + MMO.PVP_MAX), ver: 0 };
  // The player's Chao's name (sa2-boot kinder_chart.rs): 7 bytes, 0 ends; then the count of eggs received (card_box.rs).
  if (path === 'mmo/name-in') return { bytes: b.slice(MMO.NAME, MMO.NAME + 8), ver: 0 };
  // The server's skill numbers for the player's Chao (sa2-boot chao_player.rs
  // take_skills): 7 x [level, progress, points u16 big-endian]; no file until
  // the server has answered a record upload.
  if (path === 'mmo/skills-in') return b[MMO.SKILLS] ? { bytes: b.slice(MMO.SKILLS + 1, MMO.SKILLS + 29), ver: 0 } : null;
  // The signed-in Chao's server record (sa2-boot chao_player.rs create); no file until the page has it.
  if (path === 'mmo/own-record') return b[MMO.OWN] ? { bytes: b.slice(MMO.OWN + 4, MMO.OWN + 4 + MMO.REC), ver: 0 } : null;
  // The player's rings for the card box (sa2-boot card_box.rs): 4 bytes, little endian.
  if (path === 'mmo/rings-in') return { bytes: b.slice(MMO.RINGS, MMO.RINGS + 4), ver: 0 };
  // The stages the server lets this card start (sa2-boot story.rs); no file
  // until the page has the server's list, so the game keeps its own flags.
  if (path === 'mmo/story-in') {
    const h = mmoI32(MMO.STORY);
    if (!Atomics.load(h, 0)) return null;
    return { bytes: b.slice(MMO.STORY + 8, MMO.STORY + 8 + Math.min(Atomics.load(h, 1), MMO.STORY_MAX)), ver: 0 };
  }
  // ?geartune=1 (debug): the gear tuner's per-piece offsets (chao_player.rs).
  if (path === 'mmo/gear-tune') {
    const h = mmoI32(MMO.GEAR);
    if (!Atomics.load(h, 0)) return null;
    return { bytes: b.slice(MMO.GEAR + 8, MMO.GEAR + 8 + Math.min(Atomics.load(h, 1), MMO.GEAR_MAX)), ver: 0 };
  }
  // Staff go-to (sa2-boot chao_player.rs staff_goto); no file before the first one.
  if (path === 'mmo/goto-in') {
    const h = mmoI32(MMO.GOTO);
    if (!Atomics.load(h, 0)) return null;
    return { bytes: b.slice(MMO.GOTO + 8, MMO.GOTO + 8 + Math.min(Atomics.load(h, 1), MMO.GOTO_MAX)), ver: 0 };
  }
  // The area's character Chao from the server (sa2-boot start_stage.rs share_residents); no file before its first word.
  if (path === 'mmo/resident-in') {
    const h = mmoI32(MMO.RES_IN);
    if (!Atomics.load(h, 0)) return null;
    return { bytes: b.slice(MMO.RES_IN + 8, MMO.RES_IN + 8 + Math.min(Atomics.load(h, 1), MMO.RES_MAX)), ver: 0 };
  }
  // The chart's breeding page (sa2-boot kinder_chart.rs).
  if (path === 'mmo/breed-in') {
    const len = Math.min(Atomics.load(mmoI32(MMO.BREED), 1), MMO.BREED_MAX);
    return { bytes: b.slice(MMO.BREED + 8, MMO.BREED + 8 + len), ver: 0 };
  }
  // The memory card's Chao for the Chao Transporter (sa2-boot tiny_transporter.rs).
  if (path === 'mmo/card-chao') {
    const len = Math.min(Atomics.load(mmoI32(MMO.CARD), 1), MMO.CARD_MAX);
    return { bytes: b.slice(MMO.CARD + 8, MMO.CARD + 8 + len), ver: 0 };
  }
  const m = /^mmo\/rec\/(\d+)$/.exec(path);
  if (!m) return null;
  const s = mmoSlot(m[1]);
  if (s < 0) return null;
  const at = MMO.SLOTS + s * MMO.SLOT, h = new Int32Array(b.buffer, at + 8, 2);
  return { bytes: b.slice(at + 16, at + 16 + Math.min(h[1], MMO.REC)), ver: Atomics.load(h, 0) };
}
function mmoSlot(id) {
  const v = new DataView(sh.mmo.buffer);
  for (let i = 0; i < MMO.NSLOT; i++) if (String(v.getBigUint64(MMO.SLOTS + i * MMO.SLOT, true)) === id) return i;
  return -1;
}
// A file the game wrote, on close.
function mmoWrite(path, bytes) {
  if (path === 'mmo/karate-out') return void sh.mmo.set(bytes.subarray(0, 4), MMO.KARATE + 4); // [strong moves unlocked, press that is done, bout on, queue mark]
  const out = { 'mmo/pose-out': [MMO.POSE_OUT, MMO.POSE_OUT_MAX], 'mmo/record-out': [MMO.REC_OUT, MMO.REC],
    'mmo/tags-out': [MMO.TAGS, MMO.TAGS_MAX], 'mmo/tree-icon': [MMO.TREE, MMO.TREE_MAX], 'mmo/tiny-pick': [MMO.PICK, MMO.PICK_MAX], 'mmo/breed-out': [MMO.BREED_OUT, MMO.BREED_OUT_MAX], 'mmo/story-out': [MMO.STORY_OUT, MMO.STORY_OUT_MAX], 'mmo/resident-out': [MMO.RES_OUT, MMO.RES_MAX], 'mmo/info-out': [MMO.INFO, MMO.INFO_MAX] }[path] || null;
  if (!out) return;
  const n = Math.min(bytes.length, out[1]);
  sh.mmo.set(bytes.subarray(0, n), out[0] + 8);
  const h = mmoI32(out[0]);
  Atomics.store(h, 1, n);
  Atomics.add(h, 0, 1);
}
const isMmo = (path) => path === 'mmo' || path.startsWith('mmo/');

// ---- WASI ---------------------------------------------------------------

// The (address, length) pairs, with the memory they cover in view.
function iovs(ptr, len) {
  need(ptr + 8 * len);
  const d = dv(), list = [];
  for (let i = 0; i < len; i++) list.push([d.getUint32(ptr + 8 * i, true), d.getUint32(ptr + 8 * i + 4, true)]);
  for (const [p, n] of list) need(p + n);
  return list;
}

// A call that went past this thread's view of memory sees it again, once.
// (Calls that change state check their spans first, with need().)
function guarded(f) {
  return (...raw) => {
    // Addresses past 2 GiB arrive as negative i32s.
    const a = raw.map((x) => (typeof x === "number" ? x >>> 0 : x));
    try {
      return f(...a);
    } catch (e) {
      if (!(e instanceof RangeError)) throw e;
      const before = U8.length;
      mem.grow(0);
      views();
      try {
        return f(...a);
      } catch (e2) {
        throw new Error(`${e2} (memory ${before} -> ${U8.length} bytes, call args ${a.join(',')})`);
      }
    }
  };
}

function writeStat(ptr, s) {
  const d = dv();
  for (let i = 0; i < 64; i += 8) d.setBigUint64(ptr + i, 0n, true);
  d.setUint8(ptr + 16, s.dev ? FT.CHAR : s.dir ? FT.DIR : FT.FILE);
  d.setBigUint64(ptr + 24, 1n, true);
  d.setBigUint64(ptr + 32, BigInt(s.size || 0), true);
  if (s.ver) d.setBigUint64(ptr + 48, BigInt(s.ver) * 1000000n, true); // mtim: a new record reads as newer
}

function resolve(dirfd, p, n) {
  const name = str(p, n).replace(/^\/+/, '').replace(/\/+$/, '');
  if (dirfd === PREOPEN) return name;
  const f = fds.get(dirfd);
  if (!f || !f.dir) return null;
  return (f.path ? f.path + '/' : '') + name;
}

function lsDir(path) {
  if (isPicked(path)) return [...(pickedDirs.get(path.toLowerCase()) || new Map()).values()];
  if (hosted()) {
    const pre = path ? path + '/' : '';
    return Object.entries(fsIndex).filter(([k]) => k.startsWith(pre) && !k.slice(pre.length).includes('/')).map(([k, [d, s]]) => [k.slice(pre.length), d, s]);
  }
  const r = xhr('fs/' + encodeURI(path) + '?ls');
  const list = r.status === 200 ? JSON.parse(dec.decode(new Uint8Array(r.response))) : [];
  return list;

}

const wasi = {
  args_sizes_get(c, b) {
    const d = dv();
    d.setUint32(c, args.length, true);
    d.setUint32(b, args.reduce((n, a) => n + enc.encode(a).length + 1, 0), true);
    return 0;
  },
  args_get(argv, buf) {
    const d = dv(), m = u8();
    for (const a of args) {
      d.setUint32(argv, buf, true);
      argv += 4;
      const e = enc.encode(a);
      m.set(e, buf);
      m[buf + e.length] = 0;
      buf += e.length + 1;
    }
    return 0;
  },
  environ_sizes_get(c, b) {
    const d = dv();
    d.setUint32(c, env.length, true);
    d.setUint32(b, env.reduce((n, a) => n + enc.encode(a).length + 1, 0), true);
    return 0;
  },
  environ_get(p, buf) {
    const d = dv(), m = u8();
    for (const a of env) {
      d.setUint32(p, buf, true);
      p += 4;
      const e = enc.encode(a);
      m.set(e, buf);
      m[buf + e.length] = 0;
      buf += e.length + 1;
    }
    return 0;
  },
  clock_res_get(id, ptr) {
    dv().setBigUint64(ptr, 1000n, true);
    return 0;
  },
  clock_time_get(id, _prec, ptr) {
    // One clock for every thread: time origin + now (each worker's own
    // performance.now() starts at its own origin).
    const ms = id === 0 ? Date.now() : performance.timeOrigin + performance.now();
    dv().setBigUint64(ptr, BigInt(Math.round(ms * 1e6)), true);
    return 0;
  },
  fd_write(fd, iov, n, nw) {
    let total = 0;
    for (const [p, len] of iovs(iov, n)) {
      const bytes = u8().subarray(p, p + len);
      if (fd === 1 || fd === 2) flushOut(fd, dec.decode(bytes.slice()));
      else if (fd === DEV_GX) gxWrite(bytes);
      else if (fd === DEV_AUDIO) audioWrite(bytes);
      else if (fd === DEV_CARD) card.out.push(bytes.slice());
      else if (fd === DEV_AREA) postMessage({ area: bytes[bytes.length - 1] }); // sa2_boot::start_stage
      else if (fd === DEV_EXTRAS) postMessage({ extras: [bytes[0], bytes[1]] }); // sa2_boot::pause_extras
      else if (fds.get(fd)?.out) fds.get(fd).out.push(bytes.slice());
      else return E.BADF;
      total += len;
    }
    dv().setUint32(nw, total, true);
    return 0;
  },
  fd_read(fd, iov, n, nr) {
    if (fd === DEV_CARD) {
      let total = 0;
      for (const [p, len] of iovs(iov, n)) {
        const k = Math.min(len, (card.read?.length || 0) - card.pos);
        if (k <= 0) break;
        u8().set(card.read.subarray(card.pos, card.pos + k), p);
        card.pos += k;
        total += k;
      }
      dv().setUint32(nr, total, true);
      return 0;
    }
    if (fd === DEV_GXPAGES) {
      let total = 0;
      for (const [p, len] of iovs(iov, n)) {
        const k = Math.min(len, gxPages.bytes.length - gxPages.pos);
        if (k <= 0) break;
        u8().set(gxPages.bytes.subarray(gxPages.pos, gxPages.pos + k), p);
        gxPages.pos += k;
        total += k;
      }
      dv().setUint32(nr, total, true);
      return 0;
    }
    // /dev/egg: named eggs queued by the page (main.js sa2HatchEgg), taken by this
    // read: a u32 count, then that many 0xA8-byte slots.
    if (fd === DEV_EGG) {
      const [[p, len] = []] = iovs(iov, n);
      let total = 0;
      const count = new Int32Array(sh.egg.buffer, 0, 1), k = Math.min(Atomics.load(count, 0), 8);
      if (len >= 4 + k * 0xa8) {
        u8().set(sh.egg.subarray(4, 4 + k * 0xa8), p + 4);
        dv().setUint32(p, k, true);
        Atomics.sub(count, 0, k);
        // Anything queued while reading moves down to the front.
        sh.egg.copyWithin(4, 4 + k * 0xa8);
        total = 4 + k * 0xa8;
      }
      dv().setUint32(nr, total, true);
      return 0;
    }
    if (fd === DEV_PAD) {
      let total = 0;
      for (const [p, len] of iovs(iov, n)) {
        const k = Math.min(len, sh.pad.length - total);
        u8().set(sh.pad.subarray(total, total + k), p);
        total += k;
      }
      dv().setUint32(nr, total, true);
      return 0;
    }
    if (fd === 0) {
      dv().setUint32(nr, 0, true);
      return 0;
    }
    const f = fds.get(fd);
    if (!f) return E.BADF;
    if (f.dir) return E.ISDIR;
    if (f.mmo) {
      let total = 0;
      for (const [p, len] of iovs(iov, n)) {
        const k = Math.min(len, f.mmo.length - f.pos);
        if (k <= 0) break;
        u8().set(f.mmo.subarray(f.pos, f.pos + k), p);
        f.pos += k;
        total += k;
      }
      dv().setUint32(nr, total, true);
      return 0;
    }
    let total = 0;
    for (const [p, len] of iovs(iov, n)) {
      let want = Math.min(len, f.size - f.pos), at = p;
      while (want > 0) {
        const bs = blockSize(f.path), bn = Math.floor(f.pos / bs), b = block(f.path, bn);
        if (!b) return E.IO;
        const from = f.pos - bn * bs, k = Math.min(want, b.length - from);
        if (k <= 0) break;
        u8().set(b.subarray(from, from + k), at);
        at += k;
        f.pos += k;
        want -= k;
        total += k;
      }
    }
    dv().setUint32(nr, total, true);
    return 0;
  },
  fd_pread(fd, iov, n, off, nr) {
    const f = fds.get(fd);
    if (!f) return E.BADF;
    const keep = f.pos;
    f.pos = Number(off);
    const r = wasi.fd_read(fd, iov, n, nr);
    f.pos = keep;
    return r;
  },
  fd_seek(fd, off, whence, ptr) {
    if (isDevice(fd)) {
      dv().setBigUint64(ptr, 0n, true);
      return 0;
    }
    const f = fds.get(fd);
    if (!f) return E.BADF;
    const base = whence === 0 ? 0 : whence === 1 ? f.pos : f.size;
    const pos = base + Number(off);
    if (pos < 0) return E.INVAL;
    f.pos = pos;
    dv().setBigUint64(ptr, BigInt(pos), true);
    return 0;
  },
  fd_tell(fd, ptr) {
    const f = fds.get(fd);
    if (!f) return E.BADF;
    dv().setBigUint64(ptr, BigInt(f.pos), true);
    return 0;
  },
  fd_close(fd) {
    if (fd === DEV_GX) gx.slot = -1;
    const f = fds.get(fd);
    if (f?.out) {
      const all = new Uint8Array(f.out.reduce((k, b) => k + b.length, 0));
      let at = 0;
      for (const b of f.out) all.set(b, at), (at += b.length);
      mmoWrite(f.path, all);
    }
    fds.delete(fd);
    return 0;
  },
  fd_fdstat_get(fd, ptr) {
    const d = dv();
    let type = FT.CHAR;
    if (fd === PREOPEN) type = FT.DIR;
    else if (fds.has(fd)) type = fds.get(fd).dir ? FT.DIR : FT.FILE;
    else if (fd > 2 && !isDevice(fd)) return E.BADF;
    d.setUint8(ptr, type);
    d.setUint16(ptr + 2, 0, true);
    d.setBigUint64(ptr + 8, 0xffffffffffffffffn, true);
    d.setBigUint64(ptr + 16, 0xffffffffffffffffn, true);
    return 0;
  },
  fd_fdstat_set_flags: () => 0,
  fd_filestat_get(fd, ptr) {
    if (fd === PREOPEN) return writeStat(ptr, { dir: true }), 0;
    if (isDevice(fd)) return writeStat(ptr, { dev: true }), 0;
    const f = fds.get(fd);
    if (!f) return E.BADF;
    writeStat(ptr, f);
    return 0;
  },
  path_filestat_get(dirfd, _flags, p, n, ptr) {
    const path = resolve(dirfd, p, n);
    if (path === null) return E.BADF;
    if (isMmo(path)) {
      if (path === 'mmo' || path === 'mmo/rec') return writeStat(ptr, { dir: true }), 0;
      const f = mmoRead(path);
      if (!f) return E.NOENT;
      writeStat(ptr, { size: f.bytes.length, ver: f.ver });
      return 0;
    }
    const s = path === '' ? { dir: true } : stat(path);
    if (!s || s.dir === undefined) return E.NOENT;
    writeStat(ptr, s);
    return 0;
  },
  path_open(dirfd, _dirflags, p, n, oflags, _rb, _ri, _fdflags, fdp) {
    const path = resolve(dirfd, p, n);
    if (path === null) return E.BADF;
    if (DEVICES[path] !== undefined) {
      // The card: opened to write (O_CREAT/O_TRUNC) gathers a save, else checks the card out.
      if (path === 'dev/gxpages') {
        const n = Atomics.load(sh.pages, 0);
        gxPages.bytes = new Uint8Array(sh.pages.buffer.slice(4, 4 + 4 * n));
        gxPages.pos = 0;
      }
      if (path === 'dev/card') {
        if (!cardToken()) return E.NOENT;
        if (oflags & 9) card.out = [];
        else if (!cardCheckOut()) return E.IO;
      }
      dv().setUint32(fdp, DEVICES[path], true);
      return 0;
    }
    if (isMmo(path)) {
      // O_CREAT/O_TRUNC: the game writes it (committed on close); else read a snapshot.
      const f = oflags & 9 ? { bytes: new Uint8Array(0), ver: 0 } : mmoRead(path);
      if (!f) return E.NOENT;
      const fd = nextFd++;
      fds.set(fd, { path, pos: 0, size: f.bytes.length, dir: false, mmo: f.bytes, ver: f.ver, out: oflags & 9 ? [] : null });
      dv().setUint32(fdp, fd, true);
      return 0;
    }
    const s = path === '' ? { dir: true, size: 0 } : stat(path);
    if (!s || s.dir === undefined) return E.NOENT;
    if (oflags & 2 && !s.dir) return E.NOTDIR; // O_DIRECTORY
    const fd = nextFd++;
    fds.set(fd, { path, pos: 0, size: s.size, dir: s.dir });
    dv().setUint32(fdp, fd, true);
    return 0;
  },
  fd_readdir(fd, buf, len, cookie, usedp) {
    const f = fds.get(fd);
    if (!f || !f.dir) return E.BADF;
    if (!f.list) f.list = lsDir(f.path);
    const d = dv(), m = u8();
    let at = 0;
    for (let i = Number(cookie); i < f.list.length && at < len; i++) {
      const [name, isDir] = f.list[i];
      const e = enc.encode(name), head = new DataView(new ArrayBuffer(24));
      head.setBigUint64(0, BigInt(i + 1), true);
      head.setBigUint64(8, BigInt(i + 1), true);
      head.setUint32(16, e.length, true);
      head.setUint8(20, isDir ? FT.DIR : FT.FILE);
      const ent = new Uint8Array(24 + e.length);
      ent.set(new Uint8Array(head.buffer), 0);
      ent.set(e, 24);
      const k = Math.min(ent.length, len - at);
      m.set(ent.subarray(0, k), buf + at);
      at += k;
    }
    d.setUint32(usedp, at, true);
    return 0;
  },
  fd_prestat_get(fd, ptr) {
    if (fd !== PREOPEN) return E.BADF;
    const d = dv();
    d.setUint8(ptr, 0);
    d.setUint32(ptr + 4, 1, true);
    return 0;
  },
  fd_prestat_dir_name(fd, p, n) {
    if (fd !== PREOPEN) return E.BADF;
    u8()[p] = 0x2f; // "/"
    return 0;
  },
  poll_oneoff(inp, outp, nsubs, neventsp) {
    const d = dv();
    let wait = Infinity;
    const now = BigInt(Math.round((performance.timeOrigin + performance.now()) * 1e6));
    for (let i = 0; i < nsubs; i++) {
      const s = inp + 48 * i;
      if (d.getUint8(s + 8) !== 0) continue; // only clocks
      let t = d.getBigUint64(s + 24, true);
      if (d.getUint16(s + 40, true) & 1) t = t > now ? t - now : 0n;
      wait = Math.min(wait, Number(t) / 1e6);
    }
    if (wait !== Infinity && wait > 0) Atomics.wait(sleeper, 0, 0, wait);
    for (let i = 0; i < nsubs; i++) {
      const s = inp + 48 * i, o = outp + 32 * i;
      d.setBigUint64(o, d.getBigUint64(s, true), true);
      d.setUint16(o + 8, 0, true);
      d.setUint8(o + 10, d.getUint8(s + 8));
    }
    d.setUint32(neventsp, nsubs, true);
    return 0;
  },
  random_get(p, n) {
    need(p + n);
    const tmp = new Uint8Array(n);
    for (let i = 0; i < n; i += 65536) crypto.getRandomValues(tmp.subarray(i, Math.min(n, i + 65536)));
    u8().set(tmp, p);
    return 0;
  },
  sched_yield: () => 0,
  proc_exit(code) {
    flushOut(1, '\n');
    flushOut(2, '\n');
    postMessage({ exit: code, tid });
    throw new Error('exit ' + code);
  },
  fd_advise: () => 0,
  fd_allocate: () => 0,
  fd_datasync: () => 0,
  fd_sync: (fd) => (fd === DEV_CARD && !cardSend() ? E.IO : 0),
};

function threadSpawn(arg) {
  const t = Atomics.add(ctl(), 0, 1) + 1;
  postMessage({ spawn: t, arg });
  return t;
}

// Two steps, so the page can keep threads ready: "init" loads the game build
// into this worker (the slow part), "run" starts a thread in it.
onmessage = (ev) => {
  const m = ev.data;
  if (m.init) {
    mem = m.memory;
    sh = { ctl: new Int32Array(m.ctl), pad: new Uint8Array(m.pad), egg: new Uint8Array(m.egg), card: new Uint8Array(m.card), audio: new Int16Array(m.audio), slots: m.slots.map((s) => new Uint8Array(s)), mmo: new Uint8Array(m.mmo), pages: new Int32Array(m.pages) };
    args = m.args;
    env = m.env;
    if (m.picked && m.picked.length) pick(m.picked);
    const imports = { env: { memory: mem }, wasi_snapshot_preview1: {}, wasi: { 'thread-spawn': threadSpawn } };
    for (const imp of WebAssembly.Module.imports(m.module)) {
      if (imp.module === 'wasi_snapshot_preview1') {
        imports.wasi_snapshot_preview1[imp.name] = wasi[imp.name] ? guarded(wasi[imp.name]) : ((...a) => {
          postMessage({ log: `wasi: ${imp.name} isn't supported`, err: true, tid });
          return E.NOSYS;
        });
      }
    }
    views();
    instance = new WebAssembly.Instance(m.module, imports);
    return;
  }
  tid = m.tid;
  nextFd = 100 + tid * 1000;
  try {
    if (tid === 0) instance.exports._start();
    else instance.exports.wasi_thread_start(tid, m.arg);
    postMessage({ done: true, tid });
    close(); // the thread ended: nothing more runs here
  } catch (e) {
    if (!String(e).startsWith('Error: exit')) postMessage({ crash: String(e && e.stack || e), tid });
  }
};
