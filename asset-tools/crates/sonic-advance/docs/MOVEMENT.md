# Sonic movement evidence

This module ports a bounded subset of Sonic Advance USA revision-zero movement.
The input must match ROM SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
Slope projection, loops, gravity inversion, water, powerups, attacks and other
characters are not implemented by this kernel. The runtime's pixel-ray terrain
settlement remains an adaptation. A matching local input trace does not prove a
complete engine port.

## References and source identity

The inspected primary source is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, selecting its `GAME_SA1` branches.
The implementation is original Rust derived from the format/rule evidence;
third-party source files remain private.

| Pinned source path | SHA-256 |
| --- | --- |
| `src/game/shared/stage/player.c` | `300f37f946203ab4bb20fbbd5c62b20b0a5e9f55618ffc58bbd8f2b13d49bdb8` |
| `src/game/sa1/stage/player_sonic.c` | `fea5f251ad077581065c1932eacb0d033175e3d575230287ae87b0184ca27059` |
| `include/game/shared/parameters/characters.h` | `bb8489c7f0c5681e05970dce43e96495de997d8584fa71ff735a9ecf33423aed` |

The private source copies were also checked against their Git blob identities
in the pinned tree. Relevant routines are `InitializePlayer`,
`Player_TrySpindash`, `Player_TryJump`, `Player_8043DDC`, `Player_80447D8`,
`Player_AirInputControls`, `Player_HandleInputs` and
`Player_HandleSpriteYOffsetChange`.

## Fixed-point and input rules

Positions have eight fractional bits and signed 32-bit storage. Native speed
words are signed 16-bit; this API widens their values and explicitly normalizes
them before arithmetic. Base ground acceleration is 8, braking 96, friction 8,
and the input-accelerated speed limit is 1152. Air acceleration is 16, gravity
42, jump impulse 1248 and released-jump upward limit -768. These values are per
logical frame. The SA1 position path does not apply SA2's 15-pixel fall cap.

New jump presses are edges. Horizontal input suppresses vertical input. A
successful jump changes state and velocity without integrating position or
gravity on that tick. The new action API also applies the native standing-to-
curled center adjustment. Airborne position integration precedes gravity.

## Crouch, roll and spindash

DOWN at rest establishes crouch. A jump edge from the preceding crouched state
starts charging at zero; pressing DOWN and jump simultaneously from standing
performs a normal jump. A moving character enters roll on DOWN. Entry uses
that frame's ordinary friction; subsequent rolling friction is 4. Opposite
direction braking is 24, with a crossing-zero replacement speed of 96 before
rolling friction. Same-direction input does not accelerate a roll. Rolling
jumps disable directional air acceleration through the native flip flag.

Charging requires `configure_spindash` followed by
`advance_with_actions(input, down, clearance_pixels)`. The existing `Input`
struct and walk/jump entry points remain compatible. The legacy entry point
retains its original standing-height jump contract; the native action entry
point uses the curled body.

While DOWN remains held, charge loses `charge >> 5` each frame. A new jump edge
then adds 512. **Retail uses a minimum of 2048 after that addition, rather than a
maximum.** Actual ASOE Thumb instructions at ROM `0x441DA..0x441E0` confirm this
unusual comparison; it is not silently repaired in Rust. The complete inspected
routine at ROM offset `0x44070`, length `0x194`, has SHA-256
`6d085cb8480b10ea05f698b039346facb1b6bf0ff27da86937a2bf5c35ea24e5`.

Release selects the signed speed word indexed by `charge >> 8`. Physical
alternating press edges and decay bound this implementation to indices 0..32.
The source-backed prefix at ROM `0x4A7084`, length 66, has SHA-256
`5297e53ffab0d05d677f379f2f0d7a09b3886c7c0e5c7f931bf1685a7631a1d0`.
Its first nine words are the normal release table; higher indices reproduce
the original adjacent-data read, including unusual or negative speeds. No
release words are embedded in the executable. Invalid source or a charge
outside the proven prefix is rejected without changing the movement state.

Release changes ground speed but leaves air-X velocity and position unchanged
on that tick. The following rolling tick projects ground speed to air X,
subtracts rolling friction and integrates position. Continued grounded terrain
settlement must preserve the release speed even while air X is still zero.

## Body and animation integration

Standing offsets are `(6,14)`; curled offsets are `(6,9)`. Shrinking moves the
center five pixels down, and uncurling moves it five pixels up, preserving the
feet on flat ground. `body_half_height()` supplies the active height. A real
air-to-ground transition uncurls once; continuing grounded settlement does not
discard roll or charge state. The runtime must use the old and new body sizes
in its collision sweep, not a fixed standing rectangle.

The ASOE character-state table at `0x4A6F0C` maps crouch to animation 3 variant
0, roll/jump to 8 variant 0, charge to 18 variant 0 and rev to 18 variant 1.
`notify_animation_finished()` implements the rev-state return to charge and
must be called from the actual native clip END event.

## Independent retail check

The ignored movement test reproduces the independently recorded mGBA ASOE
frames 1800..1810: crouch; charge with the five-pixel center shift; charge 2048,
1984, 2434, 2358, 2285 and 2214; release speed 2304 with air-X velocity zero;
then rolling speed 2300 on the next tick. This is a limited state/number
comparison, not a screen, collision, sound, camera or whole-game parity claim.

The repeated-edge test also reaches the bounded charge fixed point 8324 and
checks the corresponding source lookup. These practice mechanics never alter
canonical Chao profiles, MMO rings, saves or credentials.

## Independent ring-route limit

A separate cold-boot retail replay used the same exact ROM and mGBA core
SHA-256 `d775ecef6da1da5489ff2e0a37c13826b510c73711f89e78cb8d757c6f8c1426`.
After reaching the settled stage start, it applied 20 idle frames, 120 right,
30 right+jump and 100 idle. The native ring counter at IWRAM `0x4FEC` and the
individual decompressed ring records were observed together. No save was read
or written.

| Retail absolute frame / route tick | Native record / position | Player center Q24.8 | Air speed X/Y | Body half-height | Native count |
| --- | --- | --- | --- | --- | --- |
| 1997 / 198 | 1267 / `(464,400)` | `(115977,100094)` | `(491,984)` | 9 | 1 |
| 2021 / 222 | 1269 / `(504,400)` | `(125913,101636)` | `(323,0)` | 14 | 2 |

The two records changed to the native collected marker on those frames.
`tests/retail_rings.rs` independently checks that the Rust map reader and pickup
rectangle select the same two records at these observed centers. It does not
assert that the current adapted terrain runtime reaches those centers.

The settled retail and Rust movement states agree through route tick 132. At
tick 133, the same X `78152`, speed `904` and standing body produce retail Y
`113664` versus Rust Y `113920`: the first slope/sensor settlement divergence.
By tick 140, retail air speed is `(906,-181)` with ground speed `925`, while
the flat-ground kernel has `(960,0)` and `960`. The next native jump projects
its impulse along the slope, which this slice does not implement.

Consequently, the adapted runtime collects its first ring at tick 181 and
does not collect the retail second ring on this route. Retail collects two;
the current slice collects one. This difference is a known movement/collision
limitation, not evidence that either native ring position should be moved or
that the source layout has only one reachable ring.
