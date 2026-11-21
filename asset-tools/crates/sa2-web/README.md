# sa2-web

The recompiled SA2 (sa2-recomp's GSNE8P code) in a browser tab, on this
computer only. Nothing from the game is bundled or hosted: the local server
reads the player's own folders.

- `game/` (`sa2-web-game`, wasm32-wasip1-threads): the desktop host's boot
  hooks, with the window's parts replaced by device files: `/dev/gx` takes
  each finished frame's GX job (pipe bytes and RAM), `/dev/pad` gives the
  keyboard and gamepad, `SA2_AUDIO_WAV=/dev/audio` the mix. The generated
  `sa2-host` crate (local, never in git) calls `sa2_web_game::main`.
- this crate (`sa2-web`, wasm32-unknown-unknown with wasm-bindgen): decodes
  the GX jobs and draws them with sa2-gx on WebGPU, or WebGL2 without it,
  blended between game frames on displays above 60 Hz, widened to the tab
  unless `?aspect=4:3`.
- `www/`: the page. `worker.js` runs each game thread in a worker on one
  shared memory and answers the WASI calls std makes (files through the
  server, threads through the page); `audio.js` plays the mix.
- `serve.py`: the local server (127.0.0.1 only, cross-origin isolated so the
  page may share memory between threads).

## Build

The game build rebuilds std with unwinding, which the game's threads use to
end, and uses wasm SIMD (a steady 60 in Iron Gate; 45-53 without), so it
needs nightly with `rust-src` and the `wasm32-wasip1-threads` target. The
generated `sa2-host` crate's `.cargo/config.toml`:

```toml
[build]
target = "wasm32-wasip1-threads"

[unstable]
build-std = ["std", "panic_unwind"]

[target.wasm32-wasip1-threads]
rustflags = ["-C", "panic=unwind", "-C", "target-feature=+exception-handling,+simd128",
             "-C", "link-arg=--max-memory=1073741824", "-C", "link-arg=--initial-memory=67108864"]
```

The page's module runs on two workers sharing one memory (render.js draws,
decode.js decodes), so it is built with wasm threads too (flags in this
crate's `.cargo/config.toml`):

```powershell
cargo +nightly build --release   # in the sa2-host crate
cargo +nightly build --release --target wasm32-unknown-unknown -Zbuild-std=std,panic_abort
wasm-bindgen --target web --out-dir www/pkg target/wasm32-unknown-unknown/release/sa2_web.wasm
```

## Run

```powershell
python -I serve.py --game <sa2-host.wasm> --mount steam=<Steam SA2 folder> `
  --mount disc=<extracted disc files> --mount rels=<hybrid modules> `
  --file research/main.dol=<main.dol> --file research/symbols.txt=<symbols.txt> `
  --env SA2_STEAM_DIR=/steam --env SA2_DISC_DIR=/disc --env SA2_DISC_FIRST=1 `
  --env SA2_REL_DIR=/rels --env SA2_AUDIO_WAV=/dev/audio --open
```

Keys: W A S D move, Space = A, E or Left Shift = B, Q = X, R = Y, Enter = Start,
1 and 3 = L and R; mouse left = B, right = A; a standard gamepad works too.
Double-click the picture for full screen.
