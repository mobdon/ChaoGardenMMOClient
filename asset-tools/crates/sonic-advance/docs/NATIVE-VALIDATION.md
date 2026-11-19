# Shared runtime validation — November 18, 2026

Verified with the local 8 MiB ASOE image, SHA-256 `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`. Chrome 157 tested the actual loopback adapter: wrong file rejected, valid local file accepted, title Start, keyboard crouch/charge/release, pause, reset to the native spawn image, and walking in Act 2. Only the four static routes were requested. No ROM upload or service calls.

* All **110 Rust tests** passed in release with private source tests enabled. Strict native and WASM Clippy passed. Verified 12 distinct ordinary stage spawns/renders and rejection of slot 12 by the practice adapter.
* The two regressions in the PR #14 handoff are covered by `runtime::tests::handoff_original_spin_digest_and_both_pickups`: settled Y = **113920 Q8**, the original 24-frame spindash hash is unchanged, small/large dust transitions match, and the walk/jump route collects original ring IDs 14 and 15 at ticks **198 and 222**.
* Cold native/WASM replay: 600 matching snapshots, trace SHA-256 `ab9fc4e27419236a7ffd9d1f7fd93ad63946a7d951695c3f151f5fb40b542506`, final RGB `0e987956075c9482f117bd87d58d4705879e1f70ac18deeda24f9b3f791eae0f`. Three jumps, three landings and 14 rings. This first checksum compares the two builds.
* Independent retail reference: after settling, six hundred frames of X/Y, both velocities, camera X/Y, collision layer and ring count match the original on desktop and WASM. Little-endian eight-i32 digest: `f3191a348b928b9fcf28f22314a091a750675941f28b35707dbb5e42338f8f34`. The pinned Windows mGBA libretro core is SHA-256 `d775ecef6da1da5489ff2e0a37c13826b510c73711f89e78cb8d757c6f8c1426`. The original runs only inside the validation harness.
* Eleven whole title frames match the original native-color video hashes from ticks 0 through 187. Six animated Act 2 stage views match independent upload/scroll reference output. Windows desktop window messages verified D, Z, P, R and Escape. Physical-device acceptance is separate.
* In the longer first-act comparison, position, velocity and collision layer match through tick 1785 (zero-based). At tick 1786 the retail player accelerates near the slight-right booster (kind 33, coordinates 7096,1248). This branch has no complete first-act retail playthrough.

From the crate directory, set `SONIC_ADVANCE_ASOE_ROM` to the private ROM and run:

```powershell
cargo test --release --offline --locked --all-features -- --include-ignored
cargo clippy --offline --locked --all-targets --all-features -- -D warnings
cargo build --release --offline --locked --manifest-path wasm/Cargo.toml --target wasm32-unknown-unknown
cargo clippy --offline --locked --manifest-path wasm/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
node tools/verify-shared.mjs $env:SONIC_ADVANCE_ASOE_ROM
node --test tools/serve-runtime.test.mjs
cargo build --release --offline --locked --example spawn_hash
node tools/verify-browser.mjs $env:SONIC_ADVANCE_ASOE_ROM $env:CHROME_EXE $env:PLAYWRIGHT_CORE_MODULE ./target/release/examples/spawn_hash.exe
python validation/retail_trace.py $env:MGBA_LIBRETRO_DLL $env:SONIC_ADVANCE_ASOE_ROM
```

For local browser practice, build WASM as above and run `node tools/serve-runtime.mjs`. For desktop: `cargo run --release --features native-window --bin sonic-advance-native -- play $env:SONIC_ADVANCE_ASOE_ROM`. Left/right or A/D moves, Down/S crouches or rolls, Space/Z jumps or revs, P pauses, R restarts, Enter continues a stopped practice run, and Escape exits. The standalone desktop stage runner begins at the selected act, not the title.
