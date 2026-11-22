// The page: starts the game's threads (workers on one shared memory), sends
// them the keyboard and gamepad, plays their sound, and draws each finished
// frame they hand over with the renderer (render.js, a worker of its own).
//
// Shared with the workers (ctl, Int32): [0] last thread id, [1..2] slot full,
// [3..4] slot length, [5] slot the game fills next, [6] sound written,
// [7] sound read, [8] canvas aspect (render.js), [9] sound dropouts, [10] sound skips.

const SLOT = 96 << 20; // a frame's pipe bytes and the game RAM (SA2_MEM1_MB 80 fits; more needs a bigger slot)
const AUDIO_RING = 1 << 15; // samples (a power of two), half a second
const PAGES_START = 1024, PAGES_MAX = 16384; // 64 MiB to 1 GiB, as the game build links (browsers commit the maximum)
const q = new URLSearchParams(location.search);
// The MMO has no title or card screen: without a signed-in Chao, go to the login.
if (!mmoToken()) location.replace('/');
const wide = q.get('aspect') !== '4:3';
const fpsCap = Number(q.get('fps')) || 0; // 60: never blend between frames

const $ = (id) => document.getElementById(id);
const canvas = $('screen'), status = $('status'), logBox = $('log');
const lines = [];
const gameLog = (window.gameLog = []);
function log(text, err) {
  gameLog.push(text);
  if (gameLog.length > 3000) gameLog.shift();
  (err ? console.warn : console.log)(text);
  if (!err) return;
  lines.push(text);
  if (lines.length > 12) lines.shift();
  logBox.textContent = lines.join('\n');
}

const sh = {
  ctl: new SharedArrayBuffer(64),
  pad: new SharedArrayBuffer(24),
  egg: new SharedArrayBuffer(4 + 8 * 0xa8),
  audio: new SharedArrayBuffer(AUDIO_RING * 2),
  slots: [new SharedArrayBuffer(SLOT), new SharedArrayBuffer(SLOT)],
  card: new SharedArrayBuffer(64), // the server card's revision and lease (worker.js)
  mmo: new SharedArrayBuffer(67864 + 32 * 2064 + 8 + 8 + 4096 + 8 + 16384 + 8 + 256 + 8 + 32), // the MMO files (worker.js, MMO)
  // The RAM pages the decoder read for its last frame: [0] word count, then bits (decode.js, worker.js /dev/gxpages).
  pages: new SharedArrayBuffer(4 + 65536),
};
const ctl = new Int32Array(sh.ctl), pad = new Uint8Array(sh.pad);
// A named egg hatches in the Chao Lobby (sa2_boot::start_stage reads /dev/egg).
// Up to 8 queued; each slot is a u32 "has gene" flag then an AL_GENE (0xA4 bytes,
// al_chao_info.h) holding the server egg's own alleles where the game keeps them.
window.sa2HatchEgg = egg => {
  const count = new Int32Array(sh.egg, 0, 1), n = Atomics.load(count, 0);
  if (n >= 8) return false;
  const slot = new DataView(sh.egg, 4 + n * 0xa8, 0xa8), g = egg?.gene;
  for (let i = 0; i < 0xa8; i++) slot.setUint8(i, 0);
  if (g) {
    slot.setUint32(0, 1, true);
    const at = 4, pair = (o, [a, b]) => { slot.setInt8(at + o, Number(a)); slot.setInt8(at + o + 1, Number(b)); };
    if (egg.eggColor != null) slot.setInt8(at + 0x01, egg.eggColor);
    g.abl.forEach((p, i) => pair(0x5c + i * 2, p));
    pair(0x6c, g.lifeTime);
    g.personality.forEach((p, i) => pair(0x74 + i * 2, p));
    pair(0x8e, g.taste); pair(0x90, g.tv); pair(0x92, g.music); pair(0x94, g.color);
    pair(0x96, g.nonTex); pair(0x98, g.jewel); pair(0x9a, g.multi);
  }
  Atomics.add(count, 0, 1);
  return true;
};

// ---- input --------------------------------------------------------------
// Bits as the game reads them (sa2-web-game KEYS): A D S W, Space, E,
// Left Shift, Q, R, Enter, Escape, 1, 3, then left and right mouse; bit 15 is Tab
// (and the gamepad's Back), the Chao World chart (sa2-boot kinder_chart.rs).
const KEYS = ['KeyA', 'KeyD', 'KeyS', 'KeyW', 'Space', 'KeyE', 'ShiftLeft', 'KeyQ', 'KeyR', 'Enter', 'Escape', 'Digit1', 'Digit3'];
const TAB = 1 << 15;
let held = 0;
function hold(bit, down) {
  held = down ? held | bit : held & ~bit;
}
addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement) return; // typing in chat, not playing
  if (karateKey(e)) return;
  if (e.code === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey) { e.preventDefault(); hold(TAB, true); return; }
  const i = KEYS.indexOf(e.code);
  if (i < 0 || e.ctrlKey || e.altKey || e.metaKey) return;
  e.preventDefault();
  hold(1 << i, true);
});
addEventListener('keyup', (e) => {
  if (e.code === 'Tab') hold(TAB, false);
  const i = KEYS.indexOf(e.code);
  if (i >= 0) hold(1 << i, false);
});
addEventListener('blur', () => (held = 0));
canvas.addEventListener('mousedown', (e) => e.button === 0 ? hold(1 << 13, true) : e.button === 2 && hold(1 << 14, true));
// The last left click for the game (pad bytes 16..24, sa2-web-game read_pad): where on the
// canvas (0..65535 each way), the canvas aspect (ctl[8]), and a count that changes per click.
let clicks = 0;
canvas.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const r = canvas.getBoundingClientRect();
  const u = (v) => Math.round(Math.min(1, Math.max(0, v)) * 65535);
  new DataView(sh.pad).setUint16(16, u((e.clientX - r.left) / r.width), true);
  new DataView(sh.pad).setUint16(18, u((e.clientY - r.top) / r.height), true);
  new DataView(sh.pad).setUint16(20, Atomics.load(ctl, 8), true);
  new DataView(sh.pad).setUint16(22, (clicks = (clicks + 1) & 0xffff), true);
});
addEventListener('mouseup', (e) => e.button === 0 ? hold(1 << 13, false) : e.button === 2 && hold(1 << 14, false));
canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('dblclick', () => document.fullscreenElement ? document.exitFullscreen() : canvas.requestFullscreen());

// The first standard-layout gamepad, mapped as the desktop build maps it:
// A/Cross = A, X/Square = B, B/Circle = X, Y/Triangle = Y, Start, RB = Z,
// LT/RT = L/R (analog), d-pad.
const BUTTONS = [[0, 0x0100], [2, 0x0200], [1, 0x0400], [3, 0x0800], [9, 0x1000], [5, 0x0010],
  [14, 0x0001], [15, 0x0002], [13, 0x0004], [12, 0x0008]];
function writePad() {
  writeExtras();
  const g = [...(navigator.getGamepads ? navigator.getGamepads() : [])].find((p) => p && p.connected);
  const keys = held | (g && g.buttons[8] && g.buttons[8].pressed ? TAB : 0);
  pad[0] = keys & 0xff;
  pad[1] = keys >> 8;
  if (!g) {
    pad[2] = 0;
    return;
  }
  let b = 0;
  for (const [i, bit] of BUTTONS) if (g.buttons[i] && g.buttons[i].pressed) b |= bit;
  const trig = (i) => Math.round(Math.min(1, Math.max(0, g.buttons[i] ? g.buttons[i].value : 0)) * 255);
  const lt = trig(6), rt = trig(7);
  if (lt > 200) b |= 0x0040;
  if (rt > 200) b |= 0x0020;
  const ax = (i, s) => Math.round(Math.max(-1, Math.min(1, (g.axes[i] || 0) * s)) * 100) & 0xff;
  pad[2] = 1;
  pad[4] = b & 0xff;
  pad[5] = b >> 8;
  pad[6] = ax(0, 1);
  pad[7] = ax(1, -1);
  pad[8] = ax(2, 1);
  pad[9] = ax(3, -1);
  pad[10] = lt;
  pad[11] = rt;
}

// ---- the player's own files -----------------------------------------------
// Folders picked on the start screen, as [game path, File]: the game sees the
// Steam folder as /steam and the disc folder as /disc. A folder not picked
// comes from the local server, if it has one.
let picked = [];
function pickedFiles() {
  const list = [];
  for (const [id, mount] of [['steam-dir', 'steam'], ['disc-dir', 'disc']])
    for (const f of $(id).files) list.push([`${mount}/${f.webkitRelativePath.split('/').slice(1).join('/')}`, f]);
  // A disc image (.iso, .gcm, .rvz): each game thread opens it itself (disc.js).
  if ($('disc-img').files[0]) list.push(['@disc', $('disc-img').files[0]]);
  return list.concat(addedFiles);
}
// pickFolder('steam', [[path in folder, File], …]): the same from a script
// (tests: the browser's automation can't fill a folder input).
const addedFiles = [];
window.pickFolder = (mount, entries) => entries.forEach(([p, f]) => addedFiles.push([`${mount}/${p}`, f]));
// The files the player picked once on the MMO login page (src/game-files.js, same site's IndexedDB).
async function savedFolders() {
  const db = await new Promise((resolve) => {
    const r = indexedDB.open('sa2-game-files', 1);
    r.onupgradeneeded = () => r.result.createObjectStore('handles');
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => resolve(null);
  });
  if (!db) return;
  const get = (key) => new Promise((resolve) => {
    const r = db.transaction('handles').objectStore('handles').get(key);
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => resolve(null);
  });
  const walk = async (dir, path) => {
    for await (const entry of dir.values()) {
      if (entry.kind === 'directory') await walk(entry, `${path}${entry.name}/`);
      else addedFiles.push([`steam/${path}${entry.name}`, await entry.getFile()]);
    }
  };
  for (const [key, handle] of [['steam', await get('steam')], ['disc', await get('disc')]]) {
    if (!handle) continue;
    if (await handle.queryPermission({ mode: 'read' }) !== 'granted') {
      log(`game files: no read access to the picked ${key === 'steam' ? 'Steam folder' : 'disc image'}; log in again to allow it`, true);
      continue;
    }
    if (key === 'steam') await walk(handle, '');
    else addedFiles.push(['@disc', await handle.getFile()]);
  }
}

// ---- threads ------------------------------------------------------------
let module, memory, env, workers = 0;
// Workers with the game already loaded, so a new game thread (a stage load,
// an event) starts at once instead of loading 80 MB first.
const POOL = 4;
const pool = [];
function loadedWorker() {
  const w = new Worker('worker.js');
  w.onmessage = (ev) => {
    const m = ev.data;
    if (m.spawn !== undefined) startThread(m.spawn, m.arg);
    else if (m.log !== undefined) log(m.log, /panicked|can't|error|stopped/i.test(m.log));
    else if (m.crash) log(`game thread ${m.tid} stopped: ${m.crash}`, true);
    else if (m.exit !== undefined) log(`the game ended (exit ${m.exit})`, true);
    else if (m.done) workers--;
    else if (m.area === 254 || m.area === 253) cover(m.area === 254); // sa2_boot::start_stage: boot logos
    else if (SHOE_PICKUPS[m.area]) pickedUp(SHOE_PICKUPS[m.area]);
    else if (m.area !== undefined) roomArea(m.area);
    else if (m.extras !== undefined) extrasChanged(...m.extras);
  };
  w.onerror = (e) => log(`game thread: ${e.message}`, true);
  w.postMessage({ init: true, module, memory, env, picked, args: ['sa2-host', '/research/main.dol', '/research/symbols.txt'], ...sh });
  return w;
}
function startThread(tid, arg) {
  const w = pool.shift() || loadedWorker();
  workers++;
  w.postMessage({ tid, arg });
  while (pool.length < POOL) pool.push(loadedWorker());
}

// ---- the MMO server memory card -------------------------------------------
// The MMO title screen keeps the signed-in Chao's token (src/save-slots.js).
function mmoToken() {
  try { return localStorage.getItem('chao-garden-session-v1') || ''; } catch { return ''; }
}
// The game's own shoe upgrade, picked up in a level (sa2_boot::gear_pickup):
// the server lets this card's Chao wear the shoes from then on.
const SHOE_PICKUPS = { 252: 'sonic-light-shoes', 251: 'shadow-air-shoes' };
function pickedUp(item) {
  const token = mmoToken();
  if (!token) return;
  fetch('/api/gear-pickup', { method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ item }) }).then(r => r.ok, () => false).then(ok => ok || log(`could not tell the server about ${item}`, true));
}
// In a Chao World area from a hub portal, stay in that zone's shared room on
// the MMO server (same socket and messages as the hub page, src/main.js).
// The game's own Chao Lobby is the MMO hub: stage 90 with no area.
// One server room per ChaoMain area (server CHAO_WORLD_ZONES).
const roomZones = { '': 'hub', 1: 'garden', 2: 'hero', 3: 'dark', 4: 'chao-race', 5: 'race-entrance', 6: 'kindergarten', 9: 'stadium', 10: 'chao-karate', 11: 'chao-karate' };
let room = null;
function joinRoom(token, zone) {
  const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws?token=${encodeURIComponent(token)}`);
  room = { ws, zone };
  ws.onopen = () => ws.send(JSON.stringify({ type: 'action', action: 'chao-world', area: room.zone }));
  // The MMO overlay (Inventory, breeding, Tiny Garden) is the client's own module; the game boots without it.
  let overlay = null;
  import('/src/game-overlay.js').then(m => { overlay = m.mountOverlay(ws); }).catch(() => {});
  ws.onmessage = ({ data }) => {
    let m; try { m = JSON.parse(data); } catch { return; }
    if (m.type === 'snapshot') { showCard(m.players.find(p => p.id === m.selfId)); overlay?.snapshot(m); othersIn(m, token); tagNames(m); }
    if (m.type === 'event') overlay?.event(m);
    if (m.kind === 'chat') chatLine(m); else if (m.kind === 'voice') voicePlay(m);
  };
  // 4001: this card's Chao went out somewhere else; don't fight over it.
  ws.onclose = ({ code }) => { if (code !== 4001) setTimeout(() => joinRoom(token, room.zone), 3000); };
}
// ---- the other players' Chao (sa2-boot chao_player.rs, worker.js MMO files) --
// The game's own pose goes to the room each 100 ms; the room's other players
// with a pose come back as lines "id x y z angle motion frame gear" (a Tiny
// Garden resident with no pose as "id idle"); Chao
// records go to and from the server by version. The server routes may not
// exist yet: failures stay quiet.
const MMO = { POSE_OUT: 0, REC_OUT: 264, REC: 2048, POSE_IN: 2320, POSE_IN_MAX: 65536, SLOTS: 67864, SLOT: 2064, NSLOT: 32, KARATE: 133912, TAGS: 133920, TAGS_MAX: 4096, TREE: 138024, TREE_MAX: 16384, CARD: 154416, CARD_MAX: 256, PICK: 154680, PICK_MAX: 32 };
const mmoBytes = new Uint8Array(sh.mmo);
globalThis.sa2Mmo = sh.mmo; // for checks from the console
const mmoHead = (at) => new Int32Array(sh.mmo, at, 2);
// The game keys remote Chao by a number: the first 52 bits of the player id.
const numId = (id) => String(BigInt('0x' + (String(id).replace(/[^0-9a-f]/gi, '').slice(0, 13) || '0')));
let poseSeen = 0, recSeen = 0, pickSeen = 0, poseSent = null, poseSeq = 0, poseFullAt = 0, poseWs = null;
// The memory card's Chao for the Chao Transporter's picker (sa2-boot
// tiny_transporter.rs): their ids to /mmo/card-chao in the card's order, their
// records to the record slots. A Chao that never played has no record and is
// left out. The card code is the login page's (src/save-slots.js).
const cardIds = new Map(); // numId -> the server's Chao (player)
async function cardChao(token) {
  try {
    const card = JSON.parse(localStorage.getItem('chao-garden-slots-v1') || '[]').find((r) => r.token === token)?.card;
    if (!card) return;
    const r = await fetch('/api/card', { headers: { Authorization: `Bearer ${card}` } });
    const lines = [];
    for (const { player } of r.ok ? (await r.json()).chao || [] : []) {
      const rec = await fetch(`/api/chao-record/${encodeURIComponent(player.id)}`, { headers: { Authorization: `Bearer ${token}` } });
      if (!rec.ok) continue;
      recordIn(numId(player.id), new Uint8Array(await rec.arrayBuffer()));
      cardIds.set(numId(player.id), player);
      lines.push(numId(player.id));
    }
    const text = new TextEncoder().encode(lines.join('\n') + '\n').subarray(0, MMO.CARD_MAX);
    mmoBytes.set(text, MMO.CARD + 8);
    Atomics.store(mmoHead(MMO.CARD), 1, text.length);
  } catch {}
}
const recVersions = new Map();
function mmoTick(token) {
  karateLocks();
  const pose = mmoHead(MMO.POSE_OUT), seq = Atomics.load(pose, 0);
  if (seq !== poseSeen && room?.ws.readyState === 1) {
    poseSeen = seq;
    try {
      const text = new TextDecoder().decode(mmoBytes.slice(MMO.POSE_OUT + 8, MMO.POSE_OUT + 8 + pose[1]));
      // Only the keys that changed, with a sequence number; the whole pose
      // each second and after joining or changing room (server merges keys).
      const cur = JSON.parse(text), now = performance.now(), full = !poseSent || room.ws !== poseWs || now - poseFullAt > 1000;
      const out = { seq: ++poseSeq };
      for (const k of ['x', 'y', 'z', 'angle', 'motion', 'frame', 'gear']) if (full || cur[k] !== poseSent[k]) out[k] = cur[k];
      if (full) poseFullAt = now, poseWs = room.ws;
      poseSent = cur;
      if (full || Object.keys(out).length > 1) room.ws.send(JSON.stringify({ type: 'action', action: 'chao-pose', pose: out }));
    } catch {}
  }
  const pick = mmoHead(MMO.PICK), pseq = Atomics.load(pick, 0);
  if (pseq !== pickSeen) {
    pickSeen = pseq;
    // The Chao picked in the Chao Transporter becomes the phone's: its picture
    // sheet (src/tiny-sheet.js) goes to the server, which keeps one per card.
    const picked = cardIds.get(new TextDecoder().decode(mmoBytes.slice(MMO.PICK + 8, MMO.PICK + 8 + pick[1])));
    if (picked) import('/src/tiny-sheet.js').then((m) => m.renderTinySheet(picked)).then((sheet) => {
      console.log('tiny-transporter: picked', picked.id, sheet ? `sheet ${sheet.size} bytes` : 'no sheet');
      return sheet && fetch(`/api/tiny-chao?chao=${encodeURIComponent(picked.id)}`, { method: 'PUT', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'image/png' }, body: sheet });
    }).then((r) => r && console.log('tiny-transporter: sheet stored', r.status)).catch((e) => console.log('tiny-transporter: sheet failed', String(e)));
  }
  const rec = mmoHead(MMO.REC_OUT), rseq = Atomics.load(rec, 0);
  if (rseq !== recSeen) {
    recSeen = rseq;
    const body = mmoBytes.slice(MMO.REC_OUT + 8, MMO.REC_OUT + 8 + rec[1]);
    fetch('/api/chao-record', { method: 'PUT', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/octet-stream' }, body }).catch(() => {});
  }
}
function othersIn(m, token) {
  const lines = [];
  for (const p of m.players || []) {
    if (p.id === m.selfId) continue;
    const c = p.chaoPose;
    if (c) lines.push([numId(p.id), c.x, c.y, c.z, c.angle | 0, c.motion | 0, c.frame || 0, c.gear | 0].join(' '));
    // Away in Tiny Chao Garden (the server's resident): the Chao stays in its garden on the game's own AI.
    else if (p.watch?.resident) lines.push(`${numId(p.id)} idle`);
    if (p.chaoRecordVersion !== undefined && recVersions.get(p.id) !== p.chaoRecordVersion) {
      recVersions.set(p.id, p.chaoRecordVersion);
      fetch(`/api/chao-record/${encodeURIComponent(p.id)}`, { headers: { Authorization: `Bearer ${token}` } })
        .then((r) => (r.ok ? r.arrayBuffer() : null)).then((b) => b && recordIn(numId(p.id), new Uint8Array(b))).catch(() => {});
    }
  }
  const text = new TextEncoder().encode(lines.join('\n') + (lines.length ? '\n' : ''));
  const n = Math.min(text.length, MMO.POSE_IN_MAX);
  mmoBytes.set(text.subarray(0, n), MMO.POSE_IN + 8);
  Atomics.store(mmoHead(MMO.POSE_IN), 1, n);
}
// A remote Chao's record into its slot (by id, else the first free one), with a new version.
function recordIn(id, bytes) {
  const v = new DataView(sh.mmo);
  let slot = -1, free = -1;
  for (let i = 0; i < MMO.NSLOT; i++) {
    const k = String(v.getBigUint64(MMO.SLOTS + i * MMO.SLOT, true));
    if (k === id) slot = i;
    if (k === '0' && free < 0) free = i;
  }
  if (slot < 0) slot = free;
  if (slot < 0) return;
  const at = MMO.SLOTS + slot * MMO.SLOT, h = new Int32Array(sh.mmo, at + 8, 2);
  mmoBytes.set(bytes.subarray(0, MMO.REC), at + 16);
  h[1] = Math.min(bytes.length, MMO.REC);
  v.setBigUint64(at, BigInt(id), true);
  Atomics.add(h, 0, 1);
}

// ---- name tags (sa2-boot name_tags.rs) -----------------------------------
// The game writes each tagged Chao's spot once a frame to /mmo/tags-out as
// lines "label x y" (x, y: normalised device coordinates of its highest node,
// drawn for 4:3); the tags are the old hub's name plates, drawn over the
// canvas. Labels: "me", "p<id>" (numId), "r23" / "r24" / "r25" the residents.
// A Tiny Garden resident (snapshot watch.resident) shows the game's own tree
// icon (/mmo/tree-icon: u32 width, height, RGBA) on the left inside its tag.
const RESIDENT_TAGS = { r23: 'Tails · NPC', r24: 'Knuckles · NPC', r25: 'Amy · NPC' };
const tagInfo = new Map();
function tagNames(m) {
  tagInfo.clear();
  for (const p of m.players || []) {
    tagInfo.set(p.id === m.selfId ? 'me' : `p${numId(p.id)}`, { name: `${p.admin ? 'ADMIN • ' : ''}${p.name || 'Chao'}`, admin: !!p.admin, tiny: !!p.watch?.resident });
  }
}
let tagLayer = null, tagSeen = -1, tagAt = 0, treeUrl = null;
const tagEls = new Map();
function treeIcon() {
  if (treeUrl !== null) return treeUrl;
  const h = mmoHead(MMO.TREE);
  if (!Atomics.load(h, 0)) return null;
  const v = new DataView(sh.mmo, MMO.TREE + 8), w = v.getUint32(0, true), ht = v.getUint32(4, true);
  if (!w || !ht || w * ht * 4 + 8 > h[1]) return (treeUrl = '');
  const c = Object.assign(document.createElement('canvas'), { width: w, height: ht });
  c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(mmoBytes.slice(MMO.TREE + 16, MMO.TREE + 16 + w * ht * 4)), w, ht), 0, 0);
  return (treeUrl = c.toDataURL());
}
function tagEl(label) {
  let el = tagEls.get(label);
  if (!el) {
    el = document.createElement('div');
    el.className = 'tag';
    el.append(Object.assign(document.createElement('img'), { alt: '' }), document.createElement('span'));
    tagLayer.append(el);
    tagEls.set(label, el);
  }
  const info = tagInfo.get(label), name = RESIDENT_TAGS[label] || info?.name || '';
  if (el.lastChild.textContent !== name) el.lastChild.textContent = name;
  el.classList.toggle('admin', !!info?.admin);
  const tree = info?.tiny ? treeIcon() : '';
  el.firstChild.hidden = !tree;
  if (tree && el.firstChild.src !== tree) el.firstChild.src = tree;
  return el;
}
function drawTags() {
  if (!tagLayer) {
    tagLayer = Object.assign(document.createElement('div'), { id: 'tags' });
    // The styles travel with this script, so a page that only takes main.js still places the tags.
    // The plate is the old hub's (src/scene.js label): admin is gold on red.
    const css = document.createElement('style');
    css.textContent = `#tags { position: fixed; inset: 0; pointer-events: none; overflow: hidden; }
.tag { position: absolute; left: 0; top: 0; margin-top: -.3rem; display: flex; align-items: center; gap: .3rem; padding: .15rem .7rem; border-radius: 1rem; background: rgba(18,42,42,.84); color: #e8f7ef; font: 600 13px system-ui, sans-serif; white-space: nowrap; max-width: 16rem; overflow: hidden; text-overflow: ellipsis; }
.tag[hidden] { display: none; }
.tag.admin { background: rgba(117,27,35,.96); color: #ffdc72; }
.tag img { width: 16px; height: 16px; image-rendering: pixelated; }`;
    document.head.append(css);
    document.body.append(tagLayer);
  }
  const h = mmoHead(MMO.TAGS), seq = Atomics.load(h, 0);
  if (seq !== tagSeen) {
    tagSeen = seq;
    tagAt = performance.now();
    const text = new TextDecoder().decode(mmoBytes.slice(MMO.TAGS + 8, MMO.TAGS + 8 + Math.min(h[1], MMO.TAGS_MAX)));
    const r = canvas.getBoundingClientRect(), aspect = r.width / Math.max(1, r.height);
    // render: a wide canvas widens the 3D view, x scaled by (4/3) / aspect (sa2_gx::widescreen).
    const k = wide && aspect > 4 / 3 ? 4 / 3 / aspect : 1;
    const shown = new Set();
    for (const line of text.split('\n')) {
      const [label, x, y] = line.split(' ');
      if (!label || !(label in RESIDENT_TAGS || tagInfo.has(label))) continue;
      const el = tagEl(label);
      el.style.transform = `translate(${r.left + ((Number(x) * k + 1) / 2) * r.width}px, ${r.top + ((1 - Number(y)) / 2) * r.height}px) translate(-50%, -100%)`;
      shown.add(label);
    }
    for (const [label, el] of tagEls) el.hidden = !shown.has(label);
  } else if (performance.now() - tagAt > 250) for (const el of tagEls.values()) el.hidden = true; // out of Chao World
}

// While the boot logos run before an MMO start, the game is covered (plain
// black, as the page looks before the first frame) and its sound muted.
function cover(on) {
  let c = document.getElementById('mmo-cover');
  if (!c) {
    c = Object.assign(document.createElement('div'), { id: 'mmo-cover' });
    c.style.cssText = 'position:fixed;inset:0;background:#000;z-index:5;pointer-events:none';
    document.body.append(c);
  }
  c.style.display = on ? 'block' : 'none';
  if (gameVolume) gameVolume.gain.value = on ? 0 : Number.isInteger(mmoSettings.volume) ? mmoSettings.volume / 10 : 1;
}

// The game changed Chao World area (its own teleporters): follow it into that room.
function roomArea(area) {
  karateButtons(area === 10 || area === 11);
  const zone = roomZones[area === 0 ? '' : area];
  // Arrived in Chao World from a level (the pause menu's EXIT): no room yet.
  if (!room && zone && mmoToken()) return joinRoom(mmoToken(), zone);
  if (!room || !zone || zone === room.zone) return;
  room.zone = zone;
  if (room.ws.readyState === 1) room.ws.send(JSON.stringify({ type: 'action', action: 'chao-world', area: zone }));
}
// ---- Chao Karate attack buttons (sa2-boot karate.rs) ----------------------
// The player's Chao does the pressed attack at its next attack; the game
// refuses the two strong ones until Power is above 400, and says so here.
// shortcut: plain page buttons; the game's own font and capsule sprites are
// not drawn here yet, upgrade when the page can read the player's textures.
const KARATE_MOVES = [[4, 'Hard Kick'], [7, 'Punch'], [6, 'Beating Up'], [8, 'Windup Punch', true], [5, 'Roundhouse', true]];
function karateButtons(on) {
  let bar = document.getElementById('karate');
  if (!bar && on) {
    bar = Object.assign(document.createElement('div'), { id: 'karate' });
    bar.style.cssText = 'position:fixed;left:0;right:0;bottom:12vh;display:flex;gap:.5rem;justify-content:center;z-index:4';
    for (const [id, name, strong] of KARATE_MOVES) {
      const b = Object.assign(document.createElement('button'), { type: 'button', textContent: `${bar.children.length + 1} ${name}` });
      b.style.cssText = 'padding:.45rem 1rem;border-radius:1.2rem;background:#f5a800;color:#fff;font-weight:700;text-shadow:0 1px 2px #000';
      if (strong) b.dataset.strong = '1';
      b.addEventListener('click', () => {
        mmoBytes[MMO.KARATE + 1] = id;
        mmoBytes[MMO.KARATE]++; // a new press, after its button is in place
        canvas.focus();
      });
      bar.append(b);
    }
    document.body.append(bar);
  }
  if (bar) bar.style.display = on ? 'flex' : 'none';
}
// Keys 1 to 5 press the buttons while they show (1 and 3 are the game's L and R elsewhere).
function karateKey(e) {
  const bar = document.getElementById('karate'), n = '12345'.indexOf(e.key);
  if (!bar || bar.style.display === 'none' || n < 0 || e.ctrlKey || e.altKey || e.metaKey) return false;
  e.preventDefault();
  if (!e.repeat && !bar.children[n].disabled) bar.children[n].click();
  return true;
}
function karateLocks() {
  for (const b of document.querySelectorAll('#karate [data-strong]')) {
    b.disabled = !mmoBytes[MMO.KARATE + 4];
    b.title = b.disabled ? 'Needs Power above 400' : '';
    b.style.background = b.disabled ? '#666' : '#f5a800';
  }
}
// The MMO player card over the game (name, Chao select icon, rings); it never takes focus.
function showCard(player) {
  const card = document.getElementById('mmo-card');
  if (!player || !card) return; // the card element comes from the MMO overlay
  card.style.display = 'flex';
  // The player's own Chao as the game's Chao Select shows it (src/chao-portrait.js); drawn once.
  const icon = card.querySelector('.icon');
  if (!icon.dataset.chao) {
    icon.dataset.chao = '1';
    const canvas = Object.assign(document.createElement('canvas'), { width: 96, height: 120 });
    canvas.style.cssText = 'width:48px;height:60px;display:block';
    icon.replaceChildren(canvas);
    import('/src/chao-portrait.js').then(m => m.mountChaoPortrait(canvas, player)).catch(() => {});
  }
  card.querySelector('.name').textContent = player.name;
  card.querySelector('.rings').textContent = player.rings.toLocaleString();
}
// Hold the card while the game runs: renew its lease (10 minutes on the
// server) and give the Chao back when the tab closes.
function keepCard(token) {
  const lease = () => new TextDecoder().decode(new Uint8Array(sh.card).slice(8)).replace(/\0+$/, '');
  const call = (method, keepalive) => lease() && fetch('/api/game-save/lease', { method, keepalive,
    headers: { Authorization: `Bearer ${token}`, 'x-save-lease': lease() } }).catch(() => {});
  setInterval(() => call('POST'), 120_000);
  addEventListener('pagehide', () => call('DELETE', true));
}

// ---- drawing (render.js, a worker of its own) ------------------------------
let renderer, picture = '';
const ticksMain = [];
// The page's own loop: the controller and the canvas's size, each refresh.
function frame() {
  writePad();
  drawTags();
  const dpr = devicePixelRatio || 1;
  const w = Math.round(canvas.clientWidth * dpr), h = Math.round(canvas.clientHeight * dpr);
  if (renderer && (w !== frame.w || h !== frame.h)) {
    frame.w = w;
    frame.h = h;
    renderer.postMessage({ size: [w, h] });
  }
  requestAnimationFrame(frame);
}
setInterval(() => {
  const drop = Atomics.exchange(ctl, 9, 0), skip = Atomics.exchange(ctl, 10, 0);
  const sound = drop || skip ? ` · sound: ${drop} dropouts, ${skip} skips` : '';
  status.textContent = `${picture}${sound} · ${workers} threads`;
}, 1000);

async function start() {
  $('go').disabled = true;
  await savedFolders().catch((e) => log(`game files: ${e}`, true));
  picked = pickedFiles();
  status.textContent = 'loading…';
  if (!crossOriginIsolated) throw new Error('the page needs cross-origin isolation (open it from the local server)');
  const audio = new AudioContext({ latencyHint: 'interactive' });
  await audio.audioWorklet.addModule('audio.js');
  const mix = new AudioWorkletNode(audio, 'sa2-mix', { outputChannelCount: [2], processorOptions: { ctl: sh.ctl, audio: sh.audio } });
  // Master volume for the MMO sound corner (src/game-overlay.js) and the Extras page.
  gameVolume = globalThis.sa2Gain = audio.createGain();
  mix.connect(gameVolume).connect(audio.destination);
  const dpr = devicePixelRatio || 1;
  canvas.width = Math.round(canvas.clientWidth * dpr);
  canvas.height = Math.round(canvas.clientHeight * dpr);
  const off = canvas.transferControlToOffscreen();
  renderer = new Worker('render.js', { type: 'module' });
  renderer.onmessage = (ev) => {
    if (ev.data.status) picture = ev.data.status;
    else if (ev.data.log) log(ev.data.log, true);
  };
  renderer.postMessage({ canvas: off, ctl: sh.ctl, slots: sh.slots, pages: sh.pages, wide, fpsCap }, [off]);
  // Steam first: its files win, a picked disc only fills what Steam lacks.
  const e = await (await fetch('env.json')).json();
  delete e.SA2_DISC_FIRST;
  if (picked.some(([p]) => p === '@disc' || p.startsWith('disc/'))) e.SA2_DISC_DIR = '/disc';
  e.SA2_STEAM_DIR ??= '/steam';
  // Signed in on the Chao MMO title screen (same site): the server memory card is in slot A.
  // ?stage=N from the Chao MMO's Stage Select: start that stage from boot (sa2_boot::start_stage).
  if (/^\d+$/.test(q.get('stage') || '')) e.SA2_START_STAGE = q.get('stage');
  if (/^\d+$/.test(q.get('area') || '')) e.SA2_CHAO_AREA = q.get('area');
  // ?stage=60&combat=1: Big Foot's arena as the combat area (sa2_boot::combat).
  if (q.get('combat') === '1') e.SA2_COMBAT = '1';
  if (/^\d+$/.test(q.get('wave') || '')) e.SA2_COMBAT_WAVE = q.get('wave');
  // A new player (no Chao on the card yet): their egg is laid in the lobby.
  if (q.get('hatch') === '1') e.SA2_HATCH_EGG = '1';
  const token = mmoToken();
  if (token) e.SA2_MMO_TOKEN = token;
  if (token) Object.assign(e, { SA2_POSE_OUT: '/mmo/pose-out', SA2_POSE_IN: '/mmo/pose-in', SA2_RECORD_OUT: '/mmo/record-out', SA2_RECORD_IN: '/mmo/rec', SA2_KARATE_IN: '/mmo/karate-in', SA2_KARATE_OUT: '/mmo/karate-out', SA2_TAGS_OUT: '/mmo/tags-out', SA2_TREE_OUT: '/mmo/tree-icon', SA2_CARD_CHAO: '/mmo/card-chao', SA2_TINY_PICK_OUT: '/mmo/tiny-pick' });
  env = Object.entries(e).map(([k, v]) => `${k}=${v}`);
  if (token) keepCard(token);
  if (token) cardChao(token);
  if (token) loadSettings(token);
  if (token && q.get('stage') === '90' && roomZones[q.get('area') ?? '']) joinRoom(token, roomZones[q.get('area') ?? '']);
  if (token) setInterval(() => mmoTick(token), 100);
  status.textContent = 'compiling the game…';
  // The compressed build (game.wasm.br, sent as brotli-encoded wasm) when the
  // site has a current one; else the plain file.
  const br = await fetch('game.wasm.br').catch(() => null);
  const packed = br && br.ok && (br.headers.get('content-type') || '').startsWith('application/wasm');
  module = await WebAssembly.compileStreaming(packed ? br : fetch('game.wasm'));
  memory = new WebAssembly.Memory({ initial: PAGES_START, maximum: PAGES_MAX, shared: true });
  $('start').remove();
  canvas.focus();
  startThread(0, 0);
  requestAnimationFrame(frame);
}

$('go').addEventListener('click', () => start().catch((e) => {
  log(String(e), true);
  status.textContent = 'could not start';
}));
// ?autostart: start without the button (for testing; sound may stay off).
if (q.has('autostart')) $('go').click();

// ---- Extras settings (pause menu), saved only on the MMO server ----------
// The page applies the rows it owns: game volume, chat box, frame cap; voice
// reads mmoSettings.voiceMode. The game applies its own rows (language, sound mode...).
let gameVolume = null;
function applySettings(s) {
  const opening = s.voiceMode === 'open' && mmoSettings.voiceMode !== 'open';
  Object.assign(mmoSettings, s);
  if (opening) globalThis.mmoVoiceStart?.().catch(() => {}); // mic refused: stays silent
  if (gameVolume && Number.isInteger(mmoSettings.volume)) gameVolume.gain.value = mmoSettings.volume / 10;
  chat.hidden = mmoSettings.chat === false;
  if (renderer && mmoSettings.fps) renderer.postMessage({ fps: mmoSettings.fps });
}
async function loadSettings(token) {
  const r = await fetch('/api/settings', { headers: { Authorization: `Bearer ${token}` } }).catch(() => null);
  if (r?.ok) applySettings((await r.json()).settings || {});
  settingsLoaded = true;
}
// The game's pause Extras pages (sa2_boot::pause_extras): its rows, in its order,
// and the bytes it reads back on /dev/pad 12-15.
const THEMES = ['def', 'sn', 'sd', 'tl', 'kn', 'rg', 'am', 'eg', 'ma', 'ch', 'se'];
const EXTRAS_ROWS = [
  ['voiceLanguage', ['english', 'japanese']], ['sound', ['stereo', 'mono']], ['vibration', [true, false]],
  ['menuTheme', THEMES], ['voiceMode', ['hold', 'open']], ['chat', [true, false]],
  ['volume', [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]], ['music', [true, false]], ['fps', [60, 120, 165]],
];
function extrasChanged(row, choice) {
  const [key, choices] = EXTRAS_ROWS[row] || [];
  if (key && choice < choices.length) mmoSaveSettings({ [key]: choices[choice] });
}
let settingsLoaded = false;
function writeExtras() {
  const s = mmoSettings, bit = (on, n) => (on ? 1 << n : 0);
  pad[12] = bit(settingsLoaded, 0) | bit(s.sound === 'mono', 1) | bit(s.voiceLanguage === 'japanese', 2)
    | bit(s.vibration === false, 3) | bit(s.music === false, 4) | bit(s.chat === false, 5) | bit(s.voiceMode === 'open', 6);
  pad[13] = Number.isInteger(s.volume) ? s.volume : 10;
  pad[14] = Math.max(0, [60, 120, 165].indexOf(s.fps));
  pad[15] = Math.max(0, THEMES.indexOf(s.menuTheme));
}

// The Extras page calls this when a row changes: applied now, saved on the server.
globalThis.mmoSaveSettings = (changes) => {
  applySettings(changes);
  const token = mmoToken();
  if (token) fetch('/api/settings', { method: 'POST', body: JSON.stringify(changes),
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' } }).catch(() => {});
};

// ---- chat: world (everyone) and room (this area), relayed by the MMO server --
// T opens the box; a message starting "/w " goes to world chat, the rest to the room.
const chat = document.createElement('div');
chat.id = 'chat';
chat.innerHTML = '<div id="chat-log" aria-live="polite"></div><input id="chat-input" maxlength="200" placeholder="Room chat (/w for world)" aria-label="Chat message" hidden>';
document.body.append(chat);
const chatLog = chat.firstChild, chatInput = chat.lastChild;
// Muting adds a "Muted" line; clicking that line unmutes (the server keeps the list).
function muteClick(fromId, fromName, muted) {
  room?.ws.send(JSON.stringify({ type: 'action', action: 'mute', playerId: fromId, muted }));
  chatLine({ channel: 'system', fromName: muted ? 'Muted' : 'Unmuted', text: muted ? `${fromName} (click to unmute)` : fromName,
    onclick: muted && (() => muteClick(fromId, fromName, false)) });
}
function chatLine({ channel, fromId, fromName, text, onclick }) {
  const line = document.createElement('div');
  line.className = channel;
  if (fromId) line.title = `Click to mute ${fromName}`;
  if (fromId) line.onclick = () => muteClick(fromId, fromName, true);
  if (onclick) line.onclick = onclick;
  line.textContent = `${channel === 'world' ? '[World] ' : ''}${fromName}: ${text}`; // textContent: never HTML
  chatLog.append(line);
  while (chatLog.childElementCount > 50) chatLog.firstChild.remove();
  chatLog.scrollTop = chatLog.scrollHeight;
}
addEventListener('keydown', (e) => {
  if (e.code !== 'KeyT' || e.target === chatInput || !room || chat.hidden) return;
  e.preventDefault();
  held = 0;
  chatInput.hidden = false;
  chatInput.focus();
});
chatInput.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { chatInput.value = ''; chatInput.blur(); }
  if (e.key !== 'Enter') return;
  const world = chatInput.value.startsWith('/w ');
  const text = (world ? chatInput.value.slice(3) : chatInput.value).trim();
  if (text && room?.ws.readyState === 1) room.ws.send(JSON.stringify({ type: 'action', action: 'chat', channel: world ? 'world' : 'room', text }));
  chatInput.value = '';
  chatInput.blur();
});
chatInput.addEventListener('blur', () => (chatInput.hidden = true));

// ---- proximity voice: hold V to talk --------------------------------------
// Mic audio goes only to the MMO server (never player to player, so no one sees
// another player's address); the server relays it to Chao within its radius.
// 8 kHz 16-bit mono in 60 ms frames keeps each message under the server's 2 KB cap.
const VOICE_RATE = 8000, VOICE_FRAME = 480;
let voiceCtx = null, voiceMic = null, talking = false;
// MMO settings the game's own options menu sets (saved on the server by that menu).
// voiceMode: 'hold' (hold V to talk) or 'open' (open mic).
const mmoSettings = (globalThis.mmoSettings ??= { voiceMode: 'hold' });
const voiceAt = new Map(); // sender id -> next play time, so frames queue back to back
async function voiceStart() {
  voiceCtx ??= new AudioContext();
  if (voiceMic) return;
  const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true } });
  const src = voiceCtx.createMediaStreamSource(stream);
  const proc = voiceCtx.createScriptProcessor(2048, 1, 1);
  const step = voiceCtx.sampleRate / VOICE_RATE;
  let pending = [], phase = 0;
  proc.onaudioprocess = ({ inputBuffer }) => {
    const input = inputBuffer.getChannelData(0);
    // Open mic sends only while the player speaks (level gate), not room hiss.
    const speaking = talking || (mmoSettings.voiceMode === 'open' && Math.hypot(...input) / Math.sqrt(input.length) > 0.02);
    if (!speaking || room?.ws.readyState !== 1) { pending = []; return; }
    for (; phase < input.length; phase += step) pending.push(Math.max(-1, Math.min(1, input[phase | 0])) * 32767);
    phase -= input.length;
    while (pending.length >= VOICE_FRAME) {
      const pcm = new Int16Array(pending.splice(0, VOICE_FRAME));
      const data = btoa(String.fromCharCode(...new Uint8Array(pcm.buffer)));
      room.ws.send(JSON.stringify({ type: 'action', action: 'voice', data }));
    }
  };
  src.connect(proc);
  proc.connect(voiceCtx.createGain()).connect(voiceCtx.destination); // gain 1 on silence: keeps the processor running
  voiceMic = { stream, proc };
}
function voicePlay({ fromId, fromName, volume, data }) {
  if (!voiceCtx) return; // the server never sends voice from players you muted
  // A new voice nearby gets one clickable line, so it can be muted like chat.
  if (!voiceAt.has(fromId)) chatLine({ channel: 'system', fromId, fromName, text: 'is talking nearby (click to mute)' }); // browsers allow audio only after the first key or click
  const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
  const pcm = new Int16Array(bytes.buffer, 0, bytes.length >> 1);
  const buf = voiceCtx.createBuffer(1, pcm.length, VOICE_RATE);
  buf.getChannelData(0).set(Float32Array.from(pcm, (v) => v / 32768));
  const node = voiceCtx.createBufferSource();
  node.buffer = buf;
  const gain = voiceCtx.createGain();
  gain.gain.value = Math.max(0, Math.min(1, volume));
  node.connect(gain).connect(voiceCtx.destination);
  const at = Math.max(voiceCtx.currentTime + 0.05, voiceAt.get(fromId) ?? 0);
  node.start(at);
  voiceAt.set(fromId, at + buf.duration);
}
addEventListener('keydown', (e) => {
  if (e.code !== 'KeyV' || e.repeat || e.target instanceof HTMLInputElement || !room) return;
  talking = true;
  voiceStart().catch(() => (talking = false));
});
addEventListener('keyup', (e) => { if (e.code === 'KeyV') talking = false; });
addEventListener('blur', () => (talking = false));
for (const ev of ['keydown', 'pointerdown']) addEventListener(ev, () => { voiceCtx ??= new AudioContext(); if (mmoSettings.voiceMode === 'open') voiceStart().catch(() => {}); }, { once: true });
// The options menu calls this after switching voiceMode to 'open' (needs the mic now).
globalThis.mmoVoiceStart = () => voiceStart();
