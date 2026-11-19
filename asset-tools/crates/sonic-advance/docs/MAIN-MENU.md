# English main menu (ASOE USA rev. 0)

Accepted Start on the title opens the five-item Rust menu in the browser. Up/Down
wrap, holding a direction repeats, and a fresh A confirms. A black fade ends in a
destination signal for Single Player (character select), Multiplayer, Time Attack
or Options. Tiny Chao Garden fades to white. The adapter stops at the destination;
it does not enter any of these scenes. Menu sounds and other languages are pending.

Source ROM SHA-256: afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020.
The menu keeps the title backdrop. Animation 779 has five English label scripts;
each row below identifies 36 bytes at a file offset. The renderer positions the
labels at (120, 93 + 14 × index), applying the decoded OAM anchor. Selected labels
use palette 178; the others use 179. There is no added cursor.

| Variant | Offset | Script SHA-256 |
|---|---|---|
| 1 | 0xF90CC | 47713db19ebc804a1e9e6f303ddadf279031f6c7eb055d70d393922cc64f0c28 |
| 2 | 0xF90F0 | a033fb30c9286c831fcefabccac6a5ecfa42e8536766c236a5ff63af656bdf20 |
| 3 | 0xF9114 | 3f8be11c69a0a0880bb446f94fbce0f9a8c84d48a2381c8cb5c58ecac14d049e |
| 4 | 0xF9138 | 1c2bca2c9686a5f25b273e3b648008ce8a3b4dee21d6aa7cfad6aa0917871596 |
| 5 | 0xF915C | 8c08dbf2001ead291c6d9a7dd6673524cf8ed65b4a9791f7008736feda35dc35 |

Behavior reference: SAT-R/sa2 at 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
src/game/sa1/menus/title_screen.c and src/core.c (GetInput). For independent
video evidence, tools/retail-replay.py runs the local ROM on the mGBA libretro
DLL with SHA-256 d775ecef6da1da5489ff2e0a37c13826b510c73711f89e78cb8d757c6f8c1426.

Both cold boots press Start at frames 60–61, 1050–1052 and 1250; Up at 1260;
A at 1320. Only the Options route holds Down at 1270–1314. All other inputs are
released. Run the script with LOCAL_ROM LOCAL_DLL and either --menu (Options)
or --menu-tiny. The two tests/retail-menu*.json fixtures contain only timings
and SHA-256 digests. Each digest covers a full 240 × 160 displayed frame packed as
little-endian BGR555. The sample at frame 1250 is the title before the transition
is displayed; menu state after n updates is compared with frame 1251 + n.
Each route samples 28 frames through 1369; unsampled frames are not certified.

The black fade matches the pinned emulator's RGB565 video reduced to five bits:
red and green round the scaled remainder down; blue subtracts a rounded-down
decrement. This comparison does not establish physical GBA display behavior.

Run cargo test --release --offline --locked for ROM-free logic checks; set
SONIC_ADVANCE_ASOE_ROM to the local ROM and run
cargo test --release --offline --locked --test main_menu -- --include-ignored
for pixel parity. The browser procedure in NATIVE-VALIDATION.md also confirms
Tiny Chao Garden via keyboard before switching to practice play.

Handoff: continue from the destination enum in src/title/menu.rs. The WASM host
owns the title-to-menu transition; the browser only sends buttons and renders.
Character select is the next unconnected Single Player scene. Coordinate its
ownership before editing the host; this slice does not modify stage movement,
flash/save logic, server authority, or the live preview.
