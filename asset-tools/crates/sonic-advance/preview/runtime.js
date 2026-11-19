// Presentation, local file I/O and input only. Rust owns source verification,
// native decoding, simulation, camera, rings, collision and the complete frame.
const WIDTH = 240;
const HEIGHT = 160;
const ROM_LENGTH = 8 * 1024 * 1024;
const FRAME_MS = 1000 / 60;
const MAX_FRAME_BATCH = 4;
const canvas = document.querySelector('#stage');
const context = canvas.getContext('2d', { alpha: false });
const image = new ImageData(WIDTH, HEIGHT);
const fileInput = document.querySelector('#rom-file');
const actSelect = document.querySelector('#practice-act');
const pauseButton = document.querySelector('#pause');
const resetButton = document.querySelector('#reset');
const status = document.querySelector('#status');
const runState = document.querySelector('#run-state');
const emptyState = document.querySelector('#empty-state');
const decoder = new TextDecoder();
const held = new Set();
const acceptedKeys = new Set(['ArrowLeft', 'ArrowRight', 'ArrowDown', 'ArrowUp', 'Enter', 'KeyZ', 'Space']);
let api;
let loaded = false;
let atTitle = false;
let running = false;
let busy = false;
let lastTime = null;
let accumulator = 0;
let lastSnapshotAt = 0;
let selection = 0;

function message(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function controls() {
  pauseButton.disabled = !loaded || busy;
  resetButton.disabled = !loaded || busy;
  actSelect.disabled = !loaded || busy;
  fileInput.disabled = !api || busy;
  pauseButton.textContent = running ? 'Pause' : 'Resume';
  runState.textContent = busy ? 'Loading local source' : loaded ? (running ? 'Running · 60 Hz' : 'Paused') : 'No ROM loaded';
}

function pause(text) {
  running = false;
  held.clear();
  lastTime = null;
  accumulator = 0;
  controls();
  if (loaded && api && !busy) {
    try { showSnapshot(); } catch { /* Preserve the originating pause/error message. */ }
  }
  if (text && loaded) message(text);
}

function wasmError() {
  const pointer = api.asoe_error_ptr();
  const length = api.asoe_error_len();
  if (length > 8192) return 'The Rust runtime returned an oversized error.';
  // Reacquire memory after every ABI call. Previous views may be detached by
  // memory growth; no Rust call runs while this fresh view is being consumed.
  return decoder.decode(new Uint8Array(api.memory.buffer, pointer, length)) || 'The Rust runtime rejected the operation.';
}

function check(result) {
  if (result !== 0) throw new Error(wasmError());
}

function draw() {
  const pointer = api.asoe_frame_ptr();
  const length = api.asoe_frame_len();
  if (length !== WIDTH * HEIGHT * 4) throw new Error('The native frame dimensions do not match this adapter.');
  image.data.set(new Uint8ClampedArray(api.memory.buffer, pointer, length));
  context.putImageData(image, 0, 0);
}

function showSnapshot() {
  const pointer = api.asoe_snapshot_ptr();
  if (pointer === 0) throw new Error(wasmError());
  const length = api.asoe_snapshot_len();
  if (length > 8192) throw new Error('The Rust snapshot exceeds the adapter limit.');
  const snapshot = JSON.parse(decoder.decode(new Uint8Array(api.memory.buffer, pointer, length)));
  atTitle = snapshot.scene === 'title' || snapshot.scene === 'mainMenu';
  if (snapshot.scene === 'mainMenu') {
    const m = snapshot.menu;
    const names = ['Single Player', 'Multiplayer', 'Time Attack', 'Options', 'Tiny Chao Garden'];
    actSelect.value = 'title';
    document.querySelector('#stage-heading').textContent = 'Main menu';
    document.querySelector('#frame-value').textContent = String(m.frame);
    document.querySelector('#action-value').textContent = names[m.selected];
    if (m.destination) {
      running = false;
      held.clear();
      controls();
      message(names[m.selected] + ' selected. This destination is not connected yet. Choose a practice act or reset to the title.');
    } else if (!m.fading) {
      message('Up and Down select. Z or Space confirms.');
    }
    return;
  }
  if (atTitle) {
    const t = snapshot.title;
    actSelect.value = 'title';
    document.querySelector('#stage-heading').textContent = 'Title screen';
    document.querySelector('#frame-value').textContent = String(t.frame);
    document.querySelector('#position-value').textContent = '—';
    document.querySelector('#rings-value').textContent = '—';
    document.querySelector('#source-value').textContent = t.sourceSha256?.slice(0, 12) + '… verified';
    const action = document.querySelector('#action-value');
    action.textContent = t.startEnabled ? 'Press Start' : t.phase;
    if (t.phase === 'demoRequested') {
      running = false;
      held.clear();
      controls();
      message('The native title requested its demo. Choose a practice act or reset the title.');
    }
    return;
  }
  actSelect.value = String(snapshot.stageIndex);
  document.querySelector('#stage-heading').textContent = 'Neo Green Hill · Act ' + (snapshot.stageIndex + 1);
  document.querySelector('#frame-value').textContent = String(snapshot.frame);
  document.querySelector('#position-value').textContent = `${(snapshot.xQ8 / 256).toFixed(1)}, ${(snapshot.yQ8 / 256).toFixed(1)}`;
  document.querySelector('#rings-value').textContent = Number.isSafeInteger(snapshot.localRings) ? String(snapshot.localRings) : '—';
  document.querySelector('#action-value').textContent = snapshot.animation === 18 ? 'Charging' : snapshot.animation === 8 && snapshot.grounded ? 'Rolling' : snapshot.animation === 3 ? 'Crouching' : !snapshot.grounded ? 'Airborne' : snapshot.vxQ8 ? 'Moving' : 'Standing';
  document.querySelector('#source-value').textContent = typeof snapshot.sourceSha256 === 'string' ? `${snapshot.sourceSha256.slice(0, 12)}… verified` : 'Rust verified ASOE';
}

function fail(error) {
  pause();
  message(error instanceof Error ? error.message : String(error), true);
}

async function startEngine() {
  try {
    const response = await fetch('/runtime.wasm', { cache: 'no-store', credentials: 'omit' });
    if (!response.ok) throw new Error('Build the local Rust/WASM runtime, then reload this page.');
    const bytes = await response.arrayBuffer();
    if (bytes.byteLength > 16 * 1024 * 1024) throw new Error('The local WASM module exceeds the adapter limit.');
    const { instance } = await WebAssembly.instantiate(bytes, {});
    api = instance.exports;
    for (const name of ['asoe_allocate_rom', 'asoe_load_title', 'asoe_open_title', 'asoe_step', 'asoe_reset', 'asoe_select_act', 'asoe_frame_ptr', 'asoe_frame_len', 'asoe_snapshot_ptr', 'asoe_snapshot_len', 'asoe_error_ptr', 'asoe_error_len']) {
      if (typeof api[name] !== 'function') throw new Error(`Missing Rust runtime export: ${name}`);
    }
    if (!(api.memory instanceof WebAssembly.Memory)) throw new Error('The Rust runtime did not export its memory.');
    message('Choose the local 8 MiB ASOE ROM. Its contents remain in this browser’s memory.');
    controls();
  } catch (error) {
    api = undefined;
    fail(error);
  }
}

fileInput.addEventListener('change', async () => {
  const file = fileInput.files?.[0];
  if (!file || !api) return;
  const currentSelection = ++selection;
  pause();
  busy = true;
  controls();
  message('Reading and verifying the local source in Rust…');
  try {
    if (file.size !== ROM_LENGTH) throw new Error('Choose a raw 8 MiB ASOE revision-0 .gba file. ZIP archives and other versions are unsupported.');
    const bytes = await file.arrayBuffer();
    if (currentSelection !== selection) return;
    const pointer = api.asoe_allocate_rom(bytes.byteLength);
    if (pointer === 0) throw new Error(wasmError());
    new Uint8Array(api.memory.buffer, pointer, bytes.byteLength).set(new Uint8Array(bytes));
    // load takes ownership; the former input view is never reused.
    check(api.asoe_load_title(bytes.byteLength));
    loaded = true;
    emptyState.hidden = true;
    draw();
    showSnapshot();
    running = !document.hidden && document.hasFocus();
    lastTime = null;
    accumulator = 0;
    message(running ? 'ASOE verified. Enter presses Start at the title. You can also choose a practice act.' : 'ASOE verified. Choose Resume when ready.');
    canvas.focus({ preventScroll: true });
  } catch (error) {
    // Rust preserves an existing valid runtime if replacement verification
    // fails. Retain it paused, including its real source and state snapshot.
    emptyState.hidden = loaded;
    if (loaded) {
      try { draw(); showSnapshot(); } catch { /* Keep the original error below. */ }
    }
    fail(error);
  } finally {
    busy = false;
    fileInput.value = '';
    controls();
  }
});

pauseButton.addEventListener('click', () => {
  if (!loaded || busy) return;
  if (running) {
    pause('Paused. Choose Resume to continue.');
  } else {
    held.clear();
    accumulator = 0;
    lastTime = null;
    running = true;
    controls();
    message(atTitle ? 'Enter presses Start once the title is ready.' : 'Use the arrows to move; Z or Space jumps.');
    canvas.focus({ preventScroll: true });
  }
});

actSelect.addEventListener('change', () => {
  if (!loaded || busy) return;
  const selected = actSelect.value;
  pause();
  try {
    check(selected === 'title' ? api.asoe_open_title() : api.asoe_select_act(Number(selected)));
    draw();
    showSnapshot();
    message(selected === 'title' ? 'Title screen loaded. Choose Resume.' : 'Practice act loaded. Choose Resume to play.');
  } catch (error) { fail(error); }
});

resetButton.addEventListener('click', () => {
  if (!loaded || busy) return;
  held.clear();
  accumulator = 0;
  lastTime = null;
  try {
    check(api.asoe_reset());
    draw();
    showSnapshot();
    message(running ? 'Stage reset. Continue with the arrow keys and Z or Space.' : 'Stage reset. Choose Resume to continue.');
    canvas.focus({ preventScroll: true });
  } catch (error) { fail(error); }
});

window.addEventListener('keydown', event => {
  if (!acceptedKeys.has(event.code) || !loaded || !running || busy || event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.target instanceof Element && event.target.closest('input, button, a, textarea, select')) return;
  event.preventDefault();
  held.add(event.code);
});
window.addEventListener('keyup', event => {
  if (!acceptedKeys.has(event.code)) return;
  held.delete(event.code);
  if (event.target instanceof Element && event.target.closest('input, button, a, textarea, select')) return;
  if (loaded && running) event.preventDefault();
});
window.addEventListener('blur', () => pause('Paused when the window lost focus. Choose Resume to continue.'));
window.addEventListener('pagehide', () => pause());
document.addEventListener('visibilitychange', () => { if (document.hidden) pause('Paused while this tab was hidden. Choose Resume to continue.'); });

function animate(now) {
  if (running && loaded && !busy) {
    const elapsed = lastTime === null ? 0 : Math.max(0, Math.min(now - lastTime, FRAME_MS * MAX_FRAME_BATCH));
    lastTime = now;
    accumulator = Math.min(accumulator + elapsed, FRAME_MS * MAX_FRAME_BATCH);
    const frames = Math.min(MAX_FRAME_BATCH, Math.floor(accumulator / FRAME_MS));
    if (frames > 0) {
      accumulator -= frames * FRAME_MS;
      const mask = atTitle ? ((held.has('Enter') ? 8 : 0) | (held.has('ArrowUp') ? 64 : 0) | (held.has('ArrowDown') ? 128 : 0) | (held.has('KeyZ') || held.has('Space') ? 1 : 0)) : (held.has('ArrowLeft') ? 0x20 : 0) | (held.has('ArrowRight') ? 0x10 : 0) | (held.has('ArrowDown') ? 0x80 : 0) | (held.has('KeyZ') || held.has('Space') ? 1 : 0);
      try {
        check(api.asoe_step(mask, frames));
        draw();
        if (now - lastSnapshotAt >= 250) { showSnapshot(); lastSnapshotAt = now; }
      } catch (error) { fail(error); }
    }
  } else {
    lastTime = null;
  }
  requestAnimationFrame(animate);
}

controls();
void startEngine();
requestAnimationFrame(animate);
