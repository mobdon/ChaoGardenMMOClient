// Decoding, on a thread of its own: waits for each GX job the game hands
// over (shared slots, see main.js), decodes it, and leaves the frame in the
// renderer module's shared memory for render.js to draw.
import { initSync, Decoder } from './pkg/sa2_web.js';

onmessage = (ev) => {
  const { module, memory, ctl: c, slots: s, pages: pg } = ev.data;
  const used = pg ? new Int32Array(pg) : null;
  const wasm = initSync({ module, memory });
  const ctl = new Int32Array(c), slots = s.map((x) => new Uint8Array(x));
  let dec = new Decoder();
  // A GXJD job: its pipe bytes go to the decoder's job buffer and its changed
  // pages straight into the RAM the decoder keeps, so a frame copies only
  // what changed (not all of RAM) on this side too.
  function pages(src, h) {
    const fifo = h.getUint32(8, true), ram = h.getUint32(12, true), count = h.getUint32(16, true);
    if (20 + fifo + count * 4100 > src.length) throw new Error('short GXJD job');
    const jobAt = dec.job_at(fifo) >>> 0, ramAt = dec.ram_at(ram) >>> 0;
    // Another thread may have grown the memory since these views were made.
    if (Math.max(jobAt + fifo, ramAt + ram) > wasm.memory.buffer.byteLength) wasm.memory.grow(0);
    const mem = new Uint8Array(wasm.memory.buffer);
    mem.set(src.subarray(20, 20 + fifo), jobAt);
    for (let i = 0, p = 20 + fifo; i < count; i++, p += 4100) {
      const page = h.getUint32(p, true) * 4096;
      if (page >= ram) throw new Error('GXJD page past RAM');
      mem.set(src.subarray(p + 4, p + 4 + Math.min(4096, ram - page)), ramAt + page);
    }
  }
  let slot = 0, n = 0, busy = 0, since = performance.now();
  for (;;) {
    // Sleeps here until the game fills the next slot.
    while (Atomics.load(ctl, 1 + slot) !== 1) Atomics.wait(ctl, 1 + slot, 0, 1000);
    const t0 = performance.now();
    let len = ctl[3 + slot];
    try {
      const src = slots[slot].subarray(0, len);
      const h = new DataView(src.buffer, src.byteOffset, src.length);
      // ctl[8]: the canvas's width / height x 1000 (0 keeps 4:3), from render.js.
      // A clock both workers share (each worker's performance.now() has its own start).
      const now = performance.timeOrigin + t0, aspect = Atomics.load(ctl, 8) / 1000;
      if (h.getUint32(0, true) === 0x47584a44) {
        pages(src, h);
        dec.decode_pages(now, aspect);
        // The pages this frame read, for the game to send next time (worker.js /dev/gxpages).
        if (used) {
          const n = Math.min(dec.used_len(), used.length - 1), at = dec.used_at() >>> 0;
          if (at + 4 * n > wasm.memory.buffer.byteLength) wasm.memory.grow(0);
          used.set(new Int32Array(wasm.memory.buffer, at, n), 1);
          Atomics.store(used, 0, n);
        }
      } else {
        const at = dec.job_at(len) >>> 0;
        if (at + len > wasm.memory.buffer.byteLength) wasm.memory.grow(0);
        new Uint8Array(wasm.memory.buffer, at, len).set(src);
        dec.decode(now, aspect);
      }
    } catch (e) {
      postMessage({ log: `decode: ${e}` });
      // A panic inside the decoder leaves it borrowed for good (every later
      // call fails "recursive use of an object"): start a fresh one. Its RAM
      // starts empty, so pages fill in as they change (a full job resends all).
      dec = new Decoder();
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
