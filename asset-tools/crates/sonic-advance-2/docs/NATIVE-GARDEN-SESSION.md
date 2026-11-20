# Cartridge-practice end-to-end verification

`source_garden_session.rs` independently tests the new `GardenSession`
composition through physical minigame inputs, native ring credit, hatching,
shopping, item placement, feeding, timed weeds and save/reopen. All three tests
pass with the verified local A2NE cartridge and an initially empty 64KiB flash
image. No claimed reward, injected wallet balance, manufactured board or
preloaded inventory is supplied to the Rust session.

This is a local cartridge-practice harness. Named Garden actions enter traced
native callbacks directly. It does not establish a complete original cursor,
resident Chao AI, sound/render task scheduler, hardware emulator or canonical
MMO service round trip. An exported practice save cannot overwrite the
authenticated MMO Chao or establish legitimate MMO gains.

## Winning Memory round and care loop

The independent native Thumb oracle generates the physical input stream from
the original cartridge's private board. The Rust test receives only that fixed
input stream, not a private board or a way to set its outcome. Consecutive
frames with the same held edge are prohibited in the recorded route; the
session derives pressed edges from held buttons itself.

Default load seed 0 performs the actual `0x8810` initializer draw and leaves
seed `0x3039`. The matching native round trace then contains **1,775 physical
input frames**, with original selection beginning at callback frame 969 and
1,941 total original callbacks including the native intro. This matches the
Rust round boundary after instructions, not a claim that the instruction UI
has been ported. The trace SHA-256 is:

`2d579d408af0b18e73681ae48400060300b13f01826084a89f14106780dd8a42`.

The test checks native phase, selection cursor, mistakes, matched pairs and
ring score frame by frame. It checks the final saved RNG against the original
last-frame RNG. The native winning result earns **90 rings**. That round
leaves the opaque Chao bytes unchanged.

The same session then hatches the original starter egg, rejects a second
hatch, opens native stock, buys fruit 0 for **30 rings**, and rejects buying
another held item. It rejects invalid and blocked terrain placement, accepts
an original-map ground cell, and checks the saved fruit position. Feeding
consumes exactly that inventory slot; repeating the feed is rejected.

Growth is checked against the fruit's original signed channels. Every
unrelated record byte is compared before/after feeding; only the native save
header, saved RNG, consumed fruit ID and traced mood/belly/experience fields
may change in this route. Level and point state remains consistent with this
first feed. The exported record contains wallet **60** at its native bank and
byte-swapped transfer mirror. Reopening with a different external seed
preserves wallet, growth, inventory, hatched Chao and the saved RNG/opaque
record. A further save changes only its generation/checksum.

## Bounce and weeds

The Bounce route uses the exact independently verified physical policy:
press A when zero-based frame `% 60 == 0`, release on all other frames.
Without controlled gameplay-state writes, it finishes after **1,097 ticks**
with the native losing result and **7 earned rings**. Another 120 Garden
frames do not award again. An unaffordable fruit purchase is rejected, and
save/reopen retains seven rings at both native bank representations.

A separate default-seed test waits for the native initialized weed timer,
observes an emerging weed mature, enters its pull callback, verifies activity
exclusion during pulling, and advances until the original slot disappears.
Saving/reopening retains removal and a zero wallet. All bytes from the opaque
Chao prefix through the end of the 0x858-byte record remain unchanged. This
establishes timed weed integration and persistent state, not resident Chao AI.

Every route checks that physical sectors 0..9 remain untouched. The caller's
input flash and ROM bytes remain unchanged; the save API supplies a new
in-memory image. Filesystem export belongs to the separate write-new local
CLI boundary. No real owner's native `.sav` fixture was supplied, so these
checks do not establish real-save compatibility.

## Reproduction

The ignored winning input fixture can be reproduced with the existing native
oracle without changing its ordinary four-case output:

```python
import pathlib, runpy, sys
helper = pathlib.Path("asset-tools/crates/sonic-advance-2/tests/native_memory_round_oracle.py")
payload = pathlib.Path(".local/gba/mb-chao-garden-A2NE.gba")
output = pathlib.Path(".local/gba/memory-round-oracle/session-default-seed.bin")
sys.argv = [str(helper), str(payload), str(output)]
oracle = runpy.run_path(str(helper), run_name="native_memory_oracle")
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(oracle["run_game"](0x3039, 0, False))
```

This uses the local test-only Unicorn/Capstone dependencies documented by the
Memory oracle; no emulator dependency enters the Rust runtime. Then run:

```powershell
$env:A2NE_ROM = '<local verified A2NE ROM>'
$env:A2NE_MEMORY_SESSION_INPUTS = '<local session-default-seed.bin>'
cargo test --test source_garden_session -- --ignored
```

Source/header/payload identity and the lower-level reward/save comparisons
are recorded separately in `NATIVE-GARDEN-PROGRESS.md`. The end-to-end test
establishes composition of those bounded primitives through the listed local
practice routes; hardware, visual and MMO acceptance remain separate gates.
