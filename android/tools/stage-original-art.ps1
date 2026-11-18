[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$SourceImports)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$androidRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sourceRoot = (Resolve-Path -LiteralPath $SourceImports).Path
$destination = Join-Path $androidRoot 'app/src/localArt/assets'
$catalog = Get-Content -Raw -LiteralPath (Join-Path $androidRoot 'docs/ORIGINAL-ASSETS.json') | ConvertFrom-Json
if ($catalog.assets.Count -ne 11) { throw 'Expected the complete three-atlas, flame and seven-gem catalog.' }
$staged = [Collections.Generic.List[object]]::new()
$names = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
function Read-PngDimension([byte[]]$Bytes, [int]$Offset) {
    return ([uint32]$Bytes[$Offset] -shl 24) -bor ([uint32]$Bytes[$Offset + 1] -shl 16) -bor
        ([uint32]$Bytes[$Offset + 2] -shl 8) -bor [uint32]$Bytes[$Offset + 3]
}
foreach ($asset in $catalog.assets) {
    if ($asset.kind -eq 'original-emerald-facet-image') { throw 'Opaque environment textures cannot be staged as gem sprites. Render the original geometry first.' }
    $assetFileProperty = $asset.PSObject.Properties['assetFile']
    $pathProperty = $asset.PSObject.Properties['path']
    if ($null -ne $assetFileProperty) {
        if ($null -ne $pathProperty) { throw 'Bundled-only art must not invent a server route.' }
        $filename = [string]$asset.assetFile
    } else {
        if ($null -eq $pathProperty -or $asset.path -notmatch '^/api/assets/[a-z0-9_-]+\.png$') { throw 'Invalid original-art route.' }
        $filename = [IO.Path]::GetFileName([string]$asset.path)
    }
    if ($filename -notmatch '^[a-z0-9_-]+\.png$' -or -not $names.Add($filename)) { throw 'Invalid or duplicate original-art filename.' }
    $sourceFile = Join-Path $sourceRoot $filename
    $file = Get-Item -LiteralPath $sourceFile
    if ($file.Length -lt 33 -or $file.Length -gt 4194304) { throw "Invalid original PNG length: $filename" }
    [byte[]]$bytes = [IO.File]::ReadAllBytes($sourceFile)
    $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    if ($hash -ne $asset.sha256) { throw "Original PNG hash mismatch: $filename" }
    if ([Convert]::ToHexString($bytes[0..7]) -ne '89504E470D0A1A0A' -or
        [Text.Encoding]::ASCII.GetString($bytes, 12, 4) -ne 'IHDR' -or
        (Read-PngDimension $bytes 16) -ne $asset.width -or
        (Read-PngDimension $bytes 20) -ne $asset.height) { throw "Original PNG dimensions/signature mismatch: $filename" }
    $output = Join-Path $destination $filename
    if ((Test-Path -LiteralPath $output) -and (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) {
        throw "Existing staged art differs from its provenance: $filename"
    }
    $staged.Add([pscustomobject]@{ Filename = $filename; Bytes = $bytes; Output = $output; Hash = $hash })
}
# Validate every source before publishing any of the ignored local assets.
New-Item -ItemType Directory -Path $destination -Force | Out-Null
foreach ($asset in $staged) {
    if (-not (Test-Path -LiteralPath $asset.Output)) { [IO.File]::WriteAllBytes($asset.Output, $asset.Bytes) }
    Write-Output ($asset.Filename + ' SHA256 ' + $asset.Hash)
}
Write-Output 'Verified eleven original/adapted PNGs staged locally. Original bytes remain excluded from Git.'
