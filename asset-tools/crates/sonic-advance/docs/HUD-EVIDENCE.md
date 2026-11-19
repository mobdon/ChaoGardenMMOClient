# Native single-player Sonic HUD

The ASOE reader loads original 4bpp OBJ tiles and their palettes from UI table records 0, 1 and 5 at 0x680A38. It draws the transparent sprites in native OAM order over the 240 × 160 stage framebuffer. Source art, colors and attachment positions stay in ROM.

| Resource | ROM offset | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Ordinary digits and separator | 0x658478 | 704 | 8f79fe746f1fd6c4142b40383761787d84708ebd450a404a47d737b5e7b27013 |
| Red digits and separator | 0x659C78 | 704 | af60786628d9293cbf003042ab869b16feff01f2b4cd6dd05c1ca40e2afdf69d |
| Sonic life icon | 0x65A098 | 128 | edc10c406f6dab9030f8e3e11d1a0cfc4daa3bbd7aa5f3f6f729d846c99e72a0 |
| Ring counter icon | 0x65A318 | 512 | e92c5c2d1dbb9adb2c007528f6d33d3759e28a36face04cac0171d1fccc566b2 |
| Sonic OBJ palette | 0x65A118 | 32 | 5d9049ae40bcad789f96dd2557a23500bb366eba3728f4007fc6c7e84b31bc5d |
| Final ring/digit OBJ palette | 0x65A518 | 32 | 45cdc5144f5dc133f8f4f1837814db8bb14991d91faad7cf513b9000ed249ff6 |
| Frame to centiseconds glyph pairs | 0x657EF0 | 120 | 1ff937880b062ee5e642607f945a09e560c9e637824af67c159fe9dd3d183fa9 |
| Seconds glyph pairs | 0x657F68 | 120 | 2174d928e937ed669535a9aa21845a0878756efdb51627fe4b32e2f54704342f |
| Zero-ring flash sequence | 0x658070 | 4 | 16ea092497732dd0726d532ab41c0699b85dd2cf85c2a3d144ad54468a70ab14 |

At native size the ring emblem is 32 × 32 at (-2,-1) and Sonic's 16 × 16 face sits at (2,144). Digit objects are 8 × 16. The score ends at X 73 and starts at Y -2. The timer starts at (25,12). Life reserves use (22,143), saturating at nine and subtracting the current life. Ring digits start at X 9, 5 or 1 for one, two or three digits. The zero-ring sequence changes every eight task updates. The timer uses the two native lookup tables, caps at 9:59:99 and selects the warning set at the native stage-frame cadence.

Two independent comparisons reconstruct the HUD from mGBA's actual OAM, OBJ VRAM and OBJ palette memory. The fixtures were captured at retail frames 1800 and 2399. A separate Python decoder read the raw attributes, traversed OBJ indices in reverse order, clipped signed screen coordinates, used 1D 4bpp tile addressing, skipped transparent index zero and expanded 5-bit RGB by bit replication. It drew over a uniform 0x01336699 sentinel so transparency errors affect the resulting 240 × 160 checksum. The Rust HUD received only scalar game state and the local ROM; it did not read the captures.

| Capture | OAM records | Course frames | Ring count | Changed pixels | SHA-256 of framebuffer, little-endian u32 pixels |
| --- | ---: | ---: | ---: | ---: | --- |
| 1800 | 0–11 | 26 | 0 | 1,112 | 059a3dc5b1545fbbb86a07d112e64a1c19a42ab6b46dbba77409f5ba230a92f0 |
| 2399 | 0–12 | 625 | 14 | 1,080 | 00dd58c64e2ce97127f3346154c51dae27f8aedda830a29188a997dcb078f584 |

Both captures had zero score and three lives (two reserves). All four graphics extents match their captured OBJ VRAM ranges byte for byte. HUD OBJ attribute records hashed 25f73761eea1b7eff4a9eb5e286f7a42cf90d97793e08605b7db673b088ba69a and f84b29c12dda7874bd4efbb48ed8063a27678aebb491f02d73d520e2d907b320 respectively. Separate tests cover red warning and flash phases, reversed timer, caps and invalid dimensions. The two scene comparisons establish normal-stage Sonic HUD pixel parity for those states; they do not cover other characters, special stages or every timer/score value. HUD drawing does not award score, lives or rings.

Pinned source is SAT-R/sa2 at 2dd3f17d30cd3c156e508aac0017620ae485b9ba. Stage calculations and graphics setup come from src/game/sa1/stage/stage_ui.c (c9679fbcc51ffb24e10d742ac86bd7ffab09eb52), the matching stage_ui__sub_8053BAC assembly (712d8e81864b069cc4afa65693ff5c12e6c3cd9c), src/game/sa1/stage/ui_rendering.c (c7337a6ebc5ce9a394e5bed21a0c07041124b8c0) and include/constants/sa1/ui_graphics.h (3130b450a6da1c89a0a7a192fc0d2ea9c8ec5a63). Run cargo test --test hud -- --include-ignored with SONIC_ADVANCE_ASOE_ROM set to the exact source.
