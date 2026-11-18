# Local development tools

The authoritative game server is Rust; the browser renderer and local asset pipeline remain JavaScript/Python. Node runs the Vite browser development tooling. Game-derived assets and downloaded development tools stay ignored under `.local`.

## Rust on this Windows workstation

The project has an isolated official GNU Rust toolchain in `.local/tools/rustup` and `.local/tools/cargo`. It does not require a machine-wide Visual Studio installation for this server. Genuine GNU import tools from official MSYS2 packages are local under `.local/tools/mingw-binutils`. The complete server dependency graph and native Windows tests run with these tools.

Install or reproduce the local setup from the repository root:

```powershell
./scripts/setup-rust.ps1 -Toolchain 1.100.0 -IncludeGitHubCli
```

The setup downloads official `rustup-init.exe`, verifies the official SHA-256, uses the minimal `x86_64-pc-windows-gnu` toolchain and `--no-modify-path`, and keeps both `RUSTUP_HOME` and `CARGO_HOME` in `.local/tools`. It adds the local rustfmt/clippy components. `setup-rust-binutils.mjs` downloads genuine GNU binutils and supporting runtime DLLs from official MSYS2 package repositories, verifies each package against the SHA-256 on its official package page, and extracts only into `.local/tools`. Node22.15+ with built-in Zstandard support is required for that helper.

The Rust distribution's minimal GNU import tool needs an absent assembler; complete local `dlltool.exe` and `as.exe` solve this for `windows-sys`/`getrandom`. The validated GNU binutils package is 2.47-4. `.local/tools/mingw-binutils/package-sources.json` records every downloaded package, URL and hash. This handles native import libraries without installing MSVC or a full GCC distribution.

Setup compiles, links and runs an isolated smoke project that invokes `GetTickCount64` through a native `raw-dylib` import, exercising the same import-library path needed by Windows dependencies. The optional GitHub CLI uses the official `cli/cli` release ZIP and verifies its published SHA-256. There is no machine-wide PATH edit, Visual Studio install or background service registration.

For each new PowerShell session:

```powershell
. ./scripts/use-rust.ps1
cargo --version
cargo test --manifest-path server/Cargo.toml
cargo run --manifest-path server/Cargo.toml
```

The helper changes only the current process environment and places `.local/tools/mingw-binutils/mingw64/bin` before incomplete import tools. Its paths are derived from the script location, so it also works when invoked from another directory. The native Rust host is `x86_64-pc-windows-gnu`. The validated November 18, 2026 installation is `rustc 1.100.0` and Cargo `1.100.0`; Rustup's stable channel reported its November 12, 2026 release.

If invoking binaries directly, both local home variables are still required:

```powershell
$env:RUSTUP_HOME = Join-Path (Get-Location) '.local/tools/rustup'
$env:CARGO_HOME = Join-Path (Get-Location) '.local/tools/cargo'
& '.local/tools/cargo/bin/cargo.exe' test --manifest-path server/Cargo.toml
```

Cargo downloads dependencies into the local cargo directory. Use the session helper for a full server build: only setting the two Rust home variables omits complete GNU import tools from PATH. Crates that compile external C/C++ code may require additional build tools; the server's current pure-Rust dependency path does not require installing MSVC.

An earlier experimental Zig DLL-tool adapter was rejected after actual dependency binaries crashed at startup despite a basic smoke passing. It is not used by the setup or helper. Any artifact compiled with that adapter must be rebuilt with complete GNU tools; a fresh `CARGO_TARGET_DIR` is sufficient. The final verification includes the actual server test executable and network tests.

## GitHub CLI

The optional verified binary is `.local/tools/gh/bin/gh.exe`, version `2.105.0` released November 17, 2026. The session helper adds its directory to the current process PATH when present. Installation does not sign into GitHub or inspect credentials. Use the CLI's normal interactive authentication flow only when needed for a user-authorized GitHub operation.

## Original game content

See [INVENTORY.md](INVENTORY.md) for the complete installed-file inventory and exact source-to-browser mappings. See [ASSET-PIPELINE.md](ASSET-PIPELINE.md) for native extraction and Blender conversion. All content conversion reads the Steam installation and writes working copies/outputs to `.local`; original game files and saves are not edited.

Official tool references:

- Rustup Windows installation: https://rust-lang.github.io/rustup/installation/windows.html
- Rustup environment variables: https://rust-lang.github.io/rustup/environment-variables.html
- Official MSYS2 GNU binutils package: https://packages.msys2.org/packages/mingw-w64-x86_64-binutils
- GitHub CLI releases: https://github.com/cli/cli/releases
