param(
    [Parameter(Mandatory = $true)][string]$SourceRoot,
    [Parameter(Mandatory = $true)][string]$InspectorExe,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [string]$BlenderExe
)

# Owner-local preparation only. This never installs aliases or starts services.
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskSource = (Resolve-Path -LiteralPath $SourceRoot).Path
$taskInspector = (Resolve-Path -LiteralPath $InspectorExe).Path
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory, $taskRoot)
$taskLocal = Join-Path $taskRoot '.local'
if (-not (Test-Path -LiteralPath $taskLocal)) {
    New-Item -ItemType Directory -Path $taskLocal | Out-Null
}
$taskLocal = (Resolve-Path -LiteralPath $taskLocal).Path
if (-not $taskOutput.StartsWith($taskLocal + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory must be a new directory beneath this checkout .local.'
}
if (Test-Path -LiteralPath $taskOutput) { throw 'OutputDirectory already exists; choose a new directory.' }
$taskBlender = $null
if ($BlenderExe) { $taskBlender = (Resolve-Path -LiteralPath $BlenderExe).Path }
$taskModels = Join-Path $taskSource '.local/extracted/shadow-skate/teriosmdl'
$taskImports = Join-Path $taskSource '.local/imports'
foreach ($taskId in 92..99) {
    if (-not (Test-Path -LiteralPath (Join-Path $taskModels "$taskId.sa2mdl"))) { throw "Missing native model $taskId." }
}
foreach ($taskFile in @('shadow-air-shoes-left.glb', 'shadow-air-shoes-right.glb', 'upgrade-conversion-report.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $taskImports $taskFile))) { throw "Missing source $taskFile." }
}
$taskInspected = Join-Path $taskOutput 'inspection'
New-Item -ItemType Directory -Path $taskInspected | Out-Null
$taskManifest = Join-Path $taskRoot 'asset-tools/crates/shadow-shoe-effects/Cargo.toml'
Push-Location $taskRoot
try {
    $taskCargoMessages = @(& cargo build --offline --locked --manifest-path $taskManifest --message-format=json)
    if ($LASTEXITCODE -ne 0) { throw 'Rust bundler build failed. Activate the project Rust toolchain first.' }
    $taskArtifacts = @($taskCargoMessages | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
        $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'shadow-shoe-effects' -and $_.executable
    })
    if ($taskArtifacts.Count -ne 1) { throw 'Cargo did not identify exactly one fresh bundler executable.' }
    $taskBundle = (Resolve-Path -LiteralPath $taskArtifacts[0].executable).Path
    foreach ($taskId in 92..99) {
        & $taskInspector inspect (Join-Path $taskModels "$taskId.sa2mdl") (Join-Path $taskInspected "model$taskId.json")
        if ($LASTEXITCODE -ne 0) { throw "Native Rust inspection failed for $taskId." }
    }
    foreach ($taskSide in @('left', 'right')) {
        $taskName = "shadow-air-shoes-$taskSide.glb"
        & $taskBundle bundle $taskSide (Join-Path $taskImports $taskName) (Join-Path $taskImports 'upgrade-conversion-report.json') $taskModels $taskInspected (Join-Path $taskOutput $taskName)
        if ($LASTEXITCODE -ne 0) { throw "Native $taskSide bundle failed." }
    }
    if ($taskBlender) {
        & $taskBlender --background --factory-startup --python (Join-Path $PSScriptRoot 'inspect-native-shoe-effects.py') -- $taskSource (Join-Path $taskOutput 'saio-reference.json')
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $taskOutput 'saio-reference.json'))) { throw 'Independent SAIO inspection failed.' }
        & node (Join-Path $PSScriptRoot 'verify-native-shoe-effects.mjs') $taskSource $taskOutput
        if ($LASTEXITCODE -ne 0) { throw 'Independent native material/geometry parity failed.' }
    }
    $taskEvidence = [ordered]@{
        sourceRoot = $taskSource
        inspectorSha256 = (Get-FileHash -LiteralPath $taskInspector -Algorithm SHA256).Hash.ToLowerInvariant()
        bundlerSha256 = (Get-FileHash -LiteralPath $taskBundle -Algorithm SHA256).Hash.ToLowerInvariant()
        independentSaioProofRun = [bool]$taskBlender
        outputDirectory = $taskOutput
        limitation = 'Prepared aliases only. Browser material/grounding and canonical Rust service round trip require separate validation. Native timing/Chao mounting remain adaptations.'
    }
    $taskEvidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskOutput 'build-evidence.json') -Encoding utf8
    Write-Output "Prepared native shoe aliases at $taskOutput. No live aliases or services changed."
}
finally { Pop-Location }
