// The game's mix (16-bit stereo at 32 kHz, in a ring the game threads fill)
// to the speakers, stepped to the device's rate with a linear blend, as the
// desktop build does it.
'use strict';

const RATE = 32000;
// Samples to keep waiting (64 a millisecond): play a touch faster above this,
// slower below, so a game a little off 60 neither runs dry nor piles up.
const TARGET = 60 * 64;

class Mix extends AudioWorkletProcessor {
  constructor(o) {
    super();
    const { ctl, audio } = o.processorOptions;
    this.ctl = new Int32Array(ctl);
    this.ring = new Int16Array(audio);
    this.base = RATE / sampleRate;
    this.step = this.base;
    this.fill = TARGET;
    this.pos = 0;
    this.prev = [0, 0];
    this.next = [0, 0];
    this.waiting = true;
  }

  process(_inputs, outputs) {
    const out = outputs[0], c = this.ctl, ring = this.ring, cap = ring.length;
    const w = Atomics.load(c, 6);
    let r = Atomics.load(c, 7);
    // Samples waiting (16-bit stereo at 32 kHz: 64 a millisecond).
    let have = (w - r) >>> 0;
    // Too far behind (the tab was hidden, the game caught up): back to 60 ms.
    if (have > 150 * 64) {
      r = (w - TARGET) >>> 0;
      have = this.fill = TARGET;
    }
    // Ran dry: fade out and wait for 40 ms to build up, instead of holding a
    // sample (a click) and then playing late.
    const need = Math.ceil(out[0].length * this.step) * 2 + 2;
    if (this.waiting ? have < 40 * 64 : have < need) {
      this.waiting = true;
      for (let ch = 0; ch < out.length; ch++) {
        const k = Math.min(ch, 1);
        for (let i = 0; i < out[ch].length; i++) out[ch][i] = (this.next[k] * (1 - i / out[ch].length)) / 32768;
      }
      this.prev = this.next = [0, 0];
      Atomics.store(c, 7, r);
      return true;
    }
    this.waiting = false;
    this.fill += (have - this.fill) * 0.01;
    this.step = this.base * (1 + Math.max(-0.04, Math.min(0.04, ((this.fill - TARGET) / TARGET) * 0.05)));
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
