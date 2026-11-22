// The picture, on a thread of its own: takes each frame decode.js finished
// (in this module's shared memory) and draws it on the page's canvas, so
// neither decoding nor drawing holds up the page's input.
import init, { Screen } from './pkg/sa2_web.js';

let screen, ctl, wide, fpsCap, maxFps = 0, lastDraw = 0;
let size = [1, 1];
let shown = 0, blend = false, decoded = '';
// Game frames that arrived together and were never on screen (a hitch).
let skipped = 0;
const prof = { draw: 0, pics: 0 };
const ticks = [];

function frame(now) {
  // Extras frame cap (60/120/165): skip display refreshes that come too soon.
  if (maxFps && now - lastDraw < 1000 / maxFps - 1) return requestAnimationFrame(frame);
  lastDraw = now;
  // The display's rate: blend between game frames when it is above 60.
  ticks.push(now);
  if (ticks.length > 60) ticks.shift();
  if (ticks.length === 60 && !fpsCap) blend = 59000 / (now - ticks[0]) > 70;
  Atomics.store(ctl, 8, wide ? Math.round((size[0] / Math.max(1, size[1])) * 1000) : 0);
  const n = screen.take(blend);
  shown += n;
  if (n > 1) skipped += n - 1;
  const t1 = performance.now();
  // The same shared clock as decode.js.
  screen.draw(performance.timeOrigin + now, size[0], size[1]);
  prof.draw += performance.now() - t1;
  prof.pics++;
  requestAnimationFrame(frame);
}

setInterval(() => {
  if (!screen) return;
  const per = (v, n) => (n ? v / n : 0).toFixed(1);
  postMessage({
    status: `${screen.backend()} · ${shown} game frames/s${decoded} · ${prof.pics} pictures/s (draw ${per(prof.draw, prof.pics)} ms)${blend ? ' · blended' : ''}${skipped ? ` · ${skipped} skipped` : ''}`,
  });
  prof.draw = prof.pics = 0;
  shown = skipped = 0;
}, 1000);

onmessage = async (ev) => {
  const m = ev.data;
  if (m.size) {
    size = m.size;
    return;
  }
  if (m.fps) {
    maxFps = m.fps;
    return;
  }
  try {
    ({ wide, fpsCap } = m);
    ctl = new Int32Array(m.ctl);
    size = [m.canvas.width, m.canvas.height];
    const module = await WebAssembly.compileStreaming(fetch(new URL('./pkg/sa2_web_bg.wasm', import.meta.url)));
    const wasm = await init({ module_or_path: module });
    screen = await Screen.open_offscreen(m.canvas);
    const dec = new Worker(new URL('./decode.js', import.meta.url), { type: 'module' });
    dec.onmessage = (e) => {
      if (e.data.log) postMessage({ log: e.data.log });
      else decoded = ` (decode ${e.data.ms.toFixed(1)} ms)`;
    };
    dec.postMessage({ module, memory: wasm.memory, ctl: m.ctl, slots: m.slots });
    requestAnimationFrame(frame);
  } catch (e) {
    postMessage({ log: `screen: ${e}` });
  }
};
