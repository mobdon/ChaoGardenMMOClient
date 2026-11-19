param(
    [Parameter(Mandatory = $true)][string]$GameRoot,
    [Parameter(Mandatory = $true)][string]$InspectorExe,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

# Orchestration only: all PRS/GVM/GVR reading and PNG conversion uses Rust.
# New ignored proof outputs only. Never installs aliases or starts a service.
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskGame = (Resolve-Path -LiteralPath $GameRoot).Path
$taskInspector = (Resolve-Path -LiteralPath $InspectorExe).Path
$taskSource = Join-Path $taskGame 'resource/gd_PC/al_mm_kage.prs'
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory, $taskRoot)
$taskLocal = [IO.Path]::GetFullPath((Join-Path $taskRoot '.local'))
if (-not $taskOutput.StartsWith($taskLocal + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory must be a new directory beneath this checkout .local.'
}
if (Test-Path -LiteralPath $taskOutput) { throw 'OutputDirectory exists; choose a new directory.' }
$taskExpectedSource = 'd432d417dc07c60ec497ca9c0d4d5f8953b2a0c7957c5a945be329e9dc5139bb'
$taskHash = (Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskHash -ne $taskExpectedSource) { throw 'Unknown native Chao shadow archive fingerprint.' }
$taskArchive = Join-Path $taskOutput 'archive'
Push-Location $taskRoot
try {
    & $taskInspector extract $taskSource $taskArchive
    if ($LASTEXITCODE -ne 0) { throw 'Rust PRS/GVM extraction failed.' }
    $taskManifest = Get-Content -Raw -LiteralPath (Join-Path $taskArchive 'manifest.json') | ConvertFrom-Json
    $taskEntries = @($taskManifest.assets | Where-Object { $_.category -eq 'texture' })
    if ($taskEntries.Count -ne 1 -or $taskEntries[0].span.sha256 -ne '54ea4f6797466f4a6bbf333f5f5ec63692531609c1d70df36d194c21b22aff3c') {
        throw 'Unexpected native shadow texture entry.'
    }
    $taskGvr = Join-Path $taskArchive $taskEntries[0].output.logicalPath
    $taskHeaderPath = Join-Path $taskOutput 'texture-header.json'
    & $taskInspector inspect $taskGvr $taskHeaderPath
    if ($LASTEXITCODE -ne 0) { throw 'Rust GVR inspection failed.' }
    $taskHeader = Get-Content -Raw -LiteralPath $taskHeaderPath | ConvertFrom-Json
    if ($taskHeader.width -ne 32 -or $taskHeader.height -ne 32 -or $taskHeader.dataFormat -ne 5) { throw 'Unexpected shadow image format.' }
    $taskPng = Join-Path $taskOutput 'kyotu32_marukage.png'
    & $taskInspector texture $taskGvr $taskPng
    if ($LASTEXITCODE -ne 0) { throw 'Rust native shadow PNG conversion failed.' }
    $taskPngHash = (Get-FileHash -LiteralPath $taskPng -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($taskPngHash -ne 'f40f1132b27a6e01d5ed8a52453798aec52764eca6da22ab42c95ce849fe430e') { throw 'Native PNG parity failed.' }
    if ((Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskHash) { throw 'Installed source changed.' }
    [ordered]@{
        source = 'resource/gd_PC/al_mm_kage.prs'
        sourceSha256 = $taskHash
        sourceUnchanged = $true
        inspectorSha256 = (Get-FileHash -LiteralPath $taskInspector -Algorithm SHA256).Hash.ToLowerInvariant()
        nativeEntry = 'kyotu32_marukage'
        dimensions = @(32, 32)
        nativeFormat = 'RGB5A3 GVR; base mip decoded without pixel edits'
        output = 'kyotu32_marukage.png'
        outputSha256 = $taskPngHash
        limitation = 'Exact original PC ShadowTex projection, part scales, opacity, blending and timing remain untraced. Browser terrain projection is an adaptation using original RGBA pixels.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskOutput 'build-evidence.json') -Encoding utf8
    Write-Output "Prepared original Chao shadow texture at $taskPng. No aliases or service changed."
}
finally { Pop-Location }
