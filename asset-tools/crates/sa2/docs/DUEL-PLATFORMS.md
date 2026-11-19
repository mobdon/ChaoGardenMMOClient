# Sonic vs Shadow 2 platforms (story level 42)

Level 42 has no landtable. Its floor is built at run time by two objects in the
installed GC module `resource/gd_PC/stg42D.rel` (SHA-256
`51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d`, module 41):

| Object list row | Code (section 1) | Role |
| --- | --- | --- |
| 24 `DODAI` | +0x1D68 | One platform: copies the Basic object at section 5 +0x13C8 and registers it as dynamic collision. |
| 25 `DODAIGEN` | +0x2278 | Generator placed once by `SET0042_S.BIN` (record 0, at the origin). |

The REL object list at file offset 0x2682C (SA Tools `GC_SA2B/stg42D.ini`) has
the same 27 names as the PC list at RVA 0x12546E8. The PC executable's code
section is not readable as plain instructions, so the PowerPC module is the
behavior source. The REL template is geometrically identical to the PC
collision object at RVA 0x1251FA0 (24 vertices, normals, bounds, material,
strip order and winding), which SA Tools already corroborates.

## The platform

A 300 × 15 × 100 box, X −150..150, Y −10..5, Z −50..50 in its own space, 24
vertices in one strip meshset. DODAI copies the object, sets the copy's
position and angles from its work area, enlarges the copied model's bounding
radius by 15 (to 173.29) and registers it with flags `0x28000001` (solid,
dynamic collision, size class 0x20000000 for radius 100..200).

## The generator

Per update, with work fields `+0` mode, `+4` flags, `+6` frame counter and
`+0x14` cursor X:

1. Mode 0 sets the cursor to 0.0 and enters mode 1.
2. Mode 1, if game byte +0x42 is set: spawn one platform at X −300 with timer
   0x8000 and enter mode 2. The authored end rows (Sonic −308, Shadow −292)
   stand on it.
3. Otherwise, for each non-null player-table slot (0x801E7368) in order, while
   `cursor − player.x < 2100`: spawn DODAI at the cursor, set its timer to
   `trunc(max(0.4 × (player.x − cursor), −3600))` (low 16 bits), and add 300
   to the cursor. After the deadline flag, each new platform instead gets
   timer 900 and its activation byte set.
4. Unless game byte +0x37 is set, increment the frame counter; above 36000
   it stays at 36000 and sets flag 0x1000.

DODAI starts to fall on the first update whose signed timer exceeds 900. When
the first slot of the player data table (0x801E7328, field +0xA0) references
an inactive platform, that platform is activated with timer 720; an activated
timer counts up on updates where game byte +0x37 is clear and +0x42 is not 1,
so it falls about 180 updates later. The fall itself and the meteor objects
are not ported here.

Consecutive platforms abut, forming one deck along +X. With both authored
start rows at X 0 (Sonic Z −15, Shadow Z +15), the first two updates spawn
seven platforms from X 0 to 1800, a solid deck from −150 to 1950.

## Rust interface

- `duel_platform::read_rel(rel)` reads the template from the fingerprinted REL.
- `duel_platform::generator(rel)` reads every generator and DODAI constant
  through the relocation or instruction that consumes it.
- `Generator::step(state, frame)` replays one DODAIGEN update. It refuses an
  update that would spawn more than 4096 platforms; at very large X the native
  loop would never end.
- `duel_platform::start_terrain(rel, players_x)` returns the opening platforms
  and their world-space collision triangles.

```powershell
sa2-assets duel-terrain <game>\resource\gd_PC\stg42D.rel .local\out\terrain42.json <game>\sonic2app.exe
sa2-assets duel-generator <game>\resource\gd_PC\stg42D.rel .local\out\gen.json REQUEST.json
```

`REQUEST.json` is `{"frames":[{"players":[x|null,x|null],"resultMode":false,"paused":false}, ...]}`.

## Verification

```powershell
$env:SA2_GAME_PATH = '<Steam>\Sonic Adventure 2'
cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml --test duel_terrain -- --include-ignored
python asset-tools/crates/sa2/tests/duel_generator_ppc.py asset-tools/crates/sa2/target/debug/sa2-assets.exe
```

The second command needs Unicorn 2.1 on `PYTHONPATH` and must run from the
repository root (it writes temporary requests under the ignored `.local`).
It relocates DODAIGEN's own code and runs it under Unicorn with stub
allocator and register-save imports. Over a 36,800-update race (pause,
missing player slot, a 12,000-unit jump, the deadline), the result-platform
path and a timer-wraparound case, every update's mode, cursor bits, flags and
counter and all 3,259 spawned X bits, timers and activation bytes match
`duel-generator`. The stubs show the generator never writes a spawned
platform's Y or Z. Allocation failure, the engine allocator itself and the
DODAI update are not executed. Nothing launches the game.
