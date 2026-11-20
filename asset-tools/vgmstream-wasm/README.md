# SA2 sound in the browser (vgmstream → wasm)

Plays Sonic Adventure 2's music and sound effects from the player's own Steam
install. Nothing from the game is in this repository; files are read at runtime
from the folder the player picks.

- `shim.c`: small C API over libvgmstream (open a file from memory, info, fill float PCM, seek).
- `build.sh`: builds vgmstream's core + the shim to `web/vgmstream.wasm` with `zig cc`
  (wasm32-wasi reactor, no imports). Pinned to vgmstream `r2168`:
  `git clone --depth 1 --branch r2168 https://github.com/vgmstream/vgmstream`, then
  `ZIG=zig VGMSTREAM_SRC=<checkout> ./build.sh`. The built `.wasm` (1.4 MB) is committed, so
  CI and the web build need no C toolchain.
- `web/vgmstream.js`: loads the wasm, `decode(bytes, name, subsong)` → per-channel `Float32Array`s + loop points.
- `web/sa2-audio.js`: Web Audio backend. Takes the engine's `sa2_runtime::sound::SoundCmd`
  (serde shape, e.g. `{ PlayBgm: { file: 'BOSS_02A.ADX' } }`) and plays it on master/bgm/se/voice
  gain buses. Music loops natively at its ADX loop points.
- `web/index.html`: sound test page (pick the Steam folder, play music and effects).
- `web/vgmstream.test.mjs`: `node --test asset-tools/vgmstream-wasm/web/vgmstream.test.mjs`.
  Real-file checks read `SA2_INSTALL` (default Steam path) and skip when it is absent.

## What decodes (Steam, checked 2026-11-20)

| Source | Files | Result |
| --- | ---: | --- |
| `gd_PC/ADX/*.adx` music | 157 | all decode (CRI ADX type 03), loop points kept |
| `MLT/`, `MPB/`, `event/MLT/` `.csb` sound banks | 193 + 37 + 53 | all 6,577 waveforms decode (AAX/ADX subsongs) |
| `event_adx(_e).afs` voices | 2 archives, 2,727 entries each | **not decoded**: entries are AHX (MPEG layer II), which vgmstream only decodes with libmpg123 (LGPL). `Sa2Audio` takes a `decodeVoice` hook instead; the repo's own Rust AHX decoder (`sa2::audio::decode_ahx_wav`) is the candidate. |

Subsongs are vgmstream's waveform order; `sa2::csb` maps cue names to waveforms.
