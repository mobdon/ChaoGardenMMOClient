# Running a story from the local PC installation

Run the headless Rust dispatcher from the checkout. It reads the authored sequence, loads consecutive opening event images, validates each event timeline at its final frame, and leaves the first unhandled action pending. The JSON and dependency manifest stay under the checkout's ignored .local directory. Use a new output filename on subsequent runs.

    sa2-assets story-run GAME/sonic2app.exe .local/hero.json PC_SA2/ADVERTISE.ini "Hero Story Sequence"

| Sequence | Installed events inspected (frames) | First pending action |
| --- | --- | --- |
| Hero | 0 (1,655) | 1: level 13, City Escape |
| Dark | 100 (1,189) | 1: level 11, Iron Gate |
| Last | 200 (866), 201 (1,169) | 2: event 602; no matching main event PRS |

This is a content run without drawing, elapsed-time playback, or Chao controls. No story clear is acknowledged. Controlling a Chao in City Escape still needs verified movement, collision response, stage objects, mission completion and a presentation adapter. A fan-game growth rule and once-only durable reward belong to the canonical Rust server; the dispatcher neither reads nor writes a save. Event 602 needs a verified alternate event handler.

## Paired City Escape poses

    sa2-assets city-rider-pose STAGE .local/rider.json GAME/sonic2app.exe GD_PC/sonicmdl.prs 121 3.25

STAGE accepts the pinned stg13D.rel or Data_DLL.dll. Each has 24 original Sonic motion slots (121–144) paired with 24 board slots (145–168); aliases reduce each bank to 21 distinct headers. The returned model-local poses preserve the original Sonic model 0 hierarchy and streetboard. This original Sonic animation is not a Chao animation or a downhill movement controller. Angles, translations and scale stay in source units. Sampling does not modify textures.

Run tests/board_motion_reference.ps1 with -Rider (with -PcDll for the DLL) to independently digest the 38,366 ordered rider keys per source using SAModel. The installed tests compare the hashes for both formats, the split model's hierarchy and transforms, and all 24 executable action records against the SA Tools split Sonic Animation List. For the latter point SA2_RIDER_REFERENCE at the private reference directory containing split/sonicmdl/0.sa2mdl and actions/figure/sonic/Sonic/Animation List.ini. Generate those with SA Tools split mdl on sonicmdl.prs and split binary using the Sonic Animation List section of PC_SA2/Characters.ini respectively.

The character-animations command reads all 16 native character animation lists (3,154 rows) from the pinned Steam executable. Each array index is an animation state selector; it does not identify the player action that selects it. Rows preserve motion and model slots, mode, next animation, transition speed and playback speed. Table and row addresses and table digests are included. City Escape uses the same reader for its known Sonic board states. Runtime overrides, controller state changes and motion-bank resolution for other characters remain open.

To reproduce the independent check, run `sa2-assets character-animations EXE OUTPUT.json` with a private output path, then `tests/character_animations_reference.ps1 -GameExe EXE -ToolsRoot SA_TOOLS_786 -RustJson OUTPUT.json`. The check resolves Characters.ini addresses with .NET PEReader and compares every row with SplitTools.SA2AnimationInfo.

Character MTN archives: `character-motion-bank MTN.prs OUTPUT.json` indexes each physical slot, including aliases and duplicate IDs. `character-motion MTN.prs OUTPUT.json SLOT` reads native position/rotation keys for layout 0x0003/2. It rejects ambiguous slot IDs and other channel layouts; scale, morph and camera layouts still require separate decoding. Directory offsets refer to the decompressed big-endian file. The output receipt hashes its compressed input.

`sonic-state-pose MTN.prs OUTPUT.json EXE SONICMDL.prs STATE FRAME` samples the pinned Sonic rig from a supplied animation state and frame in either the common or Sonic bank. It checks the selected model and part count, and does not resolve stage overrides or advance the player controller. Installed examples: state 0 / frame 0 in plcommtn.prs and state 87 / frame 0.5 in sonicmtn.prs.

For the local regression, run SA Tools 786 splitMTN on a private copy of sonicmtn.prs, set SA2_MTN_SPLIT to its output directory and SA2_SET_DIR to gd_PC, then run `cargo test --locked --test character_motions_local -- --include-ignored`. All 34 Sonic directory rows (32 unique motions) matched the split offsets and every decoded frame, position and rotation value. SA Tools normalizes some negative zeroes on export; Rust retains the original bits. The directory test also reads the other 16 installed banks and checks the ambiguous Amy slot.
