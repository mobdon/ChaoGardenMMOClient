# Run the Rust campaign and stage tools

The CLI follows a selected entry in the original PC campaign tables into an actual
event or stage. It emits the camera or scene state for an explicitly selected frame.
This is an offline preview: selecting a later action seeks in the table. It does not
mark the preceding stage cleared, change a save, or advance an MMO account.

From the client worktree, with Cargo on PATH and SA Tools 786 installed locally:

```powershell
cargo build --locked --manifest-path asset-tools/crates/sa2/Cargo.toml
$tool = 'asset-tools/crates/sa2/target/debug/sa2-assets.exe'
$game = 'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2'
$exe = Join-Path $game 'sonic2app.exe'
$config = 'C:/path/to/sa-tools-786/GameConfig/PC_SA2'
$story = Join-Path $config 'ADVERTISE.ini'
$gd = Join-Path $game 'resource/gd_PC'
& $tool --source-root $game campaign-camera $exe .local/hero-camera.json $story 'Hero Story Sequence' 0 995 0
& $tool --source-root $game campaign-frame $exe .local/hero-stage-entry.json $story 'Hero Story Sequence' 1 0
& $tool --source-root $game city-layout "$gd/set0013_s.bin" .local/city-layout.json $exe
& $tool --source-root $game stage-paths $exe .local/city-paths.json "$config/STG13.ini"
& $tool --source-root $game death-zones $exe .local/city-death-zones.json "$config/STG13.ini"
& $tool --source-root $game object-tables $exe .local/object-lists.json "$config/ObjectLists.ini"
```

The camera example selects action 0, event frame 995, camera slot 0. The source
switches from motion 3 on frame 994 to motion 18 on frame 995. It reports the
original position, target, roll and field of view; it does not render the entire
screen. The second command selects Sonic's City Escape level entry. It looks up
authored character poses and mounts the normal 516- and 898-placement layers.
It rejects requests for later level frames. No default camera, controller, level
clear, or fallback movie is substituted. The first use of an output filename
creates a hash receipt beside it; choose a new ignored path on subsequent runs.

| Data or behavior | What the local checks establish |
| --- | --- |
| [Campaign](NATIVE-STORY.md) | 15 lists, 143 nonterminal entries and 171 ordered actions compared with SA Tools. The executable SHA-256 must match the supported PC build. |
| [Events](EVENT-PARITY.md) and [cameras](EVENT-CAMERA.md) | 66 retail event images decoded. Four independently split events matched at scene/entity level. 7,780 camera samples matched SA Tools across all 13 tracks in the three campaign openers. The beta event remains unsupported. |
| [Stage catalogs](NATIVE-STAGE-CATALOG.md) and [SET](STAGE-SET.md) | 69 PE object lists, 5,104 definitions. Across 232 installed SET files, 58,926 of 59,020 placements resolve. The remaining 94 retain their source ID and a reason. A resolved definition is not an implemented object. |
| [Paths](STAGE-PATHS.md), [death zones](DEATH-ZONES.md) and [positions](STAGE-POSITIONS-PC.md) | 228 authored paths with 10,360 points, 192 death-zone models, and 329 character pose records matched independent local conversions. Reading a trigger volume does not establish the player's collision response. |
| [City object replays](CITY-REPLAY.md) | 148 placements in five source layers select one of five partially ported callbacks: fixed ring, two springs, board trigger and music trigger. The Rust callbacks were compared with compiled reconstructed GameCube C using controlled external services. Native PC engine scheduling, contacts and player physics were not executed. |
| [Controller records](DEMO-INPUT-PC.md) | 12,160 physical records in four raw two-player demo files. Held/pressed input, signed axes and float bits are preserved; retail stop timing is unknown. |
| [Static rendering assets](NATIVE-VISUAL.md) | Seven emeralds and the neutral palm, 670 triangles. Original source units, UVs, normals or vertex colors, textures and alpha checked independently. Camera-normal reflections require a corresponding renderer. No general weighted character renderer. |

Run `cargo test --locked` from the crate for the ordinary checks.
`verify-stage-runtime.ps1` runs the opt-in local suites, including the installed
campaign executable tests. Its parameters identify the original install, config,
C compiler, pinned reconstruction, and independent reference exports. Output
receipts hash the exact inputs, including the PRS and motion bank for a campaign
camera. Keep exports, reports, saves and retail media under ignored local storage.
The [playthrough acceptance trace](PLAYABLE-GAPS.md) records the next missing
evidence for an input-driven City Escape run.
