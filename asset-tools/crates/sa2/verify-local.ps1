param(
    [Parameter(Mandatory=$true)][string]$GamePath,
    [Parameter(Mandatory=$true)][string]$ReferenceRoot
)
$ErrorActionPreference = 'Stop'
$taskManifest = Join-Path $PSScriptRoot 'Cargo.toml'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskLogRoot = Join-Path $taskRoot ('.local/sa2-parity/' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $taskLogRoot -Force | Out-Null
$taskOldGame=$env:SA2_GAME_PATH
$taskOldLocal=$env:SA2_LOCAL_ROOT
$taskOldReference=$env:SA2_REFERENCE_ROOT
$taskOldInstall=$env:SA2_INSTALL
try {
    $env:SA2_GAME_PATH = (Resolve-Path -LiteralPath $GamePath).Path
    $env:SA2_INSTALL = $env:SA2_GAME_PATH
    $taskReference = (Resolve-Path -LiteralPath $ReferenceRoot).Path
    $env:SA2_REFERENCE_ROOT = $taskReference
    $env:SA2_LOCAL_ROOT = Join-Path $taskReference '.local'
    foreach($taskTest in @('native_local','catalog_local','local_texture_parity')) {
        & cargo test --locked --manifest-path $taskManifest --test $taskTest -- --ignored --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $taskLogRoot ($taskTest+'.txt'))
        if($LASTEXITCODE -ne 0) { throw "Local parity failed: $taskTest" }
    }
    $env:SA2_REFERENCE_ROOT = Join-Path $taskReference '.local'
    & cargo test --locked --manifest-path $taskManifest local_audio_parity -- --ignored --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $taskLogRoot 'audio.txt')
    if($LASTEXITCODE -ne 0) { throw 'Local audio parity failed' }
    Write-Output "Local parity passed; logs: $taskLogRoot"
} finally {
    $env:SA2_GAME_PATH=$taskOldGame
    $env:SA2_LOCAL_ROOT=$taskOldLocal
    $env:SA2_REFERENCE_ROOT=$taskOldReference
    $env:SA2_INSTALL=$taskOldInstall
}
