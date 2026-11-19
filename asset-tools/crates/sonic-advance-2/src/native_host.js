'use strict';
// Presentation and physical-key binding only. Every simulated frame, movement
// decision, collision response and animation command executes in Rust.
const canvas = document.getElementById('game');
const context = canvas.getContext('2d', { alpha: false });
const status = document.getElementById('status');
const detail = document.getElementById('state');
const toggle = document.getElementById('toggle');
const reset = document.getElementById('reset');
const pressed = new Set();
const keyBits = new Map([['ArrowRight', 16], ['ArrowLeft', 32], ['KeyZ', 1], ['Space', 1]]);
let running = false;
let busy = true;
let halted = false;
let pendingReset = false;
toggle.disabled = reset.disabled = true;

function draw(buffer) {
  const bytes = new Uint8Array(buffer);
  if (bytes.length < 8 || String.fromCharCode(...bytes.slice(0, 4)) !== 'A2NF') throw new Error('Invalid local frame');
  const length = new DataView(buffer).getUint32(4, true);
  if (length > 65536 || 8 + length > bytes.length) throw new Error('Invalid local frame metadata');
  const frame = JSON.parse(new TextDecoder().decode(bytes.subarray(8, 8 + length)));
  if (frame.width !== 240 || frame.height !== 160 || bytes.length !== 8 + length + 240 * 160 * 4) throw new Error('Invalid native viewport');
  context.putImageData(new ImageData(new Uint8ClampedArray(buffer, 8 + length), 240, 160), 0, 0);
  const state = frame.state;
  halted = Boolean(state.halted);
  status.textContent = state.halted || `Frame ${state.frame} · (${(state.player.q_world_x / 256).toFixed(2)}, ${(state.player.q_world_y / 256).toFixed(2)}) · ${state.callback}`;
  detail.textContent = JSON.stringify(state, null, 2);
  if (halted) setRunning(false);
}

async function exchange(path, body) {
  const response = await fetch(path, body === undefined ? { cache: 'no-store' } : {
    method: 'POST', headers: { 'Content-Type': 'application/x-a2ne-intent' }, body: String(body), cache: 'no-store'
  });
  if (!response.ok) throw new Error(await response.text());
  draw(await response.arrayBuffer());
}

function setRunning(value) {
  running = value && !halted;
  toggle.textContent = running ? 'Pause' : 'Run';
  if (!running) pressed.clear();
}

// Pacing is a local inspection-host choice. No client clock or claimed
// progression is submitted. Rust advances one fixed native frame per request.
async function tick() {
  const start = performance.now();
  if (pendingReset && !busy) await resetGame();
  else if (running && !busy) {
    busy = true;
    try {
      let bits = 0;
      for (const code of pressed) bits |= keyBits.get(code) || 0;
      await exchange('/tick', bits);
    } catch (error) { status.textContent = error.message; setRunning(false); }
    finally { busy = false; }
  }
  setTimeout(tick, Math.max(0, 1000 / 60 - (performance.now() - start)));
}

toggle.addEventListener('click', () => { setRunning(!running); canvas.focus(); });
async function resetGame() {
  if (busy) return;
  busy = true; pendingReset = false;
  toggle.disabled = reset.disabled = true;
  try { await exchange('/reset', ''); halted = false; canvas.focus(); }
  catch (error) { status.textContent = error.message; }
  finally { busy = false; toggle.disabled = reset.disabled = false; }
}
reset.addEventListener('click', () => {
  setRunning(false); pendingReset = true;
  if (!busy) resetGame();
});
window.addEventListener('keydown', event => {
  if (keyBits.has(event.code) && (event.target === canvas || running)) { event.preventDefault(); pressed.add(event.code); }
});
window.addEventListener('keyup', event => { pressed.delete(event.code); });
window.addEventListener('blur', () => { pressed.clear(); setRunning(false); });
document.addEventListener('visibilitychange', () => { if (document.hidden) setRunning(false); });
exchange('/frame').then(() => { busy = false; toggle.disabled = reset.disabled = false; tick(); }).catch(error => { status.textContent = error.message; });
