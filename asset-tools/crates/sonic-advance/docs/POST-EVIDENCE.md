# Single-player stage posts

Native map kind 7 uses checkpoint animation 452 and kind 0 uses goal animation 453. Both allocate OBJ priority 2 and order 18. The runtime reads tiles, palettes, animation variants and hitboxes from the local ASOE source. The post helper returns world-space anchors for the stage compositor.

The checkpoint's first retail image is 16 × 48, anchored at (8, 43), with hitbox (-7, -44) through (8, 5) inclusive. Its uncomposited RGBA SHA-256 is c87ad319558df2061a82dccfd90b9fac1f03b2fbd8453dd9f19fb49e7cf44f07. Goal variant 0 is 48 × 40, anchored at (24, 38), with RGBA SHA-256 05f38963b5e89ee3f8a0402be78cbd43723393926f8c2b01f056f005178be50e. These are Rust render regression hashes from the retail data. Independent emulator captures of those images remain outstanding.

A checkpoint samples the integer player center. On activation it records the signed checkpoint index, world X, world Y less the current player height offset, and the stage checkpoint clock. An interactable override blocks activation. Variant 1 advances to variant 2 on native animation end. Returning within camera range reconstructs the active variant from the recorded index. Egg Rocket (stage 10) has invisible checkpoints: index 1 requires X strictly past the placement, other indexes require the player's bottom strictly above post Y + 4. Its stage separation and launch effects remain open.

Single-player goals require a nonzero second parameter. The first parameter controls visibility independently, so crossing an invisible trigger still starts the finish sequence. The idle goal displays its creation frame until the player crosses strictly to its right. Stage 7 also requires Y strictly below the line 80 pixels above the anchor. Stage 10 additionally requires the native body hitbox to include the anchor. The initial goal task has a nonmatching C reconstruction; its single-player gates were compared to the matching assembly at 0x0801F4D8–0x0801F572.

The goal owner records elapsed task calls. After 30 prior calls, the regular-stage results helper can request victory braking when grounded and moving at least 2.25 pixels/tick. The native goal task creates results after more than 120 prior calls if character state 28 has been reached (unconditional for Egg Rocket). The local signal tracks the brake request as the state-28 condition. Input forcing, speed capping, stage completion UI, results presentation, camera shifts, music, task interleaving, boss transitions and save effects require their respective runtime systems. Egg Rocket launch/separation and stage flags, special boss logic, multiplayer goals and complete retail-result execution are outside this module.

The reference is SAT-R/sa2 at commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba.

| Source | Git blob |
| --- | --- |
| src/game/sa1/stage/interactables/checkpoint.c | f1adab9b95176ccb2c11b39d2ef52bf5601de93a |
| src/game/sa1/stage/interactables/stage_goal.c | 1196f819976a21db38e0a511deaf20d785e125b1 |
| asm/non_matching/game/sa1/stage/interactables/stage_goal__Task_StageGoal.inc | 9f249945ca3a3db63ad093f57e8a279a6a836340 |
| src/game/shared/stage/stage.c | e42773050528c39ecf3493784ddf0957f25deee4 |
| include/game/shared/stage/camera.h | 85c3075383ca2be6695a7f4d324e31e3c0fd8133 |

Run cargo test --test posts -- --include-ignored with SONIC_ADVANCE_ASOE_ROM set to the exact local source. The tests exercise source graphics, checkpoint reentry, inclusive and strict edges, script override, hidden goals, mode filters, Egg Rocket body bounds and results gating.
