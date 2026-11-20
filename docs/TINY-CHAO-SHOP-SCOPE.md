# Tiny Garden shop: native evidence and remaining systems

November 18, 2026. This is a scoped shop/care inventory, supplementing `TINY-GARDEN.md`; it does not mark the full GBA games or their complete Tiny systems ported. Production purchase/care work belongs to the separate Rust core/server. No alternate browser simulation was added.

## Source and evidence boundary

The supplied A2NE USA revision 0 ROM has SHA-256 `f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260`. Its decoded Tiny payload is `.local/gba/mb-chao-garden-A2NE.gba`, 191,340 bytes, SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`. All offsets below are **decoded payload offsets**, relative to native load address `0x02000000`, not cartridge offsets.

Local primary reference files are `.local/research/sat-r/chao_garden_data_data.s`, `chao_garden_data_data3.s`, `chao_garden_src_game.c` and `chao_garden_asm_code.s`, pinned by the existing research record to SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Table values were read from the supplied fingerprinted payload; assembly call sites establish their use. The existing Rust A2NE reader lives in `.local/worktrees/sonic-advance-2/asset-tools/crates/sonic-advance-2`; its current semantic resource catalog does not yet name these shop tables. Existing opaque source ranges are not a completed shop implementation.

The isolated Rust inspection crate `.local/reviews/tiny-shop-scope-export` reuses `VerifiedRom`, bounded LZ77 and `graphics` from that reader. It produced **22 original static 16×16 sprites** in `.local/reviews/tiny-shop-static-sprites-a2ne/`. `provenance.json` records every output hash, tile/palette selection and source ROM hash. The optional `shop-sprites-review-8x.png` is a nearest-neighbor inspection sheet; individual subject PNGs retain native pixels and index-zero transparency. These outputs remain ignored/local and are not automatically web routes.

## Seven native fruits

`sub_02007978` reads a price by native item ID, checks the wallet and fruit capacity, then subtracts the price when purchasing. The seven `u32` little-endian prices occupy `0x12548`, length `0x1C`, SHA-256 `e9eaed988f3156a9b8c28f72dc48adac7c1735915afd280c7a2406529cc2dc19`. Native shop order `0x12541`, length seven, is `[0,1,2,3,4,5,6]`.

The effect table at `0x12510` is **seven signed seven-byte rows**, length `0x31`, SHA-256 `b33ae558a366ac031e7326d7ade4d3a38f3818f21c6139f6a97a8ceeaa3c49bd`. The following seven bytes are the order table, not an eighth fruit. Names below are visually verified colors of the original sprite; matching desktop `SA2BFruit` identities are a cross-game correspondence, not interchangeable native IDs.

| Tiny ID | Original subject | Rings | Mood | Belly | Swim | Fly | Run | Power | Stamina | Matching desktop ID |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 0 | Orange Fruit | 30 | 1 | 2 | 3 | -2 | -2 | 3 | 1 | 13 |
| 1 | Blue Fruit | 60 | 0 | 1 | 2 | 5 | -1 | -1 | 3 | 14 |
| 2 | Pink Fruit | 55 | 2 | 2 | 4 | -3 | 4 | -3 | 2 | 15 |
| 3 | Green Fruit | 50 | -1 | 1 | 0 | -1 | 3 | 4 | 2 | 16 |
| 4 | Purple Fruit | 30 | 1 | 2 | -2 | 3 | 3 | -2 | 1 | 17 |
| 5 | Yellow Fruit | 55 | 2 | 2 | -3 | 4 | -3 | 4 | 2 | 18 |
| 6 | Red Fruit | 70 | -3 | 0 | 3 | 1 | 3 | 2 | -5 | 19 |

These are **native ratings**, not ready-to-add MMO levels or total points. At assembly labels `0x0200493C` onward, the first two signed ratings are multiplied by 20 and clamped to 0–200; the five skill ratings are multiplied by 10 and applied to native progress gauges. Crossing a gauge threshold drives a separate level/grade/random-point routine. Luck and Intelligence do not occur in this seven-field fruit table. A 0–100 MMO need scale, total-point accounting, delayed eating and grade effects require an explicit Rust adaptation or further native parity work.

The exact raw tuples agree with local desktop `.local/extracted/care/Chao/FruitStats.ini` rows 13–19 after dividing its five skill entries by ten; `.local/research/SA2Enums.h` supplies the desktop names/IDs. This independent match and the original sprites confirm the color association. It does not establish identical lifecycle, favorite-food or transfer behavior.

Native fruit purchase code allows at most eight stored fruit entries and fills an unused byte in the save span starting `0x5A`, using `0xFF` as empty. The C save header still contains incomplete/unnamed fields, so do not reuse its provisional array declarations as an authoritative MMO inventory schema.

The initial `garden-fruit` at **3 rings** is a separately planned MMO item. The existing desktop Amy shop has its own 5-ring immediate fruit effect, and the older grove action costs 3 rings. None of these prices is the original seven-fruit Tiny table. Keep their catalog names, transaction paths and balance rules explicit.

## Eggs and toys

The native egg price table is `0x12564`, length `0x30`, SHA-256 `53168556e91578a45777528ddd9f9ce8ed0ae63bbdfc0afb8850ca8fdc055b2d`. `sub_02007B00` performs the indexed wallet check/subtraction and saves the egg ID at save-relative `0x59`. Stock selection in `sub_02007688` includes saved-state and random branches; a zero table price is not proof of unrestricted free purchases.

| Egg ID | Rings | Visually observed static shell; native item/material name unverified |
| --- | ---: | --- |
| 0 | 0 | Normal spotted shell |
| 1 | 1,000 | Gold/yellow appearance |
| 2 | 500 | Silver/white appearance |
| 3 | 5,000 | Pink |
| 4 | 7,000 | Blue |
| 5 | 10,000 | Green/mint |
| 6 | 8,000 | Purple |
| 7 | 14,000 | Cyan |
| 8 | 12,000 | Red |
| 9 | 20,000 | Gray/dark purple |
| 10 | 16,000 | Yellow-green |
| 11 | 18,000 | Orange |

Do not infer jewel genetics, shiny flags, hatch results, rarity or desktop egg codes from this palette list. A persistent MMO egg instance, stock policy and hatch result must preserve the same canonical Chao rather than replacing it silently.

The toy prices are **three**, not six, `u32` entries at `0x12594`, length `0xC`, SHA-256 `0935002e4c7afc324f5aca05efcb6b3d674834a5cd1e6391f2fdf7e66c940909`. The next twelve bytes at `0x125A0` are native composition pointers and must not be interpreted as additional prices. `sub_02007C4C` applies the wallet check/subtraction; `sub_02007688` gates the next offered toy by saved toy state and a countdown.

| Native toy ID | Original sprite, visually verified | Rings | Remaining behavior |
| --- | --- | ---: | --- |
| 0 | Trumpet | 1,000 | Native use/action timing and shared ownership |
| 1 | Rubber Duck | 2,000 | Water/use behavior and saved placement |
| 2 | Television | 8,000 | Availability countdown, playback/reactions and saved placement |

Purchasing toy ID 1 sets native field `0x14` to `0x34BC0`; the garden routine decrements it and checks it before offering toy ID 2. Keep the verified timer/gate distinct from a claim about real-world/offline hours. No native offline training or passive reward mechanism is established by it.

## Exact art path and export recipe

`sub_02001C60` loads OBJ palette `0x246E4` (512 bytes) and expands the OBJ stream at `0x268B4` (3,812-byte catalog span, 6,144 decoded bytes) to OBJ VRAM offset `0x1580`. Its compressed-span SHA-256 is `fe67f7c742546face674d447daadd7f7df8cba1c7f7c14ec3a8c32ab0aa14e8a`. Static shop constructors all reference composition record `0x205E4` and the native 16×16 footprint; retain source tile ordering and transparency.

| Subjects | Constructor | Native tile selection | Palette selection | Local output filename pattern |
| --- | --- | --- | --- | --- |
| Seven fruits | `0x02008B3C` | `0xBC + 4 × ID` | `0x20EFC` banks `[2,3,5,4,3,4,2]` in `0x246E4` | `tiny-native-fruit-{0..6}-a2ne.png` |
| Twelve eggs | `0x02008B7C` | `0xD8` for ID 0, otherwise `0xDC` | Per-ID 16-color rows at `0x24AA4 + 32 × ID`, loaded into native bank 14 | `tiny-native-egg-{0..11}-a2ne.png` |
| Three toys | `0x02008BC4` | `0xE0 + 4 × ID` | `0x20F03` banks `[5,7,6]` in `0x246E4` | `tiny-native-toy-{0..2}-a2ne.png` |

The source-owned Rust exporter is now under `asset-tools/crates/tiny-shop-presentation`. It verifies the complete supplied ROM with the existing Rust reader, pins the selected spans and all 22 PNG hashes, and preserves the exact sprite pixels/alpha. Its conventional dependency `../sonic-advance-2` is included in this client source delivery from native asset commit `1a46b2f`; the focused exporter and reader tests pass in this layout. This static exporter is not a complete shared game engine.

### Garden and Widget Stats tabs

The isolated phone preview now shares one native-font sidebar and a clickable Stats tab across Garden and Widget. Both start closed. Opening the tab places the stats beside the complete garden image, without overlaying the Chao; closing restores the wider garden. The Widget card is a section so its Stats button is independently accessible, with Open remaining a separate action. The original garden aspect ratio is preserved in either state.

Browser checks on November 18 verified both tabs against the isolated Rust profile, including the received 928-ring balance and all seven stat labels. Preview source is built from native client commit `ac7399b7b03dbc9e52fd4a7fe3dba965347ff483` with the current presentation adapters. The exact-route preview tests and focused shop/resident/flame tests passed (15 cases), and both preview and client production builds passed. This browser layout change does not update an installed Android widget.

### Native sidebar font presentation

The same exporter now recovers original ASCII glyph tiles from A2NE payload LZ77 stream `0x220EC` (2,904 bytes, 10,784 decoded bytes/337 tiles), loaded natively at BG VRAM byte `0x55E0`. Original text addresses glyph code `ASCII − 32`. The native English sidebar's palette `0x233FC` binds glyph indices 1/2/3 to original palette-zero indices 9/7/5; background index 4 becomes transparent for compositing. Palette colors are preserved from source, not a desktop-font approximation. Original horizontal ink bounds plus one pixel of spacing exactly reproduce the complete baked Mood, Swim and Stamina label rectangles from UI stream `0x235FC` and English layout in `0x24618`. Other baked words have individual spacing differences and are not claimed as full-word parity.

Luck and Intelligence are **newly composed words using those original glyph pixels**, not recovered native hidden-stat labels. They retain the original eight-pixel baseline. The output also includes a 128×48 atlas of ASCII 32–126 in 8×8 cells/sixteen columns. Render at integer scales with nearest-neighbor filtering. Their presentation does not introduce hidden-stat gameplay or change Rust authority.

| Output | Native size | PNG SHA-256 |
| --- | --- | --- |
| `tiny-native-label-luck-a2ne.png` | 20×8 | `3f8ddd0ee9462e497137a9fd2ca70b29266104e05fb0acff820d52bffe263284` |
| `tiny-native-label-intelligence-a2ne.png` | 51×8 | `b9fbee9e35fc0dbeb2ffd392526fe720a618e8d4bc9631bdce45f1bbc8e415cb` |
| `tiny-native-sidebar-ascii-a2ne.png` | 128×48 | `233baf8c9db6559fbc6cb465b568a851cabb8e2d07d2606392d7d7ceeb0e94e7` |

Evidence: `.local/reviews/tiny-shop-presentation-validation/.local/font-a2ne-final/provenance.json`. Four Rust tests passed with the supplied A2NE, original 22 shop outputs, all three font output hashes/dimensions/original colors/binary alpha, native-label pixel comparisons and rejection of actual ASOE/A3VE variants. Strict all-target Clippy passed. No native bytes were added to tracked source. UI mounting and browser/service acceptance belong to the parent client workstream.

The current native phone preview's original thirteen-asset manifest contains garden, English sidebar, Chaos atlases, flame and emeralds. It has no shop subjects. Add reviewed subject PNGs by **exact filename and hash** to the local snapshot asset catalog; a possible presentation namespace is `/phone-tiny/__tiny-shop/`, explicitly **not connected by this document**. Keep originals and manifests local/ignored. Never override the global `/api/assets` routes with these phone-specific subjects.

Existing installed SA2 assets can supplement presentation where the matching Tiny artwork is absent:

- `.local/extracted/chao-ui/al_tex_common/panel_fruit.png`, `panel_egg.png`, `panel_seed.png`, `panel_item_sp.png`: original 32×32 category icons, with source/output fingerprints in `.local/imports/chao-ui-provenance.json`. They are desktop category icons, not native Tiny fruit pictures or evidence of Tiny seed mechanics.
- `.local/tools/sa-tools-786/GameConfig/PC_SA2/ChaoCommon.ini`: native fruit models including `NeutralGardenFruit.sa2mdl`, colored fruits and their texture binding `AL_OBJECT`; native egg/hatching models with `AL_BODY`/`AL_CHILD02`; toy models with `AL_TOY`. Listed model availability does not mean these models are converted or currently exposed to Tiny.
- Existing extracted race rewards under `.local/extracted/activities/Chao/stg_race/Rewards/` include Rattle, ToyCar, Book and Normal/Hero/DarkBall. These are desktop race reward assets, not proof that they are additional Tiny shop stock. The Tiny trumpet/duck/TV pixels above should be preferred for those exact subjects.

## Systems still requiring explicit implementation or verification

| System | Evidence now available | Outstanding work |
| --- | --- | --- |
| Shopping | Native seven-fruit/twelve-egg/three-toy tables, purchase code, static subjects | Accepted Rust catalog parser, server capabilities and atomic durable buy/consume with bounded intents; distinguish native/MMO stock and prices |
| Feeding and progress | Signed fruit rows, gauge multipliers/caps and level-up call path | Native gauge/grade/point/random behavior, food refusal, animation timing and chosen MMO mapping |
| Toys | Three shop subjects/prices and saved-state/countdown gates | Full actions, placement, ownership, persistence and nonrewarding resident AI interactions |
| Eggs | Twelve prices/palettes and native occupied-egg/stock branches | Canonical egg instances, eligible hatching, type/genetics/material mapping and safe Chao ownership policy |
| Ring/item/Chao transfer | Existing bounded A2NE save/transfer readers | Exact item mapping, successful/failed handoff, once-only service transactions; native checksum never authorizes MMO gains |
| Advance USA / Pinball Party | Verified supplied ROMs and existing bounded asset readers | Separate native shop/care tables and presentation audits; A2NE offsets and prices cannot be copied across versions |
| Seeds/trees/animals/drives | Desktop sources and separate care workstream | No claim that they are native Tiny shop stock; agreed MMO catalog, effects and inventory paths |
| Five minigames | Existing Rust authoritative adaptations and sanitized browser views | Full native physics/art/reward parity and remaining touch/playthrough acceptance |
| AFK training / away competitions | Requested MMO extensions and separate Rust workstreams | Durable service jobs, ownership/activity policy and actual round trips; local sprite export adds none of these mechanics |

Production fruit parser, catalog IDs and mutation ownership were handed to the Rust shop/core agent. The proposed server namespace `tiny-fruit:0` through `tiny-fruit:6` is an MMO identifier; the original native ID remains the numeric row above. This document and the local art inspection do not deploy a service, migrate saves or mark gameplay accepted.

## Phone shop shelf, November 18, 2026

The local QA phone uses the actual A2NE 48×160 shop panel (task 02007500, tile 115h, 12-part descriptor 0202063C, OBJ palette bank 15). Item anchors are (8,26+16×slot) with the native (-8,-15) offset. Original number tiles 141h–14Ah use bank 1, with right-aligned six-pixel spacing from descriptor 0202069C. The game has seven fruit rows and optional egg and next-toy rows. For a fresh local export use the tiny-game-presentation CLI with --garden-rom and --output; point TINY_PHONE_ACTOR_ART_DIR at that new directory. Both PNG hashes are in garden-actors.json. Native bytes stay ignored.

Tapping a row inspects it. The full-size detail buttons, expanded bag/collection, and hourly egg rotation are MMO features; the panel itself and its stock art are native. Every purchase still uses the existing durable Rust intent. Normal Egg (native ID 0) is free when offered; the server uses the occupied-Chao native path: about three quarters ordinary-table rolls times about one fifth normal. Each owner's hourly offer is stable, and possession survives rotation until explicit hatching. No offline purchases.

On disposable Garden QA on port 5182, the seven fruit rows, 5,000-ring egg and 1,000-ring trumpet appeared together. A 3-ring adapted Garden Fruit purchase in All items changed the wallet from 20 to 17 and the bag from 0 to 1; reload kept both and the diary entry. Twelve source-enabled Rust egg tests also passed, including duplicate purchase/hatch, insufficient funds and restart. This is a browser preview; no device or live-save rollout.

November 18 phone QA follow-up: the shelf is the primary full-width purchase surface with intact native edge pixels, original number sprites, inline actions and a text-only Back to garden control. Inventory is a separate compact view. Weeds render into the ground pass before Chao and toys; accessible pull targets submit the same Rust intent. Garden food selection reads the actual bag and submits the existing consume intent. The Garden QA test consumed one fruit, disabled Feed on an empty bag, and recorded the action in the server diary. Four owner Dark Chaos equipment variants now bake the native cyan emerald onto torso joint 001 and sample the original SA2 pet reaction. Ring aura is still a separate layer.
Retail weed correction: A2NE 02004540 reads the active weed count at 0300159B. Above three weeds it decrements a timer, then subtracts one Mood byte (save offset 73h, floored at zero); interval is 60 * (12 - floor(count/2)) eligible garden updates. Pulling has no direct ring reward. This indirect Mood rule has not been integrated into the MMO care authority. A2NE weed descriptor 020205E4 at tile 109h offsets (-8,-15) from the saved ground anchor. The preview now exposes all 176 x 160 garden pixels. The adapted three-ring fruit uses an icon from original SA2 Neutral Garden Fruit model F29944 with AL_OBJECT textures.

### Original resident needs and feeding
Rust source in the independent server now connects A2NE 02004418–02004578 to eligible Tiny resident simulation frames. Belly declines; high Belly can raise Mood, low Belly can lower it, and four or more weeds introduce their own Mood loss. Half a canonical need point is one original byte. Below four weeds the weed penalty stops; pulling is not itself a reward. The saved countdown survives restart. A2NE 02007E80 also checks whether the signed Belly rating times ten fits before accepting an offered fruit. Refusal keeps the bag intact. The separate eating-effect multiplier is twenty.

Verified on disposable Rust sessions with the supplied A2NE payload: four weeds caused a Mood drop, pulling back to three stopped that penalty, and a full Chao kept refused fruit across reload. These are source changes, not a production deployment. Remaining original parity includes active-profile fruit experience/grade random growth, full feeding and toy animation sequences, original transfer transaction coverage, per-cartridge differences and the missing SADX Search payload. The original growth function exists in Rust; active-profile fruit skill rewards are still mapped MMO points.
