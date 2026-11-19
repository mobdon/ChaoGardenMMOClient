# ASOE Rust port handoff — stopped November 18, 2026

**One worker should own the next integration.** This worker and its subagents stopped at the user's request. Nothing was merged or deployed. This is a draft checkpoint, not a complete game port.

**Where to resume:** `mobdon/ChaoGardenMMOClient`, draft PR #14, branch `codex/sonic-advance-native-stage`, stacked on asset-catalog PR #4. All changes are under `asset-tools/crates/sonic-advance/`. The separate desktop workstream also changes core files with the same names. Do not blindly merge or overwrite either implementation. Choose the authoritative terrain/player implementation first, then keep one implementation for desktop and WASM. Keep the server history separate.

**First blocker:** 89 Rust tests pass and two fail on the stopped tree (release, including local-ROM tests). The partially integrated upright terrain probe in `src/runtime/terrain.rs` and `Runtime::advance` settles the starting Y at **114038 Q8**, versus **113920 Q8 (445 pixels)** in the original. `tests/runtime.rs::actual_stage_walk_jump_land_and_replay_are_deterministic` fails there. `tests/runtime_edges.rs::retail_crouch_charge_release_and_first_thirteen_roll_ticks` also fails its 24-frame digest. Both passed before the terrain change. Preserve the original-game expectations. Start by inspecting fractional Y during upright floor settlement, and reconcile with the desktop worker's stronger sensor implementation.

**Retail oracle:** `python tools/retail-replay.py LOCAL_ASOE_ROM LOCAL_MGBA_LIBRETRO_DLL` runs an in-memory cold boot. It checks both fingerprints and emits numbers, not assets. The original is stationary at (104,445) at frame 1799. Holding Down, pressing jump three times, then releasing Down gives ground speed 2304 with no displacement and air-X velocity zero; next tick moves with speed 2300. The walk–jump route has two original pickups at route ticks 198 and 222, source ring IDs 14 and 15. See `tests/retail_rings.rs`. The earlier flat resolver reached only one; the stopped slope change has not established full-route parity. Compare against retail; never change a test to match a port-only trace.

**Ready to reuse:**

- English title fade, affine logo, original prompt and edge-triggered Start. Eleven full-screen BGR555 digests, from title tick 0 to 187, match original mGBA video (retail frames 1052–1239). Regenerate with the oracle's `--title` flag; compare to `runtime-replay LOCAL_ASOE_ROM --title` and `tests/retail-title-frames.json`. Start signals a menu request; no main menu or character select implementation is included in this checkpoint. The browser's act selector is an explicit practice shortcut.
- Both Neo Green Hill acts render from their original stage resources; 27 timed graphics hashes verify animation/scroll. All 18 ring layout entries match independent reference coordinates; five are empty. See `docs/STAGE-ANIMATION.md` and `tests/asoe-ring-layouts.tsv`.
- Crouch, roll, spindash, bounded native overcharge lookup, source sprite clips, both native dust sizes, ring pickup effect, original ring HUD and warning phases. See `docs/MOVEMENT.md`, `docs/HUD-EVIDENCE.md`, and sprite tests. Ring effect script 708/0 at ROM `0xF5070` is 68 bytes (SHA-256 `5015bb9209802191a7ec8d7084c41306c1ae30f1de66467bafe4bcac87d61e98`). This runtime does not play its sound.
- Fresh native/WASM replay passed all **17 checkpoints over 553 ticks**, including state, framebuffer, reset, invalid inputs and batching. Artifact fingerprint is in `docs/RUNTIME-PARITY.json`. A separate headless Chrome 157 test exercised local ROM selection, a rejected file, title Start, Act 1 keyboard spindash, pause/reset with exact framebuffer comparison, and walking in Act 2. The page made only the four allowed loopback requests. These tests demonstrate the two builds agree; the retail terrain discrepancies above remain.

**Reproduce from this crate:** set `SONIC_ADVANCE_ASOE_ROM` to the exact local USA revision-0 file (SHA-256 `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`), then:

```powershell
cargo test --release --offline --locked --no-fail-fast -- --include-ignored
./tools/build-runtime.ps1
node tools/verify-runtime.mjs $env:SONIC_ADVANCE_ASOE_ROM ./target/release/runtime-replay.exe
node tools/verify-browser.mjs $env:SONIC_ADVANCE_ASOE_ROM 'PATH_TO_CHROMIUM' 'PATH_TO_PLAYWRIGHT_CORE_INDEX_MJS'
```

**Still open:** shared canonical movement, native menu/character selection, complete acts and progression, enemies and bosses, audio, saves/transfers and Tiny Garden gameplay. The reference for source-derived findings is SAT-R/sa2 at `2dd3f17d30cd3c156e508aac0017620ae485b9ba`; validate USA offsets independently. No ROMs, extracted graphics/audio, save data or runtime binaries are in this patch. No canonical MMO service round trip or rollout was attempted.
