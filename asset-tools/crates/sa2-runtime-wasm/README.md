# sa2-runtime-wasm

Iron Gate in the browser (E8): the native game core (`sa2-runtime`) drawn
with `sa2-render` on WebGPU, or WebGL2 through wgpu's fallback. The page
reads the files it needs from the Steam "Sonic Adventure 2" folder the
player picks; nothing from the game is bundled or uploaded.

Build (needs the `wasm32-unknown-unknown` target and `wasm-bindgen-cli`
0.2.130, matching the lockfile):

```powershell
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir www/pkg target/wasm32-unknown-unknown/release/sa2_runtime_wasm.wasm
python -I serve.py "C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2"
```

Open http://127.0.0.1:8411/ and pick the game folder, or open
`?from=/game/` to load the same files from `serve.py` (development).
WASD moves the stick (the arrow keys are not bound), Space is A, X is B, Enter is Start;
IJKL move the C-stick and Q/E press L/R. A controller works too: the page
reads the first one in the browser's standard Gamepad layout and turns it
into the raw GameCube pad (`sa2_runtime::host_pad`), so the game's own
`PADClamp` and button table handle it. A, B, X, Y by name, right bumper is
Z, triggers are analog L/R, d-pad and Start as labelled.

Sound: the game's `sa2_runtime::sound` queue is drained every frame
(`App.sounds()`) and played by `asset-tools/vgmstream-wasm/web/sa2-audio.js`,
which `serve.py` serves at `/sound/` (copy that folder to `www/sound/` for any
other host). Music, effect banks and voice archives are read from the picked
game folder on demand. Audio starts after the first key or click.
