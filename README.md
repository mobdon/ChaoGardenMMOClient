# SA2 recompilation (branch `claude/recomp-full-compile`)

This branch holds only the Sonic Adventure 2 port: the static recompiler
(GSNE8P `main.dol` and modules to Rust), the host hooks it boots with, the
GX renderer, and the local browser build. The Chao MMO lives on `main`.

Do not merge this branch into `main` as a whole: it has none of the MMO's
files, so git would delete them there. Move work to `main` by cherry-pick.

Nothing from the game is in git. Generated code and game files stay local.

| Crate (`asset-tools/crates/`) | What it is |
| --- | --- |
| `sa2` | Format readers and native ports of the game's code |
| `sa2-recomp` | The recompiler and the runtime its output links against |
| `sa2-boot` | Host hooks: DVD from Steam files, GX, VI, PAD, audio, memory card, window |
| `sa2-gx`, `sa2-render` | GX on wgpu (adapted from noclip.website, MIT) |
| `sa2-runtime`, `sa2-runtime-wasm`, `sa2-play` | The hand-ported runtime and its tools |
| `sa2-web` | The recompiled game in a browser tab, local only |
| `sa2-texture-wasm`, `sa2-voice-wasm` | Small browser decoders |

`asset-tools/vgmstream-wasm` is the sound decoder build; see
`THIRD_PARTY_NOTICES.md`.
