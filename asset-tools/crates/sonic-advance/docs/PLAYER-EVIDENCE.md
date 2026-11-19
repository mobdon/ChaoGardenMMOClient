# SA1 Sonic movement: implemented slice and evidence

Verified November 18, 2026. `src/player.rs` implements a deterministic Rust
dry-land upright Sonic movement slice. It uses the Sonic Advance 1 source
branches and actual ASOE tables. Its terrain resolution is a bounded upright
adapter, not the complete retail collision/state machine. It neither changes
the MMO service nor imports rewards, saves, credentials or Chao progression.

## Source identity and provenance

Input: Sonic Advance USA, `ASOE`, revision 0, 8,388,608 bytes.
SHA-256 `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
The constructor rejects every other ROM fingerprint. Native inputs and
downloaded reference text remain in ignored local storage.

Research reference: SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Original Rust is written from the
observed rules; no C implementation or native table bytes are embedded here.
The relevant reference texts were downloaded into the root checkout's ignored
`.local/sonic-advance-runtime-research/` directory. Exact UTF-8 SHA-256:

| Pinned source path | SHA-256 |
| --- | --- |
| `src/game/shared/stage/player.c` | `300f37f946203ab4bb20fbbd5c62b20b0a5e9f55618ffc58bbd8f2b13d49bdb8` |
| `src/game/sa1/stage/player_sonic.c` | `fea5f251ad077581065c1932eacb0d033175e3d575230287ae87b0184ca27059` |
| `include/game/shared/stage/player.h` | `f1ba7d811c64a6cb97713dc8f33f69750bf46c9a847d8bb3a9290716677fa83d` |
| `include/constants/sa1/char_states.h` | `d307664640fe0e82faa4f190db2f57981f3ef3076ae7db3f99da5e269f9f5adf` |
| `include/constants/sa1/animations.h` | `2bd60bad3f75ff05abbf71af4cdd8d1ebff3ede65380785c2bac9dc85eebc5bb` |
| `src/trig.c` | `92730adc2ea84c792f94cb5550bf8819832ad9f5c25ed835adcf25c5b5ca917b` |
| `include/trig.h` | `679683dc9a1d3f4f572146992530e6cb2886283befd370643fc2bc8d15b1bc13` |

The shared parameter header says its constants originated in SA2. They are
therefore not accepted solely from that heading: the implemented gravity is
the macro actually selected by `GAME_SA1`, and the jump magnitude is also
explicitly present as `0x4E0` in the SA1-only `Player_TryJump`. The selected
SA1 initializer sets maximum speed to 4.5, while the excluded SA2 branch sets
9.0 and a separate 6.0 top speed. The excluded SA2 movement, boost and
15-pixel fall-speed clamp are not applied here.

## ASOE tables, not Europe offsets

The pinned `data/sa1/data_0_4.s` defines the relationships among spawn records,
idle animation bases and state-animation pairs. ASOE locations were verified
independently using the idle-base sequence, adjacency and native code pointers:

| ASOE resource | ROM offset / length | SHA-256 |
| --- | --- | --- |
| Single-player spawn records | `0x4A6DEC` / 72 bytes | `2319766677333c64570fe5478adbe0118e7fd2bc078ebb4532f2462c011d81fb` |
| Four idle animation bases | `0x4A6F04` / 8 bytes | `f70a9d5a6329ee611d073494a26cf67146c616f253d4d3446328f287eced845a` |
| 94 state-animation pairs | `0x4A6F0C` / 376 bytes | `985682f047869070d741d18c73eed96b65830e4d341336b37eca0dc9e1818cdf` |
| Sine table, 1280 signed Q2.14 words | `0x9B3F0` / 2560 bytes | `78caf4fc1cb643a1f24d8e9d9efb1822cda68335adc1799d517582939d9c5c99` |

The spawn table is referenced at ROM `0x131A4` and `0x42230`; its first signed
pair is **(104,444)**, the native player anchor for stage index 0. The
state-animation root is referenced at `0x46090`. The sine table has multiple
native references, including `0x2F80`, `0x37CC`, `0x3988` and `0x3CD4`.

Trigonometry is read from the user's verified ROM, selecting every fourth
Q2.14 entry and arithmetic-shifting by six, as the native `SIN_24_8` path does.
No approximate browser/host sine function supplies game movement. For example,
angle 16 yields sine 97 and cosine 236 in Q8. Layer 1 is selected because
`PLAYER_LAYER__BACK` equals 1 and the SA1 initializer assigns that layer.

## Implemented arithmetic and frame order

All positions and velocities use signed fixed point with eight fractional
bits. One call consumes one 60 Hz frame with left/right/jump booleans; it
accepts no caller time delta. The regular dry path implements:

| Operation | Raw fixed-point value / rule |
| --- | --- |
| Ground acceleration | 8 subpixels/frame² |
| Opposing-direction braking | 96 subpixels/frame²; crossing zero selects the opposing 96-unit quantum |
| Coasting friction | 8 subpixels/frame², stopping at zero |
| Input top speed | 1152 = 4.5 pixels/frame; existing slope/external overspeed is preserved |
| Ground slope acceleration | `(native_sine_Q8(angle) * 3) >> 5`, applied only while ground speed is nonzero |
| Tangential speed projection | Ground speed times native cosine/sine, arithmetic `>>8` |
| Air acceleration | 16 subpixels/frame²; left takes priority if both directions are held |
| Dry jump impulse | 1248 = 4.875 pixels/frame, along the native surface normal |
| Early jump release | Rising speed is limited to −768 = −3 pixels/frame |
| Dry gravity | 42 subpixels/frame², after integrating the previous vertical speed |
| Standing/spinning offsets | Half width 6; half height 14 / 9; shrinking on a floor moves the anchor down 5 pixels |
| Rising-air horizontal drag | Exact SA1 unsigned vertical-speed predicate and signed arithmetic `vx >> 5` subtraction |

The SA1 ground-jump branch changes speed/state/offsets and skips position
integration on that input frame. The next airborne frame applies jump-release
control, air input/drag, integration, and gravity. This differs from applying an
impulse and gravity before movement on the jump-button frame. Both-direction
ground input suppresses acceleration and coasting friction, as the source's
direction switch and subsequent input mask do.

The renderer receives verified Sonic choices for idle 0, walk 4, run 5,
jump/spin 8 and brake 9/7. Run begins at absolute ground speed 4.5; walk uses
Q4.4 playback speed `clamp(abs(speed)>>4,8,16)`, and run uses 16. Full retail
turn/idle-timeout/hurt/spring/other-state transitions are absent. A non-jumping
airborne actor currently uses idle art as an explicit presentation adaptation.

## Collision adapter and remaining gaps

The adapter consumes the stage reader's actual collision profiles and floor
angles, using two floor probes and bounded pixel sweeps for upright body walls
and ceilings. Floor attachment accepts native angles in the upright band
−32..31. It does not implement retail four-direction sensor selection, wall
attachment, loops, inverted gravity or collision-layer switches. It conserves
horizontal speed on landing rather than implementing the complete native
angle-band landing projection. The scan lengths, body sweep probes and
ground-attachment window are adaptations, not claimed retail constants.

Input/world/state ranges, all terrain-provided contact distances and frame
counter overflow are checked. A query failure or invalid state leaves the
previous player unchanged. Bounded sweeps stop at an encountered wall/ceiling;
unsupported grounded non-upright state is rejected. Falling out reports an
event and does not fabricate a respawn, checkpoint or save.

Water, rolling, spindash, Sonic's additional attacks, damage/death/lives,
collectibles, springs, scripted interactables, bosses, checkpoints, stage clear,
camera/game state transitions, complete animation timing and audio remain
separate work. This slice cannot be described as a complete native game.

## Verification

Nine player tests pass, including one opt-in source-dependent test. Synthetic
tests check a 144-frame ground acceleration trace against a closed-form
position sequence, friction, reversal, overspeed, opposing inputs, source-based
slope arithmetic, jump-frame ordering, held versus released jump, air drag,
thin-wall/ceiling sweeps and atomic malformed-state/query rejection.

The actual local ASOE test loads stage 0 and the native spawn/sine records;
no synthetic stage floor is substituted. It reaches native floor contact at
Q8 anchor **(26624,114176)** = (104,446). A 150-frame rightward input trace with
jump held on frames 50–69 advances to Q8 **(81272,114176)**, reaches minimum
Q8 Y **96286**, and records jump and landing without falling out. This proves
the implemented core consumes the real local terrain; it is not an emulator
trace or a claim of complete retail collision/trajectory parity.

Run from the worktree with the root's isolated Rust toolchain activated:

```powershell
$env:SONIC_ADVANCE_ASOE_ROM = 'path/to/private/ASOE.gba'
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test player -- --include-ignored
```

Native trigonometry, ROMs, downloaded references and rendered/extracted content
remain local. Published fixtures contain original synthetic values and hash
metadata only.
