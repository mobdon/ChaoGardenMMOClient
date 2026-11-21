// The game's mix (16-bit stereo at 32 kHz, in a ring the game threads fill)
// to the speakers, stepped to the device's rate with a linear blend, as the
// desktop build does it.
'use strict';

const RATE = 32000;

class Mix extends AudioWorkletProcessor {
  constructor(o) {
    super();
    const { ctl, audio } = o.processorOptions;
    this.ctl = new Int32Array(ctl);
    this.ring = new Int16Array(audio);
    this.step = RATE / sampleRate;
    this.pos = 0;
    this.prev = [0, 0];
    this.next = [0, 0];
  }

  process(_inputs, outputs) {
    const out = outputs[0], c = this.ctl, ring = this.ring, cap = ring.length;
    const w = Atomics.load(c, 6);
    let r = Atomics.load(c, 7);
    // Behind by more than half the ring: skip to a quarter second back.
    if (((w - r) >>> 0) > cap / 2) r = (w - (cap >> 2)) >>> 0;
    for (let i = 0; i < out[0].length; i++) {
      this.pos += this.step;
      while (this.pos >= 1) {
        this.pos -= 1;
        this.prev = this.next;
        if (((w - r) >>> 0) >= 2) {
          this.next = [ring[r % cap], ring[(r + 1) % cap]];
          r = (r + 2) >>> 0;
        }
      }
      for (let ch = 0; ch < out.length; ch++) {
        const k = Math.min(ch, 1), a = this.prev[k], b = this.next[k];
        out[ch][i] = (a + (b - a) * this.pos) / 32768;
      }
    }
    Atomics.store(c, 7, r);
    return true;
  }
}

registerProcessor('sa2-mix', Mix);
