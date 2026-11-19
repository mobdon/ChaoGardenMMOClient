# Native affine OBJ presentation

`native_animation.rs` now implements the native `TransformSprite` matrix and
anchor arithmetic and affine OBJ composition. `AffineTransform` carries the
native 1024-unit rotation and signed Q8 X/Y scales. `native_transform` returns
the four signed Q8 inverse-mapping coefficients and adjusted native Sprite
position. Both use the original sine table at cartridge offset 0x97AA4.

`AnimationPlayer::render_affine(transform, double_size)` returns the same
`SpriteFrame` shape as ordinary composition, with the transformed anchor and
original pixels. It preserves each OBJ's individual center, clipping box,
shape, size, 1D tile layout, palette, transparency, priority and part ordering.
The output rectangle contains the native OBJ boxes; it is a host transport
rectangle rather than a new native game asset. The host applies camera/world
position and clips the result to its viewport.

The native player walking branch sets `rotation = player.rotation << 2`,
clears ordinary flip flags, and uses scale X -256 when facing right or +256
when facing left, with scale Y +256 under normal gravity. A negative X scale
also decrements the logical transform X position by one pixel. The native
walking path does not enable double-size mode. Integration must retain that
one-pixel offset and use the returned anchor rather than rotating a cropped
image around a guessed center.

Ordinary flip flags and frame flip metadata do not affect affine composition,
matching `DisplaySprite`. The subsequent `TransformSprite` call overwrites
the position modified by animation translation commands, so affine frames
return zero command translation. Global transforms, arbitrary external matrix
slots, mosaic, OBJ blending/window modes and hardware OAM capacity/timing are
still separate services. Templates requiring another matrix slot return an
explicit error. Zero scale is an explicit host error rather than a claim about
retail BIOS division-by-zero behavior.

`set_palette_bank(bank, words)` mirrors a single native global OBJ palette-bank
write into the host player's private palette. Banks are bounded to 0..15 and
the input has exactly sixteen original BGR555 words. The host calls this after
that frame's animation commands when reproducing the shared native OBJ palette.
The method changes no save, Chao, reward or server-authoritative state.

Player initialization uses OAM order 16 + player ID and BG priority 2. Native
OAM buckets copy in ascending order, and the lower hardware OAM index wins at
equal priority. For the single-player slice's equal-priority order values,
ring order 20 draws behind player order 16, and pickup order 15 draws in front.
Cross-sprite ordering remains the host compositor's responsibility.

## Actual cartridge execution oracle

The pinned source commit is `2dd3f17d30cd3c156e508aac0017620ae485b9ba`.
Its C `TransformSprite` is marked `NONMATCH`; the test helper therefore
executes the actual verified cartridge Thumb routine at 0x08004860, ending
before the next function at 0x08004ABC. The 604-byte routine span SHA256 is
`99ae43a517639fa31b1e6a9f9643a372c386301e5c95e4512c4a753179f346fa`.

This execution uses local Unicorn 2.1.3 only as a test oracle. A bounded shim
supplies signed integer BIOS `Div` results for nonzero scale inputs. That does
not establish BIOS exception, interrupt, frame timing or hardware parity.
The Rust game runtime has no CPU emulator dependency and implements the
arithmetic directly. Raw local assembly/header/C files are checked against
their exact Git blob hashes before use; no normalization is accepted.

The oracle covers 4,096 matrix/anchor cases across every rotation, signed
scales including tiny/narrowing cases, signed positions and original idle,
walk and jump frame dimensions. All six words per case match. The local
98,304-byte vector SHA256 is
`67e70c895cde4667f3e2036281993c80ca83c1b6f903c8c6a665e7c080c7594a`.

An independent Python OBJ sampler uses the actual routine's matrices/anchors,
original OAM templates, ROM-backed tile/palette commands and fixed-point pixel
sampling. All 360 RGBA/origin comparisons match across the six Sonic walk
variants, six Show commands each, five angles, both facing directions and
alternating normal/double-size bounds. This tests the listed composition path;
it does not claim that the independent sampler is the original GBA renderer.
One additional Rust unit test checks identity pixels, native mirrored-edge
clipping, ignored ordinary flips, double-size bounds and rejected zero scale.

From the original workstation checkout:

```powershell
. ./scripts/use-rust.ps1
$crateRoot = '.local/worktrees/sonic-advance-2-native/asset-tools/crates/sonic-advance-2'
$python = 'C:/Users/Admin/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'
$env:PYTHONPATH = (Resolve-Path .local/tools/python-affine).Path
$env:A2NE_ROM = (Resolve-Path .local/gba/A2NE-f2b7a3329401.gba).Path
& $python "$crateRoot/tests/native_affine_oracle.py" `
  --rom $env:A2NE_ROM `
  --reference-dir .local/research/sat-r-affine `
  --output-dir .local/gba/native-affine-oracle `
  --summary "$crateRoot/catalog/native-affine-parity.json"
$env:A2NE_AFFINE_ORACLE = (Resolve-Path .local/gba/native-affine-oracle/transforms.bin).Path
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --lib native_animation::
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --test source_native_affine -- --ignored
```

Provide local Unicorn and exact pinned reference files in a clean checkout;
this test dependency is not installed by Cargo. The public helper and metadata
contain no ROM, original source, executable, image or native vector fixture.
Those files remain under ignored `.local`. No live service, save migration or
runtime rollout is authorized or performed by this subsystem work.
