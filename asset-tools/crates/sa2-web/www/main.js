// The page: starts the game's threads (workers on one shared memory), sends
// them the keyboard and gamepad, plays their sound, and draws each finished
// frame they hand over with the renderer (render.js, a worker of its own).
//
// Shared with the workers (ctl, Int32): [0] last thread id, [1..2] slot full,
// [3..4] slot length, [5] slot the game fills next, [6] sound written,
// [7] sound read.

const SLOT = 40 << 20; // a frame's pipe bytes and 24 MiB of RAM
const AUDIO_RING = 1 << 15; // samples (a power of two), half a second
const PAGES_START = 1024, PAGES_MAX = 16384; // 64 MiB to 1 GiB, as the game build links (browsers commit the maximum)
const q = new URLSearchParams(location.search);
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
  audio: new SharedArrayBuffer(AUDIO_RING * 2),
  slots: [new SharedArrayBuffer(SLOT), new SharedArrayBuffer(SLOT)],
};
const ctl = new Int32Array(sh.ctl), pad = new Uint8Array(sh.pad);

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

// ---- threads ------------------------------------------------------------
let module, memory, env, workers = 0;
function startThread(tid, arg) {
  const w = new Worker('worker.js');
  workers++;
  w.onmessage = (ev) => {
    const m = ev.data;
    if (m.spawn !== undefined) startThread(m.spawn, m.arg);
    else if (m.log !== undefined) log(m.log, /panicked|can't|error|stopped/i.test(m.log));
    else if (m.crash) log(`game thread ${m.tid} stopped: ${m.crash}`, true);
    else if (m.exit !== undefined) log(`the game ended (exit ${m.exit})`, true);
    else if (m.done) workers--;
  };
  w.onerror = (e) => log(`game thread ${tid}: ${e.message}`, true);
  w.postMessage({ module, memory, tid, arg, env, args: ['sa2-host', '/research/main.dol', '/research/symbols.txt'], ...sh });
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
  status.textContent = `${picture} · ${workers} threads`;
}, 1000);

async function start() {
  $('go').disabled = true;
  status.textContent = 'loading…';
  if (!crossOriginIsolated) throw new Error('the page needs cross-origin isolation (open it from the local server)');
  const audio = new AudioContext({ latencyHint: 'interactive' });
  await audio.audioWorklet.addModule('audio.js');
  const mix = new AudioWorkletNode(audio, 'sa2-mix', { outputChannelCount: [2], processorOptions: { ctl: sh.ctl, audio: sh.audio } });
  mix.connect(audio.destination);
  const dpr = devicePixelRatio || 1;
  canvas.width = Math.round(canvas.clientWidth * dpr);
  canvas.height = Math.round(canvas.clientHeight * dpr);
  const off = canvas.transferControlToOffscreen();
  renderer = new Worker('render.js', { type: 'module' });
  renderer.onmessage = (ev) => {
    if (ev.data.status) picture = ev.data.status;
    else if (ev.data.log) log(ev.data.log, true);
  };
  renderer.postMessage({ canvas: off, ctl: sh.ctl, slots: sh.slots, wide, fpsCap }, [off]);
  env = Object.entries(await (await fetch('env.json')).json()).map(([k, v]) => `${k}=${v}`);
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
