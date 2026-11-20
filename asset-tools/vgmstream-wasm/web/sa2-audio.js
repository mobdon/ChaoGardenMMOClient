// Web Audio backend for the engine's sound commands (sa2-runtime `sound::SoundCmd`).
// Decodes the player's own Steam files with vgmstream.wasm; nothing is uploaded.
//
//   const audio = await Sa2Audio.create({ wasm: './vgmstream.wasm', readFile });
//   // readFile(path under the game folder, e.g. 'resource/gd_PC/ADX/BOSS_02A.ADX') -> Uint8Array | null
//   audio.run({ PlayBgm: { file: 'BOSS_02A.ADX' } });   // the serde shape of SoundCmd
//
// Browsers start audio only after a user gesture: call audio.resume() from a click/key handler.
import { Vgmstream } from './vgmstream.js';

const GD = 'resource/gd_PC/';

export class Sa2Audio {
  static async create({ wasm = new URL('./vgmstream.wasm', import.meta.url), readFile, context } = {}) {
    return new Sa2Audio(await Vgmstream.load(wasm), readFile, context ?? new AudioContext());
  }

  constructor(vgm, readFile, ctx) {
    this.vgm = vgm;
    this.readFile = readFile;
    this.ctx = ctx;
    this.bus = {};
    this.bus.master = ctx.createGain();
    this.bus.master.connect(ctx.destination);
    for (const b of ['bgm', 'se', 'voice']) {
      this.bus[b] = ctx.createGain();
      this.bus[b].connect(this.bus.master);
    }
    this.cache = new Map(); // decoded AudioBuffers by key
    this.bgm = null;
    this.bgmToken = 0;
  }

  resume() { return this.ctx.resume(); }

  // Runs one SoundCmd ({ PlayBgm: {...} } or 'StopBgm'). Returns a promise; errors are reported, not thrown.
  run(cmd) {
    const [kind, a] = typeof cmd === 'string' ? [cmd, {}] : Object.entries(cmd)[0];
    const job = {
      PlayBgm: () => this.playBgm(a.file),
      StopBgm: () => this.stopBgm(),
      PlaySe: () => this.playSe(a.bank, a.part, a.subsong),
      PlayVoice: () => this.playVoice(a.index, a.english),
      SetVolume: () => this.setVolume(a.bus, a.volume),
    }[kind];
    if (!job) return Promise.resolve(console.warn('sa2-audio: unknown command', cmd));
    return Promise.resolve().then(job).catch(e => console.warn('sa2-audio:', e.message ?? e));
  }

  setVolume(bus, volume) {
    const node = this.bus[String(bus).toLowerCase()];
    if (node) node.gain.value = Math.min(1, Math.max(0, Number(volume) || 0));
  }

  async playBgm(file) {
    const token = ++this.bgmToken;
    const song = await this.#buffer(`${GD}ADX/${file}`, file, 0, false); // music is large: not cached
    if (token !== this.bgmToken) return; // a later PlayBgm/StopBgm won
    this.stopBgm(false);
    const src = this.ctx.createBufferSource();
    src.buffer = song.buffer;
    if (song.loop) {
      src.loop = true;
      src.loopStart = song.loop.start / song.rate;
      src.loopEnd = song.loop.end / song.rate;
    }
    src.connect(this.bus.bgm);
    src.start();
    this.bgm = src;
  }

  stopBgm(cancel = true) {
    if (cancel) this.bgmToken++;
    this.bgm?.stop();
    this.bgm = null;
  }

  // bank: folder under gd_PC, e.g. 'MLT/se_kart' (a bare name means MLT/<name>).
  async playSe(bank, part, subsong) {
    const dir = bank.includes('/') ? bank : `MLT/${bank}`;
    const s = await this.#buffer(`${GD}${dir}/${part}.csb`, `${part}.csb`, subsong);
    this.#once(s.buffer, this.bus.se);
  }

  // shortcut: event voices are AHX (MPEG layer II), which vgmstream decodes only with
  // libmpg123 (LGPL) — not in this core build. Pass `decodeVoice(bytes) -> {rate, samples}` to enable.
  async playVoice(index, english = true) {
    if (!this.decodeVoice) throw new Error('voice lines (AHX) need a decodeVoice hook; not built in');
    const key = `voice:${english}:${index}`;
    let s = this.cache.get(key);
    if (!s) {
      const afs = await this.#read(`${GD}event_adx${english ? '_e' : ''}.afs`);
      const v = new DataView(afs.buffer, afs.byteOffset, afs.byteLength);
      if (index >= v.getUint32(4, true)) throw new Error(`no voice ${index}`);
      const off = v.getUint32(8 + index * 8, true), len = v.getUint32(12 + index * 8, true);
      s = this.#toBuffer(await this.decodeVoice(afs.subarray(off, off + len)));
      this.cache.set(key, s);
    }
    this.#once(s.buffer, this.bus.voice);
  }

  #once(buffer, bus) {
    const src = this.ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(bus);
    src.start();
  }

  async #read(path) {
    const bytes = await this.readFile(path);
    if (!bytes) throw new Error(`missing ${path}`);
    return bytes;
  }

  async #buffer(path, name, subsong, keep = true) {
    const key = `${path}#${subsong}`;
    let s = this.cache.get(key);
    if (!s) {
      const d = this.vgm.decode(await this.#read(path), name, subsong);
      if (!d) throw new Error(`vgmstream cannot read ${path}`);
      s = this.#toBuffer(d);
      if (keep) this.cache.set(key, s);
    }
    return s;
  }

  #toBuffer(d) {
    const frames = Math.max(1, d.samples[0].length);
    const buffer = this.ctx.createBuffer(d.samples.length, frames, d.rate);
    d.samples.forEach((c, i) => buffer.copyToChannel(c, i));
    return { buffer, rate: d.rate, loop: d.loop };
  }
}
