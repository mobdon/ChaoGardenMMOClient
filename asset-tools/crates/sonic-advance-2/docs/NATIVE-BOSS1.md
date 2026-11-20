# Egg Hammer Tank II: verified arithmetic slice

`native_boss1.rs` ports the A2NE **Egg Hammer Tank II** motion and hammer
controller. This is the Sonic Advance 2 boss, distinct from Sonic Advance's
Egg Hammer Tank. Its source is pinned SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, boss source blob
`4cc2f6144dad6323525414d42390b3f8f0cf344e`.

This stopping point contains no encounter renderer or player/boss collision
acceptance. The local game host does not yet launch this boss. Destruction,
pilot escape, rescue, ending, boss results and camera integration remain
separate services. It is not a completed boss encounter, full game engine,
Tiny Garden behavior port or MMO combat authority.

## Original source bytes

`Boss1Resources::load` uses the existing verified-ROM gate and bounded readers.
It reads original suspension, arm positions, interpolation coefficients,
extension speeds and plunge offsets at ROM `0xD7A18..0xD7AA7` (143 bytes).
The native sine table is read at `0x97AA4` (2,560 bytes), as in the verified
native affine reader. Two original 16-color palettes are read at `0xD7AD0`
(64 bytes); the oracle independently converts the pinned JASC palette source
and compares every byte with that ROM location.

Table SHA-256:
`ce0f6870ed8ae3c02477e5056ca56f3fcaf1010793efabf28d8e033b65edb575`.
Palette pair SHA-256:
`7ba806a41ea2d0e52c5364490db420f7dec8abb47e7f303454bd500508c8c902`.
The public parity catalog records all source blob identities and fingerprints;
ROM, original C, generated C/executable and state vectors remain ignored/local.

## Native execution represented

- Constructor state, normal/easy and final-stage health, original positions and
  speeds, eight arm/hammer segment positions, and original countdowns.
- Native roll-in arithmetic, including unsigned cubic wrapping, frames 1..20
  without rendering, and frame-127 input release and main-task transition.
- All eight hammer handlers: reset, extend, aim, plunge, slam, hold, drag and
  retract. Random slam branching uses the shared Rust `PseudoRng`, advancing
  only on the original call boundary.
- Single-tile body terrain contact, independent front/rear suspension and
  six-tile hammer lookahead through a distinct terrain trait. `NativeStage`
  implements both services with existing bounded readers.
- Arm interpolation, original hit health/score/audio requests, pilot laugh and
  invulnerability counters, and original flashing-palette selection.
- The original `sub_8004418` octant-ratio angle helper, including s16 narrowing;
  it is distinct from BIOS ArcTan2.

The original retract bug is preserved: without `BUG_FIX`, it compares the timer
against 768 instead of comparing the hammer angle. Timer decrement and fixed
width overflow are retained rather than repaired as an MMO adaptation.

`motion_tick` is transactional on terrain errors, including shared RNG state.
Successful active motion requests collision services before pilot timers and
rendering. `handle_hit` deliberately has the original helper contract: the
collision service must first accept the attack and enforce invulnerability.
`finish_active_frame` advances pilot timers and emits the zero-health
destruction boundary. Calling these helpers is not collision integration.

## Host prerequisites

Native `CreateZoneBoss(0)` clears stage flags 0x21. Creation locks player input,
seeds the shared pseudo RNG with stage time, and inserts a priority-0x4000 task.
Those global effects are explicit host responsibilities. Initial boss position
is `(980,155)` pixels and ordinary velocity is five pixels per frame. The
native boss camera advances five pixels per frame; zone-1 Y clamps are 32..216.
The source constructor/camera/common routines remain local for further review.

The next encounter work must use original body/attack hitboxes, collision call
order, camera-delta recoil quirk and native body/arm/wheel/hammer/pilot/rescue
assets. No placeholder artwork or substituted effect has been introduced.

## Executed verification

`native_boss1_oracle.py` compiles unmodified original function bodies with the
pinned native constants and score/random macros. It uses named-field C types,
deterministic probe services and graphics/audio/task stubs. This is arithmetic
and service-boundary evidence, not struct ABI, real terrain integration,
instruction-level or full-encounter parity.

`source_native_boss1` compares 34,816 cases: 2,048 per constructor, roll-in,
eight hammer handlers, arm interpolation, hit, pilot, laugh, falling,
on-terrain and contact operation. Every represented state field, RNG, score,
sound/shake request and terrain probe coordinate/order is compared. Another
test proves failed motion leaves state and RNG unchanged.

Vector output: 10,723,328 bytes; SHA-256:
`56c28a0c05472428205f61c496b1b77c08865389f443f92af1834ad56abf3f44`.
Both local source tests pass. Rust source is formatted. Reproduce with:

```powershell
$env:A2NE_ROM = '<local verified A2NE ROM>'
$env:A2NE_BOSS1_ORACLE = '<local native-boss1-oracle/results.bin>'
cargo test --test source_native_boss1 -- --ignored
```

No live save, canonical progression, service deployment or asset publication
is performed by this module or its local verification tools.
