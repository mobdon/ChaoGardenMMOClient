# Neo Green Hill Act 1 opening

The Rust input lock follows the fresh solo Sonic route from character confirmation.
A new A, B or D-pad press at counter 36 through 214 moves the counter to 215
before increment. Start, Select and shoulder buttons do not skip. Movement unlocks
at 226. A held button cannot become a fresh press when the skip window opens.

Run tools/retail-replay.py with the fingerprinted local ROM and mGBA DLL from
MAIN-MENU.md and either --act-intro or --act-intro-skip. The latter presses A at
frame 1720. The fixtures contain [retail frame, x Q8, y Q8, movement flags,
full BGR555 video SHA-256]. The input lock matches every recorded movement flag:
228 frames on the natural route (unlock at 1883), and 75 on the skipped route
(unlock at 1730). The video hashes are references for further rendering work.

| Original resource | ASOE file range (exclusive end) | SHA-256 |
|---|---|---|
| Scrolling English zone name | 0x660558..0x660C58 | 1ee9fb773aa47fa74480dcc96cd8abec754bf63eec3a000bf1731aff472461fc |
| Zone-name palette | 0x660C58..0x660C78 | fd1d3059e46da78824451deeb4992e91f17618d6e2e48d9fa4427067c284564b |
| ACT 1 lettering and digit | 0x65EE58..0x65EFD8 | a5ae175fe8dc49659a29b1b3613afd61e0f3b833e2a430e4b4a73ce914f7a653 |

The native allocators placed these art blocks at OBJ tile 476 and 432 respectively
on this cold boot. Zone-name strips are 32x16; the six act-label objects are
8x16. Palette index zero is transparent. The Rust layers draw into an exact
240x160 RGBA buffer. Sampled retail OAM positions are checked in the unit tests.
Behavior reference: SAT-R/sa2 at 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
src/game/sa1/stage/stage_ui_z.c and the original Task_8055458 implementation.

Run cargo test --release --offline --locked --lib act_intro for clock and layout
checks. The remaining character-name, zigzag, headline backing, compositing and
browser handoff are open. The two decoded layers alone do not match a complete
retail video frame; the existing browser route still enters at the playable spawn.
