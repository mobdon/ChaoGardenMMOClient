# City Escape native inputs, and the missing Sonic controller

`city_escape_native::CityEscape::read` collects City Escape's gameplay inputs
(Hero story, level 13) from the installed Steam data. It applies no adapted
rules, and it contains no player controller.

## Why there is no Sonic controller

The player controller is engine code, and none of it is readable from the
install:

| Source | What it holds |
| --- | --- |
| Steam `sonic2app.exe` | The PC player routines, for example PGetAcceleration at 0x45B610, PGetAccelerationAir at 0x45D770 and PGetFriction at 0x45F4D0, per the mod-loader declarations. The `.text` section is SteamStub-protected: a `.bind` section is present, and the bytes at those addresses have about 7.96 bits of entropy. This project does not decrypt or unpack it. |
| `resource/gd_PC/DLL/Win32/Data_DLL.dll` | Data. Its `.text` section is 0x8AA bytes and holds no player code. |
| `resource/gd_PC/*.rel` | GameCube stage, boss, kart and event modules. They call the player through the main executable but do not contain it. |
| GameCube `main.dol` (GSNE8P) | The GameCube player code. It is not part of the Steam install, and none was found on this machine. |
| roeming/SA2B at 874a480 | No player source. `config/GSNE8P/symbols.txt` names a few helpers (GetPlayerRunningSpeed at 0x800380AC, the playerpwp/playertwp arrays). |

A `main.dol` dumped from a GameCube disc you own would make the routines
executable under Unicorn, as the stage and boss modules are, so a Rust port
could be checked against them. Until then, no controller is provided for
speed stages.

## What is read

| Input | Source |
| --- | --- |
| Physics | PhysicsArray row 0 (Sonic) or 1 (Shadow), all 33 fields (`player_physics::read`) |
| Start | Single-player start row: (0, 300, 340), yaw 0x4000 as stored |
| Terrain | Mounted `objLandTable0013` Basic collision |
| Grind rails | The 12 paths whose callback is 0x4980C0 in the PC_SA2/STG13.ini path list at RVA 0xCDB35C: 147 points, with rotation words and lengths as stored |
| Death zones | PC_SA2/STG13.ini table at RVA 0xCDC678 |
| Springs | `spring::StageSpring` tasks for the 10 SPRA/SPRB placements (the C-verified callback) |
| Bush blocks | GREEN_B Basic collision at RVA 0xCAA350, placed by SET position and Y angle (18 placements) |
| Goal | The stg13D.rel goal-ring program and each GOALRING's native initialization route. Of the two placements, only the Rotation.x 0 one reaches contact. |
| Dash panels | One `dash_panel::DashPanel` task per KASOKU placement (82), running the callback ported below |
| Everything else | Raw SET rows (position, rotation words, Scale) by object name, including the 3 SAVEPOINTs. Their behaviour is not interpreted. |

`tests/city_escape_native.rs` checks these values on the pinned Steam build
(run with `--ignored`, `SA2_GAME_PATH` and `SA2_PC_CONFIG`).

## Dash panels (KASOKU)

`dash_panel` ports the KASOKU update callback from the stage module (City
Escape: stg13D.rel section 1 + 0x22C8C, 0x264 bytes). `Program::read` finds
it through the module's own object list, checks the instruction words
(relocated words masked) and all eight engine imports, and reads the
constants. The same callback is present in 56 installed stage modules.

Per update, unless the engine's clip gate returns nonzero:

- **Init:** mode 1, last slot 13, collision registered. A Scale.x of 0 or
  less becomes 14.0 and a Scale.y of 0 or less becomes 60.0.
- **Each later update:** a running timer counts down by one. On a player hit,
  the slot is 1 if the hit task is player 1's and 0 otherwise. The panel
  fires only if that player's status bit 0 is set (the engine's ground flag),
  and not again for the same slot while the timer runs. Firing passes local
  velocity (Scale.x, 0, 0) and the panel's three angles to the engine with
  the timer set from the converted Scale.y. It then plays sound 0x1003,
  rumbles the slot (4, 15, 0) and records the slot. Every update ends with
  CCL_Entry.

The engine services are boundaries the caller observes: the clip gate,
CCL_IsHitPlayer, the players' status words, and the value the timer
conversion (import 0x80110FA0) returns. How the engine turns the local
velocity and angles into player motion is part of the player code, which is
not available.

`tests/dash_panel.rs` (`--ignored`, with `SA2_GAME_PATH` and
`SA2_PPC_PYTHON` pointing at a Python with Unicorn) runs
`tests/dash_panel_ppc.py`. That script executes the installed instructions
for 64 randomized panels over 48 updates each, with stubbed engine imports.
All 3,072 updates (785 firings) match Rust: every engine call and its
arguments bit for bit, plus the mode, last slot, timer and Scale words.
