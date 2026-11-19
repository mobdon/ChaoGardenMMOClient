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
