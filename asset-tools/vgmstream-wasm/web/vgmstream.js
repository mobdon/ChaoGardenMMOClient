// Thin wrapper over vgmstream.wasm (see ../shim.c). Works in browsers and Node.
// const vgm = await Vgmstream.load(bytesOrUrl);
// const song = vgm.decode(fileBytes, 'BOSS_02A.ADX', subsong);
//   -> { rate, channels, samples: [Float32Array per channel], loop: {start, end} | null, subsongs, ... }

export class Vgmstream {
  static async load(source) {
    let bytes = source;
    if (typeof source === 'string' || source instanceof URL) {
      bytes = await (await fetch(source)).arrayBuffer();
    }
    const { instance } = await WebAssembly.instantiate(bytes, {});
    instance.exports._initialize?.();
    return new Vgmstream(instance.exports);
  }

  constructor(exports) { this.x = exports; }

  #cstr(ptr) {
    const mem = new Uint8Array(this.x.memory.buffer);
    let end = ptr;
    while (mem[end]) end++;
    return new TextDecoder().decode(mem.subarray(ptr, end));
  }

  // Opens `bytes` (named `name`; the extension picks the format). Returns a handle
  // for info/fill, or null when vgmstream does not recognise it. Call close().
  open(bytes, name, subsong = 0) {
    const x = this.x;
    const data = x.va_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, data, bytes.length).set(bytes);
    const enc = new TextEncoder().encode(name + '\0');
    const np = x.va_alloc(enc.length);
    new Uint8Array(x.memory.buffer, np, enc.length).set(enc);
    const h = x.va_open(data, bytes.length, np, subsong, 0); // takes ownership of data
    x.va_free(np);
    if (!h) return null;
    return {
      handle: h,
      rate: x.va_rate(h),
      channels: x.va_channels(h),
      subsongs: x.va_subsongs(h),
      length: x.va_samples(h),
      loop: x.va_loop_flag(h) ? { start: x.va_loop_start(h), end: x.va_loop_end(h) } : null,
      codec: this.#cstr(x.va_codec(h)),
      meta: this.#cstr(x.va_meta(h)),
      streamName: this.#cstr(x.va_stream_name(h)),
    };
  }

  close(info) { if (info) this.x.va_close(info.handle); }

  // Decodes the whole (sub)song once through, without repeating loops.
  // shortcut: whole-file decode (fine for SA2's ADX/CSB sizes); stream in chunks if memory matters.
  decode(bytes, name, subsong = 0) {
    const info = this.open(bytes, name, subsong);
    if (!info) return null;
    const x = this.x, ch = info.channels;
    const block = 4096;
    const buf = x.va_alloc(block * ch * 4);
    const out = Array.from({ length: ch }, () => new Float32Array(Math.max(1, info.length)));
    let pos = 0;
    try {
      for (;;) {
        const n = x.va_fill(info.handle, buf, block);
        if (n < 0) throw new Error(`vgmstream decode failed in ${name}`);
        if (n === 0) break;
        const f = new Float32Array(x.memory.buffer, buf, n * ch);
        const take = Math.min(n, out[0].length - pos);
        for (let i = 0; i < take; i++) for (let c = 0; c < ch; c++) out[c][pos + i] = f[i * ch + c];
        pos += take;
        if (take < n) break;
      }
    } finally {
      x.va_free(buf);
      this.close(info);
    }
    const { handle, ...rest } = info;
    return { ...rest, samples: out.map(c => c.subarray(0, pos)) };
  }
}
