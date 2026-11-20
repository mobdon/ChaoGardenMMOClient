[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$A2neCrate,
    [Parameter(Mandatory = $true)][string]$Rom,
    [ValidateSet('Garden', 'Sidebar0', 'Sidebar1')][string]$Kind = 'Garden',
    [string]$Output
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$exportKind = @{ Garden = 'garden'; Sidebar0 = 'sidebar-0'; Sidebar1 = 'sidebar-1' }[$Kind]
if (-not $Output) {
    $filename = @{ Garden = 'tiny-garden-a2ne.png'; Sidebar0 = 'tiny-garden-sidebar-0-a2ne.png'; Sidebar1 = 'tiny-garden-sidebar-1-a2ne.png' }[$Kind]
    $Output = Join-Path $PSScriptRoot ('../../.local/imports/' + $filename)
}

$cratePath = (Resolve-Path -LiteralPath $A2neCrate).Path
if (Test-Path -LiteralPath $cratePath -PathType Leaf) { $cratePath = Split-Path -Parent $cratePath }
if (-not (Test-Path -LiteralPath (Join-Path $cratePath 'Cargo.toml') -PathType Leaf)) {
    throw '-A2neCrate must point to the independently delivered sonic-advance-2 crate.'
}
$romPath = (Resolve-Path -LiteralPath $Rom).Path
$outputPath = [IO.Path]::GetFullPath($Output)
if ([IO.Path]::GetExtension($outputPath) -cne '.png') { throw '-Output must be a .png path.' }
if (($outputPath -split '[\\/]') -notcontains '.local') { throw 'Native output must remain inside an ignored .local directory.' }
if ((Test-Path -LiteralPath $outputPath) -or (Test-Path -LiteralPath ($outputPath + '.provenance.json'))) {
    throw 'Refusing to overwrite an existing image or provenance receipt.'
}
$localRoot = $outputPath
while ((Split-Path -Leaf $localRoot) -ne '.local') {
    $localRoot = Split-Path -Parent $localRoot
    if (-not $localRoot) { throw 'Missing .local ancestor.' }
}
$harnessPath = Join-Path $localRoot ('tiny-garden-render-build/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $harnessPath 'src') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'tiny-garden-render.rs') -Destination (Join-Path $harnessPath 'src/main.rs')
$tomlPath = $cratePath.Replace('\', '/')
if ($tomlPath.Contains("'")) { throw 'Crate path contains an unsupported quote.' }
$manifest = @"
[package]
name = "tiny-garden-render"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
sonic-advance-2-assets = { path = '$tomlPath' }
serde_json = "1"
png = "=0.17.16"
"@
[IO.File]::WriteAllText((Join-Path $harnessPath 'Cargo.toml'), $manifest, [Text.UTF8Encoding]::new($false))
$manifestPath = Join-Path $harnessPath 'Cargo.toml'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'tiny-garden-render.Cargo.lock') -Destination (Join-Path $harnessPath 'Cargo.lock')
& cargo fmt --manifest-path $manifestPath --check
if ($LASTEXITCODE -ne 0) { throw 'Rust renderer formatting failed.' }
& cargo test --offline --locked --manifest-path $manifestPath
if ($LASTEXITCODE -ne 0) { throw 'Rust renderer tests failed.' }
& cargo clippy --offline --locked --manifest-path $manifestPath --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Rust renderer lint failed.' }
& cargo run --offline --locked --manifest-path $manifestPath -- $romPath $outputPath $exportKind
if ($LASTEXITCODE -ne 0) { throw 'Native garden export failed.' }
Write-Output "Export: $outputPath"
Write-Output "Provenance: $outputPath.provenance.json"
Write-Output "Ignored build harness: $harnessPath"
