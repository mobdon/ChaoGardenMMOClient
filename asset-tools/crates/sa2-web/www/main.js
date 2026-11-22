// The page: starts the game's threads (workers on one shared memory), sends
// them the keyboard and gamepad, plays their sound, and draws each finished
// frame they hand over with the renderer (render.js, a worker of its own).
//
// Shared with the workers (ctl, Int32): [0] last thread id, [1..2] slot full,
// [3..4] slot length, [5] slot the game fills next, [6] sound written,
// [7] sound read, [8] canvas aspect (render.js), [9] sound dropouts, [10] sound skips.

const SLOT = 64 << 20; // a frame's pipe bytes and the game RAM (SA2_MEM1_MB 48 fits; more needs a bigger slot)
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
  pad: new SharedArrayBuffer(16),
  egg: new SharedArrayBuffer(4),
  audio: new SharedArrayBuffer(AUDIO_RING * 2),
  slots: [new SharedArrayBuffer(SLOT), new SharedArrayBuffer(SLOT)],
  card: new SharedArrayBuffer(64), // the server card's revision and lease (worker.js)
  mmo: new SharedArrayBuffer(67864 + 32 * 2064), // the MMO files (worker.js, MMO)
  // The RAM pages the decoder read for its last frame: [0] word count, then bits (decode.js, worker.js /dev/gxpages).
  pages: new SharedArrayBuffer(4 + 65536),
};
const ctl = new Int32Array(sh.ctl), pad = new Uint8Array(sh.pad);
// A named egg hatches in the Chao Lobby (sa2_boot::start_stage reads /dev/egg).
window.sa2HatchEgg = () => Atomics.add(new Int32Array(sh.egg), 0, 1);

// ---- input --------------------------------------------------------------
// Bits as the game reads them (sa2-web-game KEYS): A D S W, Space, E,
// Left Shift, Q, R, Enter, Escape, 1, 3, then left and right mouse.
const KEYS = ['KeyA', 'KeyD', 'KeyS', 'KeyW', 'Space', 'KeyE', 'ShiftLeft', 'KeyQ', 'KeyR', 'Enter', 'Escape', 'Digit1', 'Digit3'];
let held = 0;
function hold(bit, down) {
  held = down ? held | bit : held & ~bit;
}
addEventListener('keydown', (e) => {
  const i = KEYS.indexOf(e.code);
  if (i < 0 || e.ctrlKey || e.altKey || e.metaKey) return;
  e.preventDefault();
  hold(1 << i, true);
});
addEventListener('keyup', (e) => {
  const i = KEYS.indexOf(e.code);
  if (i >= 0) hold(1 << i, false);
});
addEventListener('blur', () => (held = 0));
canvas.addEventListener('mousedown', (e) => e.button === 0 ? hold(1 << 13, true) : e.button === 2 && hold(1 << 14, true));
addEventListener('mouseup', (e) => e.button === 0 ? hold(1 << 13, false) : e.button === 2 && hold(1 << 14, false));
canvas.addEventListener('contextmenu', (e) => e.preventDefault());
canvas.addEventListener('dblclick', () => document.fullscreenElement ? document.exitFullscreen() : canvas.requestFullscreen());

// The first standard-layout gamepad, mapped as the desktop build maps it:
// A/Cross = A, X/Square = B, B/Circle = X, Y/Triangle = Y, Start, RB = Z,
// LT/RT = L/R (analog), d-pad.
const BUTTONS = [[0, 0x0100], [2, 0x0200], [1, 0x0400], [3, 0x0800], [9, 0x1000], [5, 0x0010],
  [14, 0x0001], [15, 0x0002], [13, 0x0004], [12, 0x0008]];
function writePad() {
  pad[0] = held & 0xff;
  pad[1] = held >> 8;
  const g = [...(navigator.getGamepads ? navigator.getGamepads() : [])].find((p) => p && p.connected);
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
    else if (m.area !== undefined) roomArea(m.area);
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
// In a Chao World area from a hub portal, stay in that zone's shared room on
// the MMO server (same socket and messages as the hub page, src/main.js).
// The game's own Chao Lobby is the MMO hub (mob 2026-11-21): stage 90 with no area.
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
    if (m.type === 'snapshot') { showCard(m.players.find(p => p.id === m.selfId)); overlay?.snapshot(m); othersIn(m, token); }
    if (m.type === 'event') overlay?.event(m);
  };
  // 4001: this card's Chao went out somewhere else; don't fight over it.
  ws.onclose = ({ code }) => { if (code !== 4001) setTimeout(() => joinRoom(token, room.zone), 3000); };
}
// ---- the other players' Chao (sa2-boot chao_player.rs, worker.js MMO files) --
// The game's own pose goes to the room each 100 ms; the room's other players
// with a pose come back as lines "id x y z angle motion frame gear"; Chao
// records go to and from the server by version. The server routes may not
// exist yet: failures stay quiet.
const MMO = { POSE_OUT: 0, REC_OUT: 264, REC: 2048, POSE_IN: 2320, POSE_IN_MAX: 65536, SLOTS: 67864, SLOT: 2064, NSLOT: 32 };
const mmoBytes = new Uint8Array(sh.mmo);
globalThis.sa2Mmo = sh.mmo; // for checks from the console
const mmoHead = (at) => new Int32Array(sh.mmo, at, 2);
// The game keys remote Chao by a number: the first 52 bits of the player id.
const numId = (id) => String(BigInt('0x' + (String(id).replace(/[^0-9a-f]/gi, '').slice(0, 13) || '0')));
let poseSeen = 0, recSeen = 0;
const recVersions = new Map();
function mmoTick(token) {
  const pose = mmoHead(MMO.POSE_OUT), seq = Atomics.load(pose, 0);
  if (seq !== poseSeen && room?.ws.readyState === 1) {
    poseSeen = seq;
    try {
      const text = new TextDecoder().decode(mmoBytes.slice(MMO.POSE_OUT + 8, MMO.POSE_OUT + 8 + pose[1]));
      room.ws.send(JSON.stringify({ type: 'action', action: 'chao-pose', pose: JSON.parse(text) }));
    } catch {}
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
  if (globalThis.sa2Gain) globalThis.sa2Gain.gain.value = on ? 0 : 1;
}

// The game changed Chao World area (its own teleporters): follow it into that room.
function roomArea(area) {
  const zone = roomZones[area === 0 ? '' : area];
  if (!room || !zone || zone === room.zone) return;
  room.zone = zone;
  if (room.ws.readyState === 1) room.ws.send(JSON.stringify({ type: 'action', action: 'chao-world', area: zone }));
}
// The MMO player card over the game (name, Chao select icon, rings); it never takes focus.
function showCard(player) {
  if (!player) return;
  const card = document.getElementById('mmo-card');
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
  picked = pickedFiles();
  status.textContent = 'loading…';
  if (!crossOriginIsolated) throw new Error('the page needs cross-origin isolation (open it from the local server)');
  const audio = new AudioContext({ latencyHint: 'interactive' });
  await audio.audioWorklet.addModule('audio.js');
  const mix = new AudioWorkletNode(audio, 'sa2-mix', { outputChannelCount: [2], processorOptions: { ctl: sh.ctl, audio: sh.audio } });
  // Master volume for the MMO sound corner (src/game-overlay.js).
  globalThis.sa2Gain = audio.createGain();
  mix.connect(globalThis.sa2Gain).connect(audio.destination);
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
  // A new player (no Chao on the card yet): their egg is laid in the lobby.
  if (q.get('hatch') === '1') e.SA2_HATCH_EGG = '1';
  const token = mmoToken();
  if (token) e.SA2_MMO_TOKEN = token;
  if (token) Object.assign(e, { SA2_POSE_OUT: '/mmo/pose-out', SA2_POSE_IN: '/mmo/pose-in', SA2_RECORD_OUT: '/mmo/record-out', SA2_RECORD_IN: '/mmo/rec' });
  env = Object.entries(e).map(([k, v]) => `${k}=${v}`);
  if (token) keepCard(token);
  if (token && q.get('stage') === '90' && roomZones[q.get('area') ?? '']) joinRoom(token, roomZones[q.get('area') ?? '']);
  if (token) setInterval(() => mmoTick(token), 100);
  status.textContent = 'compiling the game…';
  module = await WebAssembly.compileStreaming(fetch('game.wasm'));
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
