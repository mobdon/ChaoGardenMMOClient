// AHX voice decoding with the repo's own Rust decoder (asset-tools/crates/sa2-voice-wasm → ahx.wasm).
// const ahx = await Ahx.load(bytesOrUrl); ahx.decode(entryBytes) -> { rate, samples: [Float32Array] } | null

export class Ahx {
  static async load(source = new URL('./ahx.wasm', import.meta.url)) {
    let bytes = source;
    if (typeof source === 'string' || source instanceof URL) bytes = await (await fetch(source)).arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return new Ahx(instance.exports);
  }

  constructor(x) { this.x = x; }

  decode(bytes) {
    const x = this.x;
    const p = x.alloc(bytes.length);
    new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
    const ok = x.decode(p, bytes.length);
    x.free(p, bytes.length);
    if (!ok) return null;
    return parseFloatWav(new Uint8Array(x.memory.buffer, x.out_ptr(), x.out_len()).slice());
  }
}

// The decoder writes a 32-bit float WAVE; find its fmt and data chunks.
export function parseFloatWav(wav) {
  const v = new DataView(wav.buffer, wav.byteOffset, wav.byteLength);
  let rate = 0, channels = 0, at = 12;
  while (at + 8 <= wav.length) {
    const id = String.fromCharCode(...wav.subarray(at, at + 4)), size = v.getUint32(at + 4, true), body = at + 8;
    if (id === 'fmt ') { channels = v.getUint16(body + 2, true); rate = v.getUint32(body + 4, true); }
    if (id === 'data') {
      const n = Math.floor(Math.min(size, wav.length - body) / 4 / channels);
      const samples = Array.from({ length: channels }, () => new Float32Array(n));
      for (let i = 0; i < n; i++) for (let c = 0; c < channels; c++) samples[c][i] = v.getFloat32(body + (i * channels + c) * 4, true);
      return { rate, samples };
    }
    at = body + size + (size & 1);
  }
  return null;
}
