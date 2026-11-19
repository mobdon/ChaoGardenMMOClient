$ErrorActionPreference = 'Stop'
$cratePath = Split-Path $PSScriptRoot -Parent
cargo build --offline --locked --manifest-path (Join-Path $cratePath 'Cargo.toml') --release --bin runtime-replay
if ($LASTEXITCODE -ne 0) { throw 'Native replay build failed' }
cargo build --offline --locked --manifest-path (Join-Path $cratePath 'wasm/Cargo.toml') --release --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { throw 'WASM runtime build failed' }
