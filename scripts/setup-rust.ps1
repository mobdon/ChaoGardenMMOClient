[CmdletBinding()]
param(
    [ValidatePattern('^(stable|[0-9]+\.[0-9]+\.[0-9]+)$')]
    [string]$Toolchain = 'stable',
    [switch]$IncludeGitHubCli,
    [ValidatePattern('^v[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$GitHubCliVersion = 'v2.105.0'
)

$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskDownloads = Join-Path $taskProjectRoot '.local/tools/downloads'
New-Item -ItemType Directory -Force -Path $taskDownloads | Out-Null
$env:RUSTUP_HOME = Join-Path $taskProjectRoot '.local/tools/rustup'
$env:CARGO_HOME = Join-Path $taskProjectRoot '.local/tools/cargo'
$env:RUSTUP_INIT_SKIP_PATH_CHECK = 'yes'
$taskHostTriple = 'x86_64-pc-windows-gnu'
$taskRustup = Join-Path $env:CARGO_HOME 'bin/rustup.exe'

if (-not (Test-Path -LiteralPath $taskRustup)) {
    $taskInstaller = Join-Path $taskDownloads 'rustup-init.exe'
    $taskChecksumFile = Join-Path $taskDownloads 'rustup-init.exe.sha256'
    $taskInstallerUrl = "https://static.rust-lang.org/rustup/dist/$taskHostTriple/rustup-init.exe"
    Invoke-WebRequest -Uri $taskInstallerUrl -OutFile $taskInstaller
    Invoke-WebRequest -Uri "$taskInstallerUrl.sha256" -OutFile $taskChecksumFile
    $taskExpectedHash = ((Get-Content -LiteralPath $taskChecksumFile -Raw).Trim() -split '\s+')[0]
    if ($taskExpectedHash -notmatch '^[0-9a-fA-F]{64}$' -or
        (Get-FileHash -LiteralPath $taskInstaller -Algorithm SHA256).Hash -ine $taskExpectedHash) {
        throw 'Official Rust installer SHA-256 verification failed.'
    }
    & $taskInstaller -y --profile minimal --default-host $taskHostTriple --default-toolchain $Toolchain --no-modify-path
    if ($LASTEXITCODE -ne 0) { throw 'Local rustup installation failed.' }
} else {
    & $taskRustup toolchain install "$Toolchain-$taskHostTriple" --profile minimal --no-self-update
    if ($LASTEXITCODE -ne 0) { throw 'Local Rust toolchain installation failed.' }
    & $taskRustup default "$Toolchain-$taskHostTriple"
    if ($LASTEXITCODE -ne 0) { throw 'Could not select the local GNU Rust toolchain.' }
}

if ($IncludeGitHubCli) {
    $taskRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/cli/cli/releases/tags/$GitHubCliVersion" -Headers @{ 'User-Agent' = 'Chao-Local-Development' }
    $taskZipAsset = $taskRelease.assets | Where-Object name -Match '^gh_.*_windows_amd64.zip$' | Select-Object -First 1
    $taskChecksumAsset = $taskRelease.assets | Where-Object name -Match '_checksums.txt$' | Select-Object -First 1
    if (-not $taskZipAsset -or -not $taskChecksumAsset) { throw 'Official GitHub CLI release assets were not found.' }
    $taskArchive = Join-Path $taskDownloads 'gh-windows-amd64.zip'
    $taskChecksumFile = Join-Path $taskDownloads 'gh-checksums.txt'
    Invoke-WebRequest -Uri $taskZipAsset.browser_download_url -OutFile $taskArchive
    Invoke-WebRequest -Uri $taskChecksumAsset.browser_download_url -OutFile $taskChecksumFile
    $taskChecksumLine = Get-Content -LiteralPath $taskChecksumFile | Where-Object { $_ -match [regex]::Escape($taskZipAsset.name) } | Select-Object -First 1
    if (-not $taskChecksumLine) { throw 'Official GitHub CLI checksum record is missing.' }
    $taskExpectedHash = ($taskChecksumLine.Trim() -split '\s+')[0]
    if ((Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash -ine $taskExpectedHash) {
        throw 'Official GitHub CLI SHA-256 verification failed.'
    }
    Expand-Archive -LiteralPath $taskArchive -DestinationPath (Join-Path $taskProjectRoot '.local/tools/gh') -Force
}

# The minimal GNU Rust distribution has no assembler for its import tool.
# Prepare genuine official GNU binutils and its runtime DLLs locally.
if (-not (Test-Path -LiteralPath (Join-Path $taskProjectRoot '.local/tools/mingw-binutils/mingw64/bin/as.exe'))) {
    & node (Join-Path $PSScriptRoot 'setup-rust-binutils.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'Official local GNU binutils preparation failed.' }
}
& $taskRustup component add rustfmt clippy --toolchain "$Toolchain-$taskHostTriple"
if ($LASTEXITCODE -ne 0) { throw 'Local Rust formatting/lint components failed.' }

. (Join-Path $PSScriptRoot 'use-rust.ps1')
$taskSmoke = Join-Path $taskProjectRoot '.local/rust-smoke'
New-Item -ItemType Directory -Force -Path (Join-Path $taskSmoke 'src') | Out-Null
@'
[package]
name = "chao-toolchain-smoke"
version = "0.1.0"
edition = "2024"
'@ | Set-Content -LiteralPath (Join-Path $taskSmoke 'Cargo.toml')
@'
#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" { fn GetTickCount64() -> u64; }
fn main() { assert!(unsafe { GetTickCount64() } > 0); println!("Native Rust and raw-dylib import work."); }
#[test]
fn native_import_and_test_runner_work() { assert!(unsafe { GetTickCount64() } > 0); }
'@ | Set-Content -LiteralPath (Join-Path $taskSmoke 'src/main.rs')
& cargo test --manifest-path (Join-Path $taskSmoke 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Native Rust compile/link/test smoke failed.' }
& cargo run --quiet --manifest-path (Join-Path $taskSmoke 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Native Rust executable smoke failed.' }
& rustc --version
& cargo --version
if ($IncludeGitHubCli) { & gh --version }
Write-Output 'Local tools verified. Future shell: . ./scripts/use-rust.ps1'
