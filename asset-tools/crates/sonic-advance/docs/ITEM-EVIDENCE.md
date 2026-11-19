# Native single-player item boxes

The single-player ASOE placement graph supplies monitor coordinates, item kind
and region ordering. The normal case is animation 705 variant 0 (OBJ order
18). Icon variants come from the 11-byte table at 0xB84E0 referenced at
0x1E784. Animation 706 (order 19) displays the selected icon behind the case.
Both are decoded using the existing native sprite VM. A break plays sound
167 and displays native dust animation 407 variant 0 (order 15).

At SAT-R/sa2 revision `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, see
`src/game/sa1/stage/itembox.c`, `src/game/shared/stage/dust_cloud.c`, and
`asm/non_matching/game/shared/stage/{item_box__Task_ItemBoxMain,collision__Coll_Player_Itembox_sa1,collision__sub_800CBBC}.inc`.
The matching assembly dispatches on the attack hitbox in the Sonic sprite
info block at offset 0x3C (sprite base +0x0C, second hitbox). The C main-task
approximation describes a different slot. Inactive attacks use the normal
platform contact resolver. Active attacks use the real VM hitbox, can burst
a box and reverse downward velocity. An underside collision can bump the
intact box; motion uses the native single-tile floor probe and gravity of
40/256. The shell disappears on break. The icon rises one pixel per frame
for 60 ticks, emits one grant, stays for 30 additional ticks and disappears.

The Rust module emits grants for a life, normal/magnetic shield, invincibility,
speed shoes, fixed five/ten rings or random rings. Random ring awards consume
one draw from the caller's game RNG. The six native amounts are 1, 5, 10, 20,
30 or 40. This module covers standard single-player boxes. Time Attack's
one-up exclusion, multiplayer item selection and multiplayer grants are not
implemented.

## Retail comparison

Two mGBA captures used local ASOE SHA-256
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
In both, Sonic starts at synchronized frame 2549, approaching a five-ring
monitor centered at (2536,360) in Neo Green Hill Act 1.

- Continuing the regular exercise inputs from frames 2550--2707 reproduces
  all 158 frames of position, air X/Y speed, ground speed, radius, angle
  and layer. SHA-256 over eight little-endian i32 fields per frame:
  `e2e708fe36899b16c9ef83245cd471a2c08926fc7fc7482f897d594fe5bd87b3`.
- Releasing the controls at frame 2640 brings Sonic down onto the monitor.
  Retail reverses his vertical velocity on frame 2676 and grants five rings
  on frame 2736 (17 to 22). Rust matches all 225 frames from 2550--2774
  across those eight fields plus ring count. SHA-256:
  `a27ceb619c41538766caad209e15c2e69943363be0ddfaa82e0fc0c3d74031b1`.

A separate actual-ROM test asserts the icon variant, dust/break event, bounce,
grant timing, final icon offset, removal and absence of repeated grants.
Run `cargo test --offline --locked --test items -- --include-ignored` with
`SONIC_ADVANCE_ASOE_ROM` pointing to the private ROM. All game bytes, graphics
and raw capture data stay local.
