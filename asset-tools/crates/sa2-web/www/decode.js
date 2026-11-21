// Decoding, on a thread of its own: waits for each GX job the game hands
// over (shared slots, see main.js), decodes it, and leaves the frame in the
// renderer module's shared memory for render.js to draw.
import { initSync, Decoder } from './pkg/sa2_web.js';

onmessage = (ev) => {
  const { module, memory, ctl: c, slots: s } = ev.data;
  const wasm = initSync({ module, memory });
  const ctl = new Int32Array(c), slots = s.map((x) => new Uint8Array(x));
  const dec = new Decoder();
  let slot = 0, n = 0, busy = 0, since = performance.now();
  for (;;) {
    // Sleeps here until the game fills the next slot.
    while (Atomics.load(ctl, 1 + slot) !== 1) Atomics.wait(ctl, 1 + slot, 0, 1000);
    const t0 = performance.now();
    const len = ctl[3 + slot];
    try {
      const at = dec.job_at(len) >>> 0;
      // Another thread may have grown the memory since this view was made.
      if (at + len > wasm.memory.buffer.byteLength) wasm.memory.grow(0);
      new Uint8Array(wasm.memory.buffer, at, len).set(slots[slot].subarray(0, len));
      // ctl[8]: the canvas's width / height x 1000 (0 keeps 4:3), from render.js.
      // A clock both workers share (each worker's performance.now() has its own start).
      dec.decode(performance.timeOrigin + t0, Atomics.load(ctl, 8) / 1000);
    } catch (e) {
      postMessage({ log: `decode: ${e}` });
    }
    Atomics.store(ctl, 1 + slot, 0);
    Atomics.notify(ctl, 1 + slot);
    slot ^= 1;
    n++;
    busy += performance.now() - t0;
    if (performance.now() - since >= 1000) {
      postMessage({ decoded: n, ms: n ? busy / n : 0 });
      n = busy = 0;
      since = performance.now();
    }
  }
};
