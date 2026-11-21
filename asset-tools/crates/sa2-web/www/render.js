// The picture, on a thread of its own: decodes each finished GX job the game
// hands over (shared slots, see main.js) and draws it on the page's canvas,
// so decoding never holds up the page's input or the display's refresh.
import init, { Screen } from './pkg/sa2_web.js';

let wasm, screen, ctl, slots, wide, fpsCap;
let size = [1, 1], readSlot = 0;
let shown = 0, blend = false;
const prof = { submit: 0, draw: 0, pics: 0 };
const ticks = [];

function frame(now) {
  // The display's rate: blend between game frames when it is above 60.
  ticks.push(now);
  if (ticks.length > 60) ticks.shift();
  if (ticks.length === 60 && !fpsCap) blend = 59000 / (now - ticks[0]) > 70;
  // Every finished frame (the game never waits on the display's rate).
  while (Atomics.load(ctl, 1 + readSlot) === 1) {
    const len = ctl[3 + readSlot];
    try {
      const t0 = performance.now();
      const at = screen.job_at(len) >>> 0;
      new Uint8Array(wasm.memory.buffer, at, len).set(slots[readSlot].subarray(0, len));
      screen.submit(now, wide, blend);
      prof.submit += performance.now() - t0;
    } catch (e) {
      postMessage({ log: `frame: ${e}` });
    }
    Atomics.store(ctl, 1 + readSlot, 0);
    Atomics.notify(ctl, 1 + readSlot);
    readSlot ^= 1;
    shown++;
  }
  const t1 = performance.now();
  screen.draw(now, size[0], size[1]);
  prof.draw += performance.now() - t1;
  prof.pics++;
  requestAnimationFrame(frame);
}

setInterval(() => {
  if (!screen) return;
  const per = (v, n) => (n ? v / n : 0).toFixed(1);
  postMessage({
    status: `${screen.backend()} · ${shown} game frames/s (decode ${per(prof.submit, shown)} ms) · ${prof.pics} pictures/s (draw ${per(prof.draw, prof.pics)} ms)${blend ? ' · blended' : ''}`,
  });
  prof.submit = prof.draw = prof.pics = 0;
  shown = 0;
}, 1000);

onmessage = async (ev) => {
  const m = ev.data;
  if (m.size) {
    size = m.size;
    return;
  }
  try {
    ({ wide, fpsCap } = m);
    ctl = new Int32Array(m.ctl);
    slots = m.slots.map((s) => new Uint8Array(s));
    size = [m.canvas.width, m.canvas.height];
    wasm = await init();
    screen = await Screen.open_offscreen(m.canvas);
    postMessage({ ready: true });
    requestAnimationFrame(frame);
  } catch (e) {
    postMessage({ log: `screen: ${e}` });
  }
};
