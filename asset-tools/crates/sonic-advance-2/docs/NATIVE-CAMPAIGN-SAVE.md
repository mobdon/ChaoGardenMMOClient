# A2NE campaign save lifecycle

`native_campaign_save.rs` implements the main game's profile and ten-sector
journal separately from the Tiny Chao Garden record. Its source is pinned to
SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. No disk, flash
device, live save, MMO identity or server state is opened or modified.

## Native format and API

| API/data | Implemented boundary |
| --- | --- |
| `CampaignProfile` | Native fields, six raw u16 name words, three button bindings, five character progress/emerald rows, 315 raw u16 time records, ten multiplayer profiles and score |
| `CampaignProfile::new_usa` | A2NE defaults: English language 2, Sonic unlocked, A/B/R controls, 36,000-frame records and native name terminators |
| `native_bytes` / `from_native_bytes` | Exact C in-memory profile layout, including its padding byte; bounded reads retain noncanonical bool8 values |
| `pack_sector` / `unpack_sector` | Packed flag/control conversions, profile ID, independent random nonce, version increment, record/name/profile copying and score |
| `checksum` / `checksum_valid` | Wrapping sum of the 221 little-endian u32 words before offset 0x374 |
| `find_newest_sector` / `find_oldest_sector` | Original ten-header selection, tie behavior and rollover quirks |
| `inspect_flash` | Read-only header/checksum report and selection candidates; signature presence is distinct from checksum validity |
| `CampaignSaveSession` | Initialized profile/cache/buffer plus in-memory load, write and new-save transitions, with explicit memory failure injection |
| Multiplayer helpers / `generate_completed` | Original profile insertion/move-to-front, counters capped by native `<99` checks, and native completed-profile initializer |

The campaign journal occupies sectors 0..9, each 0x1000 bytes. The packed
payload is 0x378 bytes and has checksum at 0x374, security word 0x4547474D at
offset zero, version at four, profile ID at eight and a separate random nonce
at 0x36C. `pack_sector` receives its random word from the caller. Session write
methods receive a `FnMut() -> u32` binding for the native `Random32` service;
neither API invents credentials or authoritative rewards.

The actual compiled pinned C in-memory `SaveGame` layout is **0x374 bytes**,
with time records at 0x32, multiplayer rows at 0x2A8 and score at 0x370. The
reference header's older offset comments differ from these compiled offsets.
The oracle statically asserts the sizes/offsets and uses `bool8 = u8`; it does
not force the structs to those stale comments. The flash-sector layout remains
the separate 0x378-byte payload described above. Cartridge instruction-level
in-memory ABI parity is not established by this C layout test.

Time records flatten native `[character][zone][act][rank]` order, with five
characters, seven time-attack zones, three acts and three records per course.
Names remain six original words, without guessing Unicode conversions. Native
bool8 bytes are retained as u8 in profiles; sector flag packing treats nonzero
as true and unpacking normalizes the corresponding flags to zero/one.

## Journal behavior preserved

Header-only `SaveGameExists` behavior is exposed separately from integrity.
The native recovery read verifies the checksum, without checking the security
word again. A zero-filled payload therefore passes that checksum even without
a signature. Recovery starts from the native newest-header candidate, erases
failed sectors in the copied memory image and scans backward by physical
sector number, wrapping zero to nine. It does not sort all valid revisions.
Erase failures are ignored during that recovery loop, as in the original.

When any valid header has version zero, the newest-selection second pass does
not recheck header security and can select an unsigned high version from an
invalid header. The oldest-selection max-u32 path retains the original
`minVersion > 0xFFFF0000` condition; it returns no suitable sector, and the
write caller falls back to sector zero. These behaviors are preserved rather
than replaced by a different journal algorithm.

Before attempting a write, the native lifecycle may assign a zero profile ID,
copies the profile into the last-written cache and increments/packs the sector
buffer. This also happens without flash or after a programming failure. A later
unchanged write with a nonzero cached ID can report success without retrying
the failed physical operation. The Rust implementation preserves that native
state transition; its return value does not certify durable storage.

`new_save` retains the current language and score, restores other defaults,
writes sector zero and then erases sectors one through nine, stopping on the
first error. A successful reset still has native profile ID zero until a later
ordinary write assigns one. Every operation preserves sectors beyond nine.

## Explicit host memory adapter

Sessions accept 10..16 complete 4-KiB sectors and copy the input. `flash_bytes`
returns that private image for inspection; the module performs no filesystem
or device I/O. `MemoryFlashFaults` injects programming and per-sector erasure
errors. Failed operations leave the affected copied sector unchanged.

Successful simulated programming fills the target sector with erased FF bytes
and copies the 0x378-byte payload. This is an explicit test/host binding, not a
claim about the physical driver's out-of-payload writes or partial damage.
Hardware erase/program/verify retries, interrupted writes, audio suspension,
DMA/interrupt ordering, flash identification and electrical timing remain
unimplemented. Raw campaign files from an original cartridge have not been
supplied; the supplied garden fixture is a different format and cannot serve
as a real campaign-save compatibility test.

## Exact C-source evidence

`tests/native_campaign_save_oracle.py` verifies eight exact source Git blobs
before decoding them. It compiles the original struct declarations, constants,
macros and eighteen original save/profile functions against deterministic
Random/flash stubs in an ignored directory. Checksum `uintptr_t` is explicitly
bound to the native 32-bit word width; host pointer size cannot silently turn
the checksum into a 64-bit sum. All original C and generated binaries/vectors
remain local.

Three source tests pass:

- 256 synthetic raw profiles: byte-preserving native profile codec, packed
  sector bytes, unpacked state, completed-profile behavior and Random calls.
- 1,024 header configurations: newest/oldest selection and signature-only save
  existence, including zero, 0xFFFF and u32 rollover values.
- 64 session states across eight scenarios: every current profile, cached
  profile, packed buffer and all 65,536 copied flash bytes match after load,
  write, unchanged write, new-save, completed initialization and multiplayer
  operations. Scenarios include corrupt-latest fallback, checksum-valid zero
  data, no flash, programming failure and erase failure.

Three focused Rust tests also cover truncation, flag/control normalization,
rotation quirks, cached failed-write behavior and garden-sector preservation.
These are synthetic-data/C parity checks, not hardware or durable-service
integration. Metadata-only fingerprints are in
`catalog/native-campaign-save-parity.json`.

```powershell
. ./scripts/use-rust.ps1
$crateRoot = '.local/worktrees/sonic-advance-2-native/asset-tools/crates/sonic-advance-2'
$python = 'C:/Users/Admin/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'
& $python "$crateRoot/tests/native_campaign_save_oracle.py" `
  --reference-dir .local/research/a2ne-stage-reference `
  --output-dir .local/gba/native-campaign-save-oracle `
  --summary "$crateRoot/catalog/native-campaign-save-parity.json" `
  --zig .local/tools/zig/zig-x86_64-windows-0.17.1/zig.exe
$env:A2NE_CAMPAIGN_SAVE_ORACLE = (Resolve-Path .local/gba/native-campaign-save-oracle).Path
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --lib native_campaign_save::
cargo test --locked --manifest-path "$crateRoot/Cargo.toml" --test source_native_campaign_save -- --ignored
```

Use crate-relative paths and explicitly supplied local references/compiler in a
clean checkout. No fixture, profile or compatibility result grants acceptance
of MMO progression, Tiny Garden transfer, an installed client, live migration
or runtime rollout.
