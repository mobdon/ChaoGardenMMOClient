# Original Chao activity motions

The installed PC game has now supplied **622 indexed Chao motion states, resolving to 279 unique native clips**, plus **19 Karate/Race stage-actor clips**. All **298 unique extracted files were decoded and converted** into eight separate local GLB action libraries. These are original motion data, rather than a new list of approximate movement poses.

## Sources and extraction

Run `node scripts/extract-chao-activity-motions.js`. Official X-Hax SA Tools release786 reads the original PC mappings and writes ignored `.local/extracted/activity-motions`:

| Native source | Confirmed extraction |
| --- | --- |
| `resource/gd_PC/DLL/Win32/Data_DLL.dll` | Export `al_motion_table`:622 states,40-node skeleton,279 unique `.saanim` files; original Race Omochao spectator model |
| `sonic2app.exe`, `CHAOSTGKARATE.ini` | Referee table, drummer/door motions and matching actor models |
| `sonic2app.exe`, `CHAOSTGRACE.ini` | Race spectator/crab tables and crab model |
| `al_karate_obj_tex.prs`, `al_race_obj_c_tex.prs` | Original support-actor texture maps, decoded from local working copies |

The extractor records SHA-256 before and after reading every installed input. All source hashes were unchanged. `ChaoMotionsD.rel`, `ChaoStgKarate.prs` and `ChaoStgRace.prs` are additionally hashed and inventoried as installed candidates; this PC extraction uses confirmed DLL/EXE mappings instead of assuming the separate REL data has the same address/endianness layout.

The full ignored machine-readable record is `.local/extracted/activity-motions/manifest.json`. It includes every common state index, canonical clip alias, native address label, extracted-file hash and original table properties. Native-style filenames such as `alm_run` come from the official SA Tools mapping; friendly descriptions are maintainer labels. The binary-derived animation labels are also retained, so neither is mistaken for a newly invented gameplay identity.

## Requested movement families

The entire common bank is retained, including less obvious clips outside these examples:

| State | Native mapped clip | Maintainer description |
| ---: | --- | --- |
| 119 | `alm_run` | Run |
| 122–124 | `alm_run_stopped` | Brake, with separate original state timing |
| 129 | `alm_dash` | Sprint |
| 130 | `alm_tobu` | Flying |
| 131 | `alm_tobiagaru` | Diving |
| 132–134 | `alm_jump_up` | Jumping up |
| 135–137 | `alm_jump_down` | Jumping down |
| 138–141 | `alm_habatobi` | Long jump |
| 143–144 | `alm_oyogu` | Fast swimming/gliding |
| 145 | `alm_oyogu_umaku` | Fast swimming with moving arms |
| 148 | `alm_bataashi` | Swimming |
| 149 | `alm_croll` | Fast crawl |
| 155–156 | `alm_noboru` | Climb |
| 157–158 | `alm_hardclimb` | Climbing up a ledge |
| 159/160 | `alm_koke` / `alm_kokeoki` | Trip and recovery |
| 161–162 | `alm_korobu_siri` | Falling backwards |

The manifest additionally identifies falling state271 and every jump/hop/fall candidate by its original label. The locomotion/race-traversal candidate group covers64 original states and35 distinct clips. The complete622-state bank remains the source of truth, since a filename search alone cannot establish every animation the race runtime selects.

## Karate and stage actors

The dedicated child/karate tail at states576–621 contains46 states resolving40 unique clips. It includes fighting stance, walking, hard kick, roundhouse, beating/punch/windup punch, damage reactions at three strengths, dodge variants, recovery, crawl/standing transitions, masked stance, champion/victory animations, and shared child emotes. States441–446 additionally preserve six generic combat clips. The manifest lists exact indices and all duplicate aliases; tail membership does not imply that every shared emote is a Karate attack.

Confirmed native examples include576 fighting stance,579 hard kick,580 roundhouse,582 punch,583 windup punch,585–587 damage levels,588 dodge,601/602 left/right dodge,603/604 recovery,605 crawl,607 crawl dodge,608/609 stand from crawl,610/611 masked stance,612 Grand Champion victory and613 child Karate victory.

| Activity actor | Original table/address | States | Unique clips | Skeleton nodes |
| --- | --- | ---: | ---: | ---: |
| Karate Omochao referee | `karate_judgeMotionTable`, EXE `4A5950` | 7 | 4 | 11 |
| Race Omochao spectator | `omo_originalMotionTable`, EXE `EE8398` | 7 | 6 | 9 |
| Race crab | `race_kaniMotionTable`, EXE `EF52A0` | 3 | 3 | 18 |
| Right drummer | EXE `EEC7EC` / `EEEE14` | 2 direct clips | 2 | 18 |
| Left drummer | EXE `EED02C` / `EEEEBC` | 2 direct clips | 2 | 18 |
| Right door | EXE `EED49C` | 1 direct clip | 1 | 1 |
| Left door | EXE `EED974` | 1 direct clip | 1 | 1 |

These19 support clips preserve their original actors and rigs. They are separate from the40-node playable Chao clips.

## Browser action libraries

Run after the normal native model extraction has prepared the child Chao reference model:

```powershell
$env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local/blender-user'
& '.local/tools/blender-5.1.2-windows-x64/blender.exe' --background --factory-startup --python 'scripts/convert-chao-activity-motions.py'
node scripts/verify-activity-motions.js
```

All outputs are under ignored `.local/imports/activity-motions`:

| Library | Exported clips |
| --- | ---: |
| `chao-actions.glb` | 279 |
| `karate-referee-actions.glb` | 4 |
| `race-spectator-actions.glb` | 6 |
| `race-crab-actions.glb` | 3 |
| `karate-right-drummer-actions.glb` | 2 |
| `karate-left-drummer-actions.glb` | 2 |
| `karate-right-door-actions.glb` | 1 |
| `karate-left-door-actions.glb` | 1 |
| **Total** | **298** |

Total GLB size is6,945,032bytes on the verified run. `conversion-report.json` records all action names, native-file hashes, frame ranges, reference FPS, original skeleton node count and output SHA-256. `geometry-report.json` records reference geometry/bounds. The verification command checks every expected action name, nonempty animation channels, all library counts and all298 native source hashes.

The conversion uses SAIO2.2.2 and Blender5.1.2. Blender5 action slots are bound explicitly during sampling so the active first action is retained; counts/names are verified against the actual exported GLB, not inferred from an import-success message. No existing live player/NPC, garden or scene GLB is replaced by this workflow.

## Timing, readiness and remaining integration

These libraries preserve raw actions on a60-sample-per-second conversion reference grid. The622 source states contain additional `StartFrame`, `EndFrame`, `PlaySpeed`, `LoopProperty`, `NextAnimation` and transition values. Several states intentionally share one clip but use different ranges, rates or transitions. The runtime must apply those original properties;622 states are not622 unique animation files.

Retargeting to existing Chao variants must match the shared40-node hierarchy/index mapping rather than assuming address-derived bone names match across every model. The stage props use their own1/9/11/18-node rigs. The export has been structurally verified; every one of the298 clips has not yet received a full visual gameplay review.

Extraction/conversion does not implement climbing collision, airborne movement, race AI, stamina timing, Karate hit windows, procedural facial expressions or the original activity rules. Broad animation selection/integration is a separate runtime task. All original files and derived content remain local and must not be committed to the public source repositories.

## Actual Chao Walker candidates

The installed game also contains genuine2P **Chao Walker** and **Dark Chao Walker** resources:

| Character | Model / motion / texture archives | Main model index |
| --- | --- | ---: |
| Chao Walker | `cwalkmdl.prs`, `cwalkmtn.prs`, `cwalktex.prs` | 442 |
| Dark Chao Walker | `dwalkmdl.prs`, `dwalkmtn.prs`, `dwalktex.prs` | 465 |

Official source maps are `PC_SA2/MDL/{CWALK,DWALK}{MDL,MTN}.ini`. The activity manifest includes complete mapped model/motion rows and installed archive hashes. Dedicated mech conversion/presentation is separate from the40-node Chao library.

Confirmed Dark Walker motion IDs include199 walk,200 jog,201 run,115 jump,116 falling from jump,96 fall,97/98 low/high landing,127 hover,133 power-laser firing,192 Dark Omochao activation and198 Dark Omochao attack. Original model/effect indices include267 Vulcan shot,478/479 laser layers,480 laser thrust,481/482 launcher/rocket and283 booster thrust. A native effect/model/clip is evidence of available source content; aim, lock-on, projectile and thrust rules still require implemented gameplay.
