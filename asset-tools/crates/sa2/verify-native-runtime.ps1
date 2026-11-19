param(
    [Parameter(Mandatory=$true)][string]$ReferenceRoot,
    [Parameter(Mandatory=$true)][string]$PoseReference
)
$ErrorActionPreference = 'Stop'
$taskManifest = Join-Path $PSScriptRoot 'Cargo.toml'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskRun = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N')
$taskLogRoot = Join-Path $taskRoot ('.local/native-runtime-verification/' + $taskRun)
New-Item -ItemType Directory -Path $taskLogRoot -Force | Out-Null
$taskOldRoot = $env:SA2_REFERENCE_ROOT
$taskOldPose = $env:SA2_NATIVE_POSE_REFERENCE
try {
    $env:SA2_REFERENCE_ROOT = (Resolve-Path -LiteralPath $ReferenceRoot).Path
    $env:SA2_NATIVE_POSE_REFERENCE = (Resolve-Path -LiteralPath $PoseReference).Path
    & cargo test --locked --manifest-path $taskManifest --test native_runtime_local -- --ignored --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $taskLogRoot 'native-parity.txt')
    if($LASTEXITCODE -ne 0) { throw 'Native pose/collision parity failed' }
    Write-Output "Native-runtime parity passed; logs: $taskLogRoot"
} finally {
    $env:SA2_REFERENCE_ROOT = $taskOldRoot
    $env:SA2_NATIVE_POSE_REFERENCE = $taskOldPose
}
