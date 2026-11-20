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
const keyBits = new Map([['ArrowRight', 16], ['ArrowLeft', 32], ['ArrowDown', 128], ['KeyZ', 1], ['Space', 1]]);
const soundToggle = document.getElementById('sound');
const audition = document.getElementById('audition');
let audio, stageMusic = null, soundEpoch = 0;
const audioCache = new Map(), sources = new Set();
let supportedCues = new Set();
function stopAudio() {
  soundEpoch++;
  for (const source of sources) source.stop();
  sources.clear();
}
async function playCue(id, pan = 0, volume = 1) {
  if (!audio || !supportedCues.has(id)) return;
  const epoch = soundEpoch;
  if (!audioCache.has(id)) audioCache.set(id, fetch('/audio/' + id + '.wav').then(r => {
    if (!r.ok) throw new Error('Sound preview unavailable: ' + id);
    return r.arrayBuffer();
  }).then(b => audio.decodeAudioData(b)));
  let buffer;
  try { buffer = await audioCache.get(id); }
  catch (error) { audioCache.delete(id); status.textContent = error.message; return; }
  if (epoch !== soundEpoch || sources.size >= 16) return;
  const source = audio.createBufferSource();
  source.buffer = buffer;
  const panner = audio.createStereoPanner();
  panner.pan.value = Math.max(-1, Math.min(1, pan / 64));
  const gain = audio.createGain();
  gain.gain.value = volume;
  source.connect(panner).connect(gain).connect(audio.destination);
  source.onended = () => { sources.delete(source); source.disconnect(); panner.disconnect(); gain.disconnect(); };
  sources.add(source);
  source.start();
}
async function enableAudio() {
  audio ||= new AudioContext();
  await audio.resume();
}
soundToggle.addEventListener('change', () => { if (soundToggle.checked) enableAudio().catch(e => { soundToggle.checked = false; status.textContent = e.message; }); else stopAudio(); });
audition.addEventListener('click', () => { stopAudio(); enableAudio().then(() => playCue(stageMusic)).catch(e => { status.textContent = e.message; }); });
let running = false;
let busy = true;
let halted = false;
let pendingReset = false;
toggle.disabled = reset.disabled = true;

function draw(buffer, audible) {
  const bytes = new Uint8Array(buffer);
  if (bytes.length < 8 || String.fromCharCode(...bytes.slice(0, 4)) !== 'A2NF') throw new Error('Invalid local frame');
  const length = new DataView(buffer).getUint32(4, true);
  if (length > 65536 || 8 + length > bytes.length) throw new Error('Invalid local frame metadata');
  const frame = JSON.parse(new TextDecoder().decode(bytes.subarray(8, 8 + length)));
  if (frame.width !== 240 || frame.height !== 160 || bytes.length !== 8 + length + 240 * 160 * 4) throw new Error('Invalid native viewport');
  context.putImageData(new ImageData(new Uint8ClampedArray(buffer, 8 + length), 240, 160), 0, 0);
  const state = frame.state;
  stageMusic = frame.stage_music;
  supportedCues = new Set(frame.sound_cues);
  audition.disabled = !supportedCues.has(stageMusic);
  if (audible && running && soundToggle.checked) {
    for (const id of state.animation_sound_ids) playCue(id);
    for (const pan of state.ring_sound_pans) playCue(117, pan, 0.5);
  }
  halted = Boolean(state.halted || state.game_over);
  const tally = state.results ? ` · Time bonus ${state.results.timeBonus} · Ring bonus ${state.results.ringBonus} · Score ${state.score}` : '';
  status.textContent = state.halted || (state.game_over ? `Game over: ${state.game_over}` : null) || `Zone ${1 + (state.level_id >> 2)} Act ${1 + (state.level_id % 4)} · Rings ${state.rings} · Frame ${state.frame} · (${(state.player.q_world_x / 256).toFixed(2)}, ${(state.player.q_world_y / 256).toFixed(2)}) · ${state.goal_phase || state.callback}${tally}`;
  detail.textContent = JSON.stringify(state, null, 2);
  if (halted) setRunning(false);
}

async function exchange(path, body) {
  const response = await fetch(path, body === undefined ? { cache: 'no-store' } : {
    method: 'POST', headers: { 'Content-Type': 'application/x-a2ne-intent' }, body: String(body), cache: 'no-store'
  });
  if (!response.ok) throw new Error(await response.text());
  draw(await response.arrayBuffer(), path === '/tick');
}

function setRunning(value) {
  running = value && !halted;
  toggle.textContent = running ? 'Pause' : 'Run';
  if (!running) { pressed.clear(); stopAudio(); }
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
