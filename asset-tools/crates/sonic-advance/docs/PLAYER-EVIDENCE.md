# SA1 Sonic movement: implemented slice and evidence

Rust reads ASOE spawn and trigonometry tables and applies dry-land Sonic
movement against signed horizontal and vertical tile profiles. The player
replay below matches 600 frames of retail Neo Green Hill Act 1.

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
turn/idle-timeout/other-state transitions are absent. A non-jumping
airborne actor currently uses idle art as an explicit presentation adaptation.

## Terrain and object contact

The stage path uses the three-tile signed-distance rules in the pinned
`src/game/shared/stage/terrain_collision.c` (SHA-256
`777229b37e656108cbb8dafafff9ea361012e682cba4e63b987b2de474e5c0d2`).
`Stage::terrain_sensor` selects the horizontal or vertical signed profile,
applies the collision layer and one-way mask, handles runs of empty/full tiles
and clamps the sample coordinates. Distance arithmetic retains the original
coordinate. An empty probe leaves the caller's angle untouched.

Grounded Sonic uses the signed rotation quadrant, two contacts on the local
supporting face, the second sensor on ties, a penetration limit of 11 pixels
and a speed-dependent attachment distance. Slope acceleration and jump impulse
use the native sine table. Airborne Sonic checks side and head contacts and
applies landing speed bands. The airborne quadrant uses velocity comparisons
against diagonals. BIOS arctangent rounding near those boundaries, full-loop
traversals and inverted gravity still need separate evidence. Synthetic worlds
that supply only occupancy and floor queries retain the earlier upright sweep.

Normal-up spikes read their collision bounds and rendered pixels from ASOE
animation 454 variant 2. The native spike task runs before player movement.
The side resolver follows the pinned assembly
`asm/non_matching/game/shared/stage/collision__sub_800CBBC.inc` because the
paired C function is marked incomplete. Top contact uses dry-land Sonic hurt
state 15 (animation 19), Q8 knockback and 120 frames of invulnerability that
begin counting down after hurt ends. Shield, water, dynamic scattered-ring
objects and other spike subtypes require additional state.

Upward and diagonal springs assign character state 17 or 21, clear variable
jump height and restore standing radius along the support normal. State 21
switches to 18 at vertical speed -1.5 pixels/frame. The sprite VM end command
advances state 17 or 18 to 19. Their ROM animation/variant pairs are 11/0,
10/0, 10/1 and 10/2 respectively (states 17, 21, 18, 19). Downward springs
cancel jump shortening while preserving the spin pose. Side springs preserve
the grounded flag and select spin or walk.

## Verification

The opt-in player test replays 600 inputs from settled Neo Green Hill Act 1
frame 1799 (Q8 position 26624, 113920, zero speed, angle 0, layer 1). The
retail reference ran the exact ASOE image in mGBA, reading post-frame player
memory starting at 0x03005A20. For frames 1800 through 2399, eight signed
little-endian 32-bit values record X, Y, air X speed, air Y speed, ground
speed, half-height, angle and layer. The SHA-256 of all 19,200 bytes is
`28e0bd5eea99bda2b150079cc582519dd8655fa9339ce0fa611ebd793e62e83c`.
Rust reads the real placement data and applies plane switches and upright
spikes in this replay. Without the first spike's side collision, frame 2238
diverges. The verified trace includes pushing and jumping past that spike.
It starts after the stage entrance sequence.

An independent reference compiled the pinned C distance routine and its macros
with Zig cc. A Python reader supplies profiles directly from the private ROM.
A deterministic set of 8,192 probes covers all four directions, both layers,
both masks and clamped edges on stage 0. The FNV-1a 64 digest of the distance
and angle bytes is `ac26d6b1194ba651` (0x55 marks unchanged angle storage).
Rust reproduces the digest. Other tests check all 18 spawn entries, malformed
indices/layers, atomic error handling, spike-top damage immunity, spring art
transitions and the existing movement cases.

```powershell
$env:SONIC_ADVANCE_ASOE_ROM = 'path/to/private/ASOE.gba'
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test player --test player_sensors -- --include-ignored
```

ROM bytes, native graphics and raw reference traces stay local.

## Dry-land rolling and spindash

`Player::step_controls` gives sideways inputs priority over Down. Sonic
crouches at rest, rolls when moving, and charges a spindash from the crouch
state on a new Jump press. Charging and release retain the current position.
Additional taps decay the accumulator before adding 512 and flooring it at
2048. The released roll uses a 4/256 friction step and 24/256 opposing input.
A jumping roll retains its spin and suppresses horizontal air input. Normal
grounding clears the rolling jump. The native sprite VM drives the end of
crouch-release and rev animations. Grounded rolling and spindash request native
sounds 109 and 110. Spindash dust rendering is separate.

ASOE reads its release speed at 0x4A7084 plus twice the integer charge. The
named table has nine entries, but the native tap rule can produce larger
indices. The Rust reader fingerprints and bounds all 128 halfwords reachable
by a positive signed 16-bit Q8 charge. In the second mGBA trace the retail
accumulator reached 4784 and the next frame released with ground speed 17218
(index 18). Synthetic no-ROM worlds reject extended lookups.

Two independent mGBA captures replay from settled Q8 (26624,113920). The
300-frame run covers charging, additional taps, braking, jumping while rolling,
landing and rolling from running. A second 64-frame sequence rolls to a stop,
returns to standing, then mashes and releases. The test serializes X, Y, air
X/Y speed, ground speed, half-height, angle, spindash charge, animation ID and
variant as ten little-endian signed 32-bit integers per frame. Every frame
matched. SHA-256 digests are respectively
`a81b4bb767f5f0525f3b7d7339133709564b4fb6e249382d87b2654aeaa1315c`
and `088d8c70d66cef772caf8c6f3f902d547636ada3c8bee111a7c1c1db052fdf61`.
The exact input windows are in `tests/player_spin.rs`.
