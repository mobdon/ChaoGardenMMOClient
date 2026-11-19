<#
.SYNOPSIS
Verify the installed SA2 PC stage, story, event, camera and visual milestone.

.DESCRIPTION
GameRoot contains sonic2app.exe. PcConfig is SA Tools GameConfig/PC_SA2.
VisualLocal is the original checkout's ignored .local containing native visual
fixtures. DecompRoot must be roeming/SA2B at 874a480. Compiler is a host C
compiler (set -Zig for zig.exe). EventReference contains splitEvent exports as
described in docs/EVENT-PARITY.md. Supply paths.ini, deathzones.ini and
camera-frames.bin generated as described in docs/STAGE-PATHS.md,
docs/DEATH-ZONES.md and docs/EVENT-CAMERA.md. Run with Cargo on PATH.
#>
param(
    [Parameter(Mandatory)][string]$GameRoot,
    [Parameter(Mandatory)][string]$PcConfig,
    [Parameter(Mandatory)][string]$VisualLocal,
    [Parameter(Mandatory)][string]$DecompRoot,
    [Parameter(Mandatory)][string]$Compiler,
    [switch]$Zig,
    [Parameter(Mandatory)][string]$EventReference,
    [Parameter(Mandatory)][string]$PathMapping,
    [Parameter(Mandatory)][string]$DeathMapping,
    [Parameter(Mandatory)][string]$CameraReference
)
$ErrorActionPreference = 'Stop'
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$checkout = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
$run = Join-Path $checkout ('.local/stage-runtime-verification/' + [guid]::NewGuid().ToString('N'))
$tempDir = (New-Item -ItemType Directory -Path (Join-Path $run 'tmp') -Force).FullName
$gameData = Join-Path $GameRoot 'resource/gd_PC'
$paths = @{
    SA2_GAME_PATH = $GameRoot
    SA2_SET_DIR = $gameData
    SA2_EVENT_DIR = (Join-Path $gameData 'event')
    SA2_PC_EXE = (Join-Path $GameRoot 'sonic2app.exe')
    SA2_PC_CONFIG = $PcConfig
    SA2_VISUAL_LOCAL = $VisualLocal
    SA2B_REFERENCE = $DecompRoot
    SA2_EVENT_REFERENCE = $EventReference
    SA2_PATH_CONFIG = $PathMapping
    SA2_DEATH_CONFIG = $DeathMapping
    SA2_CAMERA_ORACLE = $CameraReference
}
$environment = @{}
foreach ($key in $paths.Keys) { $environment[$key] = (Resolve-Path -LiteralPath $paths[$key]).Path }
$environment.CC = (Get-Command $Compiler -CommandType Application).Source
$environment.SA2B_ZIG_CC = if ($Zig) { '1' } else { $null }
if ($Zig) {
    $environment.ZIG_GLOBAL_CACHE_DIR = Join-Path $run 'zig/global'
    $environment.ZIG_LOCAL_CACHE_DIR = Join-Path $run 'zig/local'
}
foreach ($key in 'TMP','TEMP','TMPDIR') { $environment[$key] = $tempDir }
$saved = @{}
function Invoke-StageCargo([string]$Name, [string[]]$CargoArgs) {
    & cargo @CargoArgs 2>&1 | Tee-Object -FilePath (Join-Path $run "$Name.txt")
    if ($LASTEXITCODE -ne 0) { throw "Cargo $Name failed with exit $LASTEXITCODE. See $run" }
}
try {
    foreach ($key in $environment.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key)
        [Environment]::SetEnvironmentVariable($key,$environment[$key])
    }
    $base = @('test','--locked','--manifest-path',$manifest)
    # This selects the ordinary library tests, leaving older opt-in audio parity separate.
    Invoke-StageCargo 'library' ($base + '--lib')
    $suites = @('set','board_trace','city_music','city_replay','input_trace','spring_trace',
        'stage_gameplay_trace','stage_spring_host','stage_catalog_local','stage_paths',
        'stage_positions_pc','death_zones','demo_pc','story_local','event_local','event_camera',
        'visual_local','visual_glb_local','object_tables_local','campaign_cli_local')
    $arguments = $base
    foreach ($suite in $suites) { $arguments += @('--test',$suite) }
    Invoke-StageCargo 'stage-suites' ($arguments + @('--','--include-ignored','--nocapture'))
    Write-Output "Passed stage milestone verification ($($suites.Count) integration suites). Logs: $run"
} finally {
    foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key,$saved[$key]) }
}
