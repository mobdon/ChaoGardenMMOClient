# Native Tiny Garden rewards, return and save lifecycle

`native_garden_progress.rs` ports bounded native A2NE Tiny Garden ring credit,
Memory-mode return decisions, Bounce return decisions and an in-memory save
lifecycle. It reuses the existing checksum and six-sector reader. It does not
accept client reward claims, change a canonical MMO Chao or write an input save.

The embedded payload is independently gated by SHA-256
`84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca`
and SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`. Reference source is pinned
to SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`.
`native-garden-progress-parity.json` records exact C/header/assembly Git blobs,
actual routine byte spans and local oracle output fingerprints. No original
code, payload, save, vector fixture or asset is published with the Rust source.

## Actual-payload credit and return

The native Memory credit helper at payload offset `0xA2DC` adds a **signed**
byte from IWRAM `0x03001970` to the u32 ring bank at `0x03005274`, then compares
the wrapped u32 against 99,999. The two return blocks at `0xBDB4` and `0xD174`
call the results UI service at `0x9DF4`; when ready, they credit the bank,
clear their local byte/phase, and select the original garden callback pointer
`0x02001BF9`. This callback value includes the Thumb bit. UI readiness remains
an explicit native service input; full minigame play is separately implemented.

Bounce's return routine at `0xF620` runs its UI/frame services, increments an
i16 fade counter and writes the resulting u16 brightness register. Only when
the signed counter exceeds 15 does it choose a destination. A nonzero stored
completion callback is selected without credit or garden-task restoration.
With a zero completion callback, it adds the original u16 reward at
`0x03001AD4`, retains native wrapping/capping, requests the original task
restore at `0x02000E28`, and selects the garden callback.

The native callback switch provides dispatch ownership: the caller must stop
invoking the old endpoint after return. These pure helpers do not invent a
second replay ledger, durable transaction or MMO reward authorization. The
credited amount is not a browser-submitted result for the server. The native
return endpoints themselves do **not** call `SaveGameState`.

## Opaque persistent state

`GardenSaveSession::load` keeps the original 0x858-byte record opaque except
for source-traced load/save fields. It preserves:

- six physical sectors 10..15, greatest unsigned generation and first-wins ties;
- native no-valid-save local-sector sentinel 6 and generation zero;
- initialization of bytes 8..0x58 and 0x58..0x6A, retaining the last-read opaque
  Chao block rather than inventing a new Chao;
- seed restoration from offset 0x18 when a valid record is selected;
- the original config-generation/ring-bank reconciliation with u32 wrapping,
  conditional 99,999 cap, byte-swapped mirror at 0x64 and clear byte 0x68.

`GardenLoadServices::from_seed(config_counter, seed)` executes native `0x8810`:
one native bounded RNG draw with limit 30, followed by `(draw+60)*60`. It reuses
the existing native garden RNG and retains the post-draw seed. Both result
and seed are independently checked against actual `0x8810`/`0x1554` Thumb
execution with a bounded BIOS Mod shim. Initial-word results cover all thirty
values from 3,600 through 5,340. Valid records instead restore their saved
seed. This helper does not claim complete garden/Chao initialization.

`write_memory` packs marker, incremented generation, ring bank, RNG seed and
native CRC before programming. It advances through at most **five** sectors
out of six, retains state changes even if every attempt fails, and reports the
exact attempted physical sectors. The explicit test adapter leaves failed
sectors unchanged; success fills one 4KiB sector with FF and copies 0x858
bytes. All other sectors and trailing bytes are retained.

This adapter is not flash hardware, partial programming, flash timing, audio
suspension, file persistence or server storage. Loading and mutating a record
does not promise a current checksum; only native save packing regenerates it.
The crate-local mutable record accessor exists for independently traced native
hatch/shop/feed fields, without assigning names to other opaque data.

## Independent checks and limits

`native_garden_progress_oracle.py` executes the actual verified embedded Thumb
bytes using local test-only Unicorn 2.1.3. Initialization and signed credit
routines are executed unmodified with a bounded BIOS Mod service.
Memory UI readiness, preliminary Bounce UI calls and garden-task
restore are explicit bounded service stubs. No graphics are synthesized by the
oracle. The Rust runtime has no emulator dependency.

`source_native_garden_progress` passes:

- all six native routine-span fingerprints against the actual payload;
- 4,096 initial-word/post-draw RNG cases, covering all thirty native results;
- 4,096 signed-credit cases, covering every byte and unsigned-wrap boundaries;
- 4,096 Memory return cases covering both native blocks and both readiness states;
- 4,096 Bounce return cases covering signed fade-counter boundaries, alternate
  callbacks, u16 reward wrapping, cap and task-restore call decisions;
- 64 complete exact-C load/write sessions covering every six-sector failure
  mask, invalid/tied/wrapped generations, every record byte and every byte of
  the 64KiB in-memory flash image. Source C uses the original 0x858-byte struct,
  verified with a compile-time size assertion, and explicit flash/RNG/audio stubs.

The save vectors are synthetic. No real native garden `.sav` has been supplied
to this workstream; the existing local `synthetic-flash.sav` is not evidence of
real-save compatibility. The user's source save and ROM are read-only inputs.

```powershell
$env:A2NE_ROM = '<local verified A2NE ROM>'
$env:A2NE_GARDEN_PROGRESS_ORACLE = '<local native-garden-progress-oracle directory>'
cargo test --test source_native_garden_progress -- --ignored
```

Full minigame physics/UI, task backup/restore implementation, complete Chao
initialization/care, JoyBus handoff, durable storage and the canonical
server round trip remain separate acceptance requirements. Local original-game
ring credit does not replace the server's bounded, once-only MMO decisions.
