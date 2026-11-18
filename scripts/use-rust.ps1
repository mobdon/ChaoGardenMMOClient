# Dot-source this file for a project-local Rust session:
# . ./scripts/use-rust.ps1
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$env:RUSTUP_HOME = Join-Path $taskProjectRoot '.local/tools/rustup'
$env:CARGO_HOME = Join-Path $taskProjectRoot '.local/tools/cargo'
$taskCargoBin = Join-Path $env:CARGO_HOME 'bin'
if (-not (Test-Path -LiteralPath (Join-Path $taskCargoBin 'cargo.exe'))) {
    throw 'Local Rust is not installed. Run ./scripts/setup-rust.ps1 first.'
}
$taskPathEntries = $env:Path -split [regex]::Escape([IO.Path]::PathSeparator)
if ($taskPathEntries -notcontains $taskCargoBin) {
    $env:Path = $taskCargoBin + [IO.Path]::PathSeparator + $env:Path
}
$taskGitHubBin = Join-Path $taskProjectRoot '.local/tools/gh/bin'
if ((Test-Path -LiteralPath (Join-Path $taskGitHubBin 'gh.exe')) -and $taskPathEntries -notcontains $taskGitHubBin) {
    $env:Path = $taskGitHubBin + [IO.Path]::PathSeparator + $env:Path
}
$taskGnuBin = Join-Path $taskProjectRoot '.local/tools/mingw-binutils/mingw64/bin'
if ((Test-Path -LiteralPath (Join-Path $taskGnuBin 'dlltool.exe')) -and $taskPathEntries -notcontains $taskGnuBin) {
    $env:Path = $taskGnuBin + [IO.Path]::PathSeparator + $env:Path
}
