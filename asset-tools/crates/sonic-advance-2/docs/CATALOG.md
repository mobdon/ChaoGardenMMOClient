# A2NE native catalog evidence

This catalog targets only the verified USA revision 0 cartridge and its verified decoded Tiny Garden payload. Cartridge offsets use base `0x08000000`; garden offsets use the decoded payload's load base `0x02000000`. A garden offset is not an offset into the compressed cartridge stream. The source and payload hash gates in the crate README apply before any range is interpreted.

The reference snapshot is SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Reference paths below are relative to that snapshot. Native source, assets and downloaded reference files remain local. The catalog contains independently recorded offsets, lengths, relationship descriptions and fingerprints; it does not redistribute the reference implementation or ROM contents.

## What is actually covered

`catalog/garden-ranges.tsv` records the reference's bounded `.incbin` ranges from `chao_garden/data/data.s` and `chao_garden/data/data3.s`. These are opaque evidence boundaries, not proof of a decoded format, a complete asset graph, reachability or unused content. `catalog/resources.json` adds overlapping semantic views. Every view receives a hash of the actual source slice at runtime. Never sum these overlapping ranges as a whole-ROM coverage percentage.

| Category | Source-backed slice | Implemented interpretation | Remaining boundary |
| --- | --- | --- | --- |
| Sprites and tiles | Main 4bpp pool; garden OBJ streams and composition records | Bounded tile pixel indexes, deterministic LZ77 output, original source bytes | Per-character ownership, VRAM allocation, complete OAM composition and native screen playback |
| Palettes | Main sprite palette bank; garden BG/OBJ banks and minigame palettes | Original BGR555 words and channel expansion | Runtime generated palettes, type-to-palette behavior and native display transfer function |
| Maps | Garden 22 by 20 map, compressed map groups; selected main BG/UI map groups | Original map words, tile index, horizontal/vertical flip and palette index | Main metatiles/collision, animated tiles, affine layouts, VRAM assembly and screen block ordering |
| Motions | Native 8-byte sprite composition records; main animation pointer index | Bounded original records and pointers | Animation command streams, frame delays, branches, complete action names and timing |
| UI | Garden glyph rows, numeric patterns, palettes and maps; selected main UI map | Exact retained bytes and shared bounded graphic readers | Character encoding, localized text layout, font ownership and complete UI state |
| Audio | Garden 37-row and main 507-row M4A song tables | Header fields, music player indexes, validated pointer starts and dummy headers | Target extents, sequence execution, instrument/sample graph, mixing and rendered audio parity |
| Minigames | Setup resources and lookup ranges in the garden payload | Exact source ranges and independently checked decoded bytes | Which unnamed setup owns every resource, complete native scoring/physics/reward rules |
| Saves and transfer | CRC table, default/name data, flash/length-command structures | Local bounded native compatibility inspection | Authentication, trustworthy progression, device pairing and live JoyBus session execution |

## Main-game sprite and audio relationships

`src/data/sa2/sprite_tables.c` defines six ordered pointers: animations, sprite dimensions, OAM data, palettes, 4bpp tiles and an 8bpp tile pointer. The matching six-word table is at cartridge offset `0xF40D4`:

| Field | Cartridge offset of target |
| --- | --- |
| Animation index | `0x135EC4` |
| Sprite dimensions | `0x14D1C4` |
| OAM index | `0x169C14` |
| Sprite palettes | `0x16ADC8` |
| 4bpp tiles | `0x1709A8` |
| Dummy 8bpp endpoint | `0x6E9E08` |

The unique adjacent 4bpp/dummy pointer pair was found in the actual ROM, then the complete six-pointer order was checked against the pinned declaration. `data/sa2/sprite_animation_data.s` independently identifies the 4bpp pool boundaries. Its length is `0x579460` bytes. The catalog's `0x80000`-byte tile pages are tool pagination at tile boundaries; they are not native animation banks. The dummy endpoint is not evidence of a native 8bpp asset collection.

The palette span `0x16ADC8..0x1709A8` is `0x5BE0` bytes, equivalent to 735 sixteen-color palettes. The active USA retail entries in `src/data/sa2/sprite_palettes.c` agree after excluding version/optional alternatives. The tool's pages of at most 256 colors are a resource limit, not a character ownership claim.

The main animation index at `0x135EC4` has 1,133 retail slots in `src/data/sa2/animations/animation_table.c`: 991 animation pointers and 142 explicit null slots. Labels range through `anim_1132` with null holes preserved. Optional decompilation-credit content is excluded. Following this index does not by itself decode the animation instruction streams or prove action semantics.

`sound/sa2/song_table.inc` identifies the main M4A table at native address `0x08AD4F4C`: cartridge offset `0xAD4F4C`, 507 eight-byte rows, length `0xFD8`. The garden's separate `chao_garden/sound/song_table.inc` table starts at native address `0x0201A4A4`: payload offset `0x1A4A4`, 37 rows, length `0x128`. Garden music uses entries 1, 2 and 4; entries 0, 3 and 5 through 10 refer to the shared dummy header. Other garden entries are effects. No audio waveform is reconstructed by a song-header parser.

## Main BG/UI map identity checks

The pinned `include/tilemap.h` describes a `0x1C`-byte descriptor: tile dimensions, animation fields, tile pointer/length, palette pointer/offset/color count and map pointer. The matching primary headers for `title_screen_bg`, `options_enter_name` and `zone_1/act_1/tilemaps/bg` specify dimensions and zero animation fields.

Candidate ROM descriptors were constrained by those fields, valid cartridge pointers and their declared palette → tile → map → descriptor ordering. Dimensions alone were not used as identity evidence: several candidates share them. Each final map's Git blob SHA-1 was computed over `"blob <length>\0"` followed by its actual source bytes, then compared to the corresponding file entry in the pinned local `sat-r-tree.json`. All three identities matched uniquely.

| Group | Descriptor | Palette offset/bytes | 4bpp tiles offset/bytes | Map offset/bytes | Dimensions | Map Git blob SHA-1 |
| --- | --- | --- | --- | --- | --- | --- |
| Title BG | `0x9E3B9C` | `0x9DE49C` / `0x200` | `0x9DE69C` / `0x4500` | `0x9E2B9C` / `0x1000` | 32 by 64 | `a4714d7a583f59066b831b1b6ca385b19dfa9da0` |
| Zone 1 act 1 BG | `0x935224` | `0x9335A4` / `0x200` | `0x9337A4` / `0x1300` | `0x934AA4` / `0x780` | 32 by 30 | `ed152fad765ae78e379ff848e352f2d271c94333` |
| Enter-name UI | `0x995250` | `0x994740` / `0x200` | `0x994940` / `0x460` | `0x994DA0` / `0x4B0` | 30 by 20 | `009d71b34de550db89dfbfca14b7c41bd58f77aa` |

The referenced raw map files are respectively `data/sa2/tilemaps/title_screen_bg/tilemap.tilemap2`, `data/sa2/maps/zone_1/act_1/tilemaps/bg/tilemap.tilemap2` and `data/sa2/tilemaps/options_enter_name/tilemap.tilemap2`. Their palette and tile ranges are identified by each verified descriptor's actual pointers. This establishes selected native assets, not a catalog of all stages, collision or UI.

## Garden graphic evidence

`chao_garden/src/game.c` defines the native eight-byte sprite composition record and its use in OAM assembly. Its renderer consumes the tile word, packed attributes, signed x/y offsets and continuation field. The tool exposes those fields without guessing an action name or duration. The `0x20494` pool and individual `0x205E4`, `0x205EC` and `0x205F4` records are referenced by the native routines; the complete action/script graph remains unknown.

`InitGarden` loads a `22 × 20` source map from payload `0x21BC4` and places it in a 32-word-wide screen with tile offset `0x200`. It also decodes the map variants at `0x24618`; selection starts at offset zero or `0x168` within the decoded output, each `9 × 20` words. A generic row-major renderer requires the caller to provide the appropriate tile base and crop; it does not silently infer the native VRAM composition.

`sub_02001c60` provides direct palette/VRAM load evidence for garden BG palette `0x233FC`, OBJ palette `0x246E4`, four BG LZ77 streams and two OBJ LZ77 streams. Palette banks at `0x248E4` and `0x24AA4` have fourteen and twelve sixteen-color rows respectively. Native bank selections above thirteen use generated RAM colors in the first case. This is not a verified Chao-type-to-palette mapping or proof of recovered Chaos Chao variants.

The type → palette/sprite/animation hypothesis in `TINY-GARDEN.md` remains open. The three desktop-model-derived phone Chaos atlases are explicitly new presentation assets; this catalog does not relabel them as recovered GBA art.

## Minigames, discrepancies and unknowns

The supplied A2NE source coverage is Chao Memory and Chao Bounce. RPS belongs to the supplied Advance source and Ball to Pinball Party. Search belongs to SADX's downloadable garden; its original payload has not been supplied. No offsets are borrowed across those versions.

Unnamed garden setup routines identify compressed tile/map resources, direct palettes, numeric display patterns and fixed layout tables. `garden.minigame.setup-d294.*` uses the exact routine address as its stable grouping because the complete Memory/Bounce ownership trace is not yet established. Likewise, the `0x15E10` range is a four-by-four byte pattern expanded into board graphics, not a verified score/reward table. Native physics, score curves, reward tables and runtime transitions are separate work.

One reference conflict is preserved deliberately: pinned assembly refers to payload `0x128A8` as an LZ77 source, but the verified USA payload bytes at that offset do not begin with BIOS type `0x10`. `garden.minigame.shared.setup-opaque` therefore remains opaque. It is not passed through the LZ77 decoder, and no output is invented. Reference region/layout divergence or another explanation requires further investigation.

Main stages/metatiles/collision, object entity tables, complete sprite dimension/OAM instruction relationships, runtime palette generation, all UI/localized text, audio sample decoding and every minigame semantic table are not exhaustively decoded. Opaque ranges remain extractable exactly and hashed. A successful build or asset conversion does not close these native-system gaps.

## Native versus MMO behavior

The readers describe original byte structures and selected source-backed relationships. The MMO's shared canonical Chao, authenticated handoff, exclusive activity lease, server-owned rewards, capped wallets, away-time training and desktop Chaos phone presentation are separate adaptations. A native save checksum or transfer length command proves neither identity nor legitimate progression. This crate does not modify live saves or grant MMO items, rings, stats or authority.
