<# Verify this milestone from the sa2 crate. Supply private SA Tools exports as
   described in docs/BOARD-MODEL.md and docs/CITY-LAND.md. Install unicorn==2.1.4
   in the selected Python environment. Code, game files and exports are read-only. #>
param(
    [Parameter(Mandatory)][string]$GameRoot,
    [Parameter(Mandatory)][string]$DecompRoot,
    [Parameter(Mandatory)][string]$Compiler,
    [Parameter(Mandatory)][string]$BoardReference,
    [Parameter(Mandatory)][string]$RelLandReference,
    [Parameter(Mandatory)][string]$WinLandReference,
    [string]$Python='python',
    [switch]$Zig
)
$ErrorActionPreference='Stop'
$game=(Resolve-Path -LiteralPath $GameRoot).Path
$gd=Join-Path $game 'resource/gd_PC'
$envMap=@{
    SA2_GAME_PATH=$game; SA2_SET_DIR=$gd; SA2_PC_EXE=(Join-Path $game 'sonic2app.exe');
    SA2_CITY_LAND_REL=(Join-Path $gd 'stg13D.rel'); SA2_CITY_LAND_DLL=(Join-Path $gd 'DLL/Win32/Data_DLL.dll');
    SA2_CITY_LAND_ORACLE=(Resolve-Path -LiteralPath $RelLandReference).Path;
    SA2_CITY_LAND_DLL_ORACLE=(Resolve-Path -LiteralPath $WinLandReference).Path;
    SA2_BOARD_REFERENCE=(Resolve-Path -LiteralPath $BoardReference).Path;
    SA2B_REFERENCE=(Resolve-Path -LiteralPath $DecompRoot).Path;
    PYTHON=@(Get-Command $Python -CommandType Application)[0].Source;
    CC=@(Get-Command $Compiler -CommandType Application)[0].Source;
    SA2B_ZIG_CC=$(if($Zig){'1'}else{$null})
}
$saved=@{}
try {
    foreach($k in $envMap.Keys){
        $saved[$k]=[Environment]::GetEnvironmentVariable($k)
        [Environment]::SetEnvironmentVariable($k,$envMap[$k])
    }
    $manifest=Join-Path $PSScriptRoot 'Cargo.toml'
    & cargo test --manifest-path $manifest --locked --test physical_board_trace --test board_motion_local --test board_model_local --test city_land --test city_replay --test player_physics -- --include-ignored
    if($LASTEXITCODE){throw "Board evidence suite failed: $LASTEXITCODE"}
    & cargo test --manifest-path $manifest --locked --test campaign_cli_local board_pose_and_entry_ground -- --ignored
    if($LASTEXITCODE){throw "Board CLI comparison failed: $LASTEXITCODE"}
} finally {
    foreach($k in $saved.Keys){[Environment]::SetEnvironmentVariable($k,$saved[$k])}
}
