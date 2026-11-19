# PC stage death-zone geometry

The death-zones command loads the mapped stage's death-zone table and Basic models from the original Steam executable. One SA Tools 786 PC_SA2 stage INI selects a stage. A combined configuration of deathzone sections selects multiple stages. The Rust reader checks the executable SHA-256 (4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16) and resolves each pointer through its file-backed PE span. It never loads the executable as a process.

SA Tools' PC_SA2/STG13.ini identifies City Escape's table at RVA 0xCDC678. The original records are a 32-bit flags word and NJS_OBJECT pointer and end with a null model pointer. The [SA2 Mod Loader DeathZone declaration](https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Structs.h) describes that pair. SA Tools' flags export identifies four bytes for character flags, Constant1, Constant2 and DeathFlag. Rust preserves the full source word. The original mesh coordinates remain local to their object. Object flags, rotation, position and scale allow a consumer to apply the original transform.

The 50 mapped tables contain 192 Basic models. All have an unlinked root node with one meshset. The loader supports the observed quad and strip mesh types and retains the original corner order, winding, vertices, normals, material words and bounding sphere. It rejects unsupported geometry layouts. Integer counts and total records are bounded. The three City Escape death zones are separate from that stage's landtable and SET placements. Player eligibility, contact, death and respawn behavior require additional native routines.

## Reproduce the comparison

Use an unused, private directory for the extracted retail models. From the sa2 crate directory run:

```powershell
$env:SA2_PC_EXE = 'C:\path\to\sonic2app.exe'
$privateOutput = 'C:\path\to\private\death-zone-verification'
./tests/death_zone_reference.ps1 -ToolsRoot 'C:\path\to\sa-tools-786' -GameExe $env:SA2_PC_EXE -PrivateOutput $privateOutput
$env:SA2_DEATH_CONFIG = Join-Path $privateOutput 'deathzones.ini'
cargo test --locked --test death_zones -- --include-ignored
```

The verification script combines the local SA Tools configurations, runs its original split.exe and reads each extracted SA1MDL with SAModel.dll. Both readers produced SHA-256 909e148110784ed7f6e505331b7de62bba15033aa255032154bc5d1da6b0aa87 over the 50 tables' flags, node transforms and geometry. The comparison treats positive and negative floating-point zero as equal because SA Tools normalizes some signed-zero normals on export. Rust preserves the original float bits. Verification used Windows PowerShell Core with DOTNET_ROLL_FORWARD set to Major on a host with the newer Windows Desktop runtime.
