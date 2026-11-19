# Native save and transfer evidence

This tool reads local compatibility data. It does not upload progression, write
flash, emulate JoyBus interrupts or implement a trusted MMO import path.

The reference is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Facts below were transcribed from
the listed reference files; third-party C/assembly and native bytes are not
copied into this package.

| Data | Exact layout or source location | Tool behavior |
| --- | --- | --- |
| Save record | `chao_garden/include/save.h`; `src/save.c` `LoadGameState`: 0x858 bytes | Reuses the existing bounded Rust reader; retains every byte. |
| Header | CRC at 0, marker 0x1965 at 2, unsigned generation at 4 | CRC-16/X-25 covers bytes 2..0x858; original greatest-counter selection and first-wins ties retained. |
| Flash selection | Physical sectors 10..15, stride 0x1000 | Reads first 64 KiB bank; reports additional bytes without guessing their meaning. |
| CRC table | Payload 0x1FD74, 256 little-endian u16 words | All words verified from the exact local decoded payload against reflected polynomial 0x8408, init/final XOR 0xFFFF. |
| Default block | `src/game.c` `sub_02001e74`: payload 0x2071C, 0x780 bytes copied to record+0xD8 | Borrowed opaque bytes with source fingerprint; no inferred stat/genetic labels. |
| Candidate names | `src/game.c` `sub_02001e74`: payload 0x202B6, two modes × ten rows × seven glyph IDs | Bounded mode/row lookup; original glyph IDs preserved. Text encoding remains unknown. |
| Receive command | `src/joy_bus.c` `sub_02000b0c`, native address 0x02000B0C | Low byte 0x2A; upper halfword is a byte-swapped count of two-byte words. Byte 8..15 remains opaque. Staging begins at IWRAM 0x03003350. |
| Send command | `src/joy_bus.c` `sub_02000b64`, native address 0x02000B64 | Low byte 0x22; same length encoding; source begins at save+0x58. Low byte 0x26 is a subsequent send-start control, not a length command. |
| Data words | `sub_02000988`, `sub_02000a40` | Low halfword is byte-swapped sequence; upper halfword is two data bytes in little-endian order. |
| Receive commit | `asm/code.s`, native 0x020030A8..0x02003126 | Completed receive copies staging into save+0x58, updates native state and calls `SaveGameState`. Tool does not perform this mutation. |

The 0x58-byte prefix leaves exactly 0x800 bytes in the 0x858-byte save record.
The native transport trusts its peer's length; this inspector explicitly
rejects declarations larger than 0x800 bytes. The IWRAM staging span before
the interrupt table is 0x810 bytes, but that is not the safe final destination
size. This is a bounded tooling rule, not a claim about retail validation.

`decode_sequential_words` accepts an already deduplicated list of payload
words, requiring one consecutive sequence number per word. Native interrupt
logic allows retransmission and echoes acknowledgements; capturing,
deduplicating, timing and confirming a complete hardware transaction remain
separate work. Command byte 8..15, payload fields, most save fields, hardware
failure recovery and all main-game save-layout semantics remain unknown.

Handshake source evidence includes words 0x6F616843 and 0x4F414843 and a
region-derived identification word. These constants are not browser protocol
messages or credentials. Neither a native CRC nor a well-framed transfer
establishes legitimate server progression. Native effects/order must be
audited separately before implementing any isolated local practice importer.

No native `.sav` or hardware JoyBus transcript was supplied for this work.
The opt-in `source_save_transfer` tests verify the actual ROM-derived default
block and all twenty glyph-table rows. Their record header and transfer-word
list are synthetic, so they demonstrate byte preservation and framing only.
They do not demonstrate a completed retail save/transfer transaction.

| Actual local payload range | SHA-256 |
| --- | --- |
| 0x202B6..0x20342, 140-byte glyph table | `3c3e22f3f953b3ab0579bc713390ac5387ba6b139e33b24fab63904cb4c7d2f5` |
| 0x2071C..0x20E9C, 0x780-byte default block | `ed21c8ce8c09c5bfde4eb07b7b2d3d8042e501ff7b70f01f293d5c4df35ebfe9` |
