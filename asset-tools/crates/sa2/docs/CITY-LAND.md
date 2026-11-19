# City Escape static land from the installed REL

`city_land::read_collision_stage` accepts the 3,304,912-byte Steam-delivered
`resource/gd_PC/stg13D.rel` with SHA-256
`bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b`.
No GameCube disc identity or choice of data by the running Windows game was established.

The Rust reader resolves section-five references from REL relocation records and
decodes the landtable at file offset 0x162D1C. The collision view retains native
entry order, node transforms, surface flags, and original Basic attachments. To reuse
the validated stage decoder it maps a section-five offset N to staging address
N+16. Subtract 16 from generated node/entry/meshset addresses to recover section
offsets; add 0x5DD40 to the recovered section offset for the original file address.
GC render attachments remain opaque; texture and animation loading are outside
this view. Queries use the existing Rust static raycaster. No retail player response,
dynamic collision update or Chao Garden integration is established here.

## Independent source comparison

SA Tools 786 `GameConfig/GC_SA2B/stg13D.ini` names `objLandTable0013`,
address 162D1C, format SA2B. A private input mapping copied the original
preamble and just that named section. Reproduce with the installed tools and game
in private, ignored storage, assigning paths to the binary, mapping and private
output folder in PowerShell:

```powershell
$env:DOTNET_ROLL_FORWARD = 'Major'
& $split binary $installedRel $singleLandIni $privateOutput
$env:SA2_CITY_LAND_REL = $installedRel
$env:SA2_CITY_LAND_ORACLE = Join-Path $privateOutput 'stg13_cityescape/objLandTable0013.sa2blvl'
cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml --test city_land -- --ignored --nocapture
```

On the installed file the converter produced a 1,411,740-byte level, SHA-256
`7346b19a3bd53c5fcf595848d06d6c8eadc005167f51053946b5ac54987e5bf7`.
The private differential test compares the two separately decoded data sources
through the same Rust geometry baker: all 2,912 entries' kinds and flags, all
536 meshes' order, 12,706 transformed vertex positions and all 14,274 triangle
index triples match exactly. At unit scale a ray from (0,1000,0) straight down
hits entry 2715 at (0,0,0). This compares decoders, not the Rust baker to the
retail engine. All source and extracted geometry remains local.

## Win32 named export

`city_land::read_win32_collision_stage` reads the installed
`resource/gd_PC/DLL/Win32/Data_DLL.dll` as inert PE32 data. The SHA-256
is `6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59`.
The export directory names `objLandTable0013` at RVA 0x1915F40 (image base
0x10000000); its distinct `header_stg13` export is not used as a landtable.
The reader maps preferred virtual addresses through file-backed PE sections.
For this reader staging addresses equal **DLL file offsets + 16**, not REL
section offsets. SA Tools lists the export in `PC_SA2/LeftoverDLL.ini`.
The export's presence and its geometry do not establish that the Windows
executable loads it during play.

SA Tools 786 direct `split dllexport` halted with a duplicate-key error (4096).
For the independent conversion, Python pefile's `PE(...).get_memory_mapped_image()`
produced a private RVA-indexed copy, without rebasing its pointers. Run
`split binary` on this copy with the mapping below and a private output directory:

```ini
key=10000000
bigendian=False
game=SA2B
[objLandTable0013]
type=landtable
address=1915F40
filename=city.sa2blvl
format=SA2B
```

This produced a 1,411,740-byte level (SHA-256
`51cff996dbb7effdca60acc9f5c7eeaed9fc29127e2fdb7b670b074cbc23b758`).
Set `SA2_CITY_LAND_DLL` and `SA2_CITY_LAND_DLL_ORACLE` to their local paths
then run the `win32_land_matches_independent_conversion` test with `--ignored`.
All 2,912 entry kinds and flags, 536 mesh associations, 12,706 transformed
vertex positions and 14,274 triangle index triples matched this DLL-specific
conversion exactly. The ground probe also matched the same expected hit.
