# Tiny Garden JoyBus transcript inspection

The Rust transfer module reads a completed data-phase transcript. Each event supplies a 32-bit host word and the Garden reply observed for that host write. audit_transcript validates the echoed length command, data, retry behavior and final acknowledgement. It returns the opaque payload bytes, counts of retries/rejected words and the native input halfword encoded in the completion. It never opens a serial device or writes a save.

The cartridge is the exact ASOE revision 0 accepted by catalog::garden. ROM offset 0x9BE78 starts a 114,900-byte compressed stream, SHA-256 35fe9c39f06f246adbe3e806532438152f9b6c16c5efd9a2cf5928f32f082bf2. The 138,752-byte decoded program has SHA-256 989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0 and runs at 0x02000000. The table below uses offsets in that decoded program.

| Code and literal pool | Offset | Length | SHA-256 |
| --- | ---: | ---: | --- |
| JoyBus initialization | 0x43C | 0x100 | cd0c28639235fd2368b11283e9e724fa6eb4e90b4f4a43527c5fd682f2b95647 |
| IRQ command dispatcher | 0x630 | 0x2D0 | 6ff9c87bce327be76e4a2304add8cf73a4d3dcc8359457a1f5097688a0ba6c32 |
| Receive setup | 0x900 | 0x58 | 4b3f1f04dd060c13e3eb4b3a71299d19b89305a3750ca66e787969278fadc18c |
| Receive data and finish | 0x958 | 0xB8 | 94b7c0fef1a18e05722211e9ebf484747b3a884aac937bcba5e550c1d4c74430 |
| Send setup | 0xA10 | 0x4C | 052ca59674deb6b49c3b43e9e3210801db56b0ab52263e15b3198e0f6f12ff9c |
| Send data and finish | 0xA5C | 0xB0 | e0825ceea59efd46d117ead9aed9b6becace77edf523c7cc4b466405e3991d13 |
| Completion consumer and call into save writer | 0x2C02 | 0x88 | fc1e6472ac4ffbbf46cea8e095ddcb7c018c55e732ed81bf820da1bfbad25cb3 |

**Command and data bytes.** The low byte selects host-to-Garden 0x2A or Garden-to-host 0x22. Bits 8–15 are preserved. Bytes 2 and 3 form a big-endian halfword count. In a data word, the low two bytes encode the big-endian sequence number, starting at zero. The remaining two bytes are retained in the order stored in the little-endian register.

**Receiving.** State 6 accepts sequence zero to enter state 7. A repeated identical command before sequence zero reinitializes the receive cursor. In state 7 a matching sequence rewrites its two stored bytes. The next consecutive sequence advances the cursor by two and writes the new pair. Other sequences return zero. After the final pair the host sends 0x6F616843. The Garden returns low halfword 0x4B43 and copies its input halfword, with swapped bytes, into the high half. The receive-completed flag becomes 2.

**Sending.** Following a 0x22 command, state 8 accepts a host word with low byte 0x26 to start. Each reply has the same sequence/data representation. The host echoes the whole last reply to advance. A different acknowledgement returns that same reply again. Echoing the last pair completes the send with the 0x4B43/input response. Repeated JOY_TRANS reads, interrupt reset and timeout sequences are outside the event representation.

**Storage boundary.** ASOE setup at 0x900 uses staging address 0x03002780; 0xA10 reads from 0x03003018. The completion consumer at 0x2C18–0x2C32 loads record base 0x03002FC0, adds 0x58 and copies halfwords from staging using the BIOS CpuSet wrapper at 0xDB08. It checks receive-completed state 2 before this copy and calls the Garden save routine at 0x3AC later in the branch. The reader caps data at 0x858 minus 0x58 = 0x800 bytes, the space available in the real record. It rejects empty declarations, excess data and command changes within a single audited capture. These guards are reader policy. Payload data does not carry the record header or its flash CRC. The parser does not ascribe names, values, eligibility or ownership to its opaque bytes.

**Independent execution.** A local Unicorn harness mapped the actual decoded ASOE program into EWRAM, mapped simulated IWRAM and I/O, and called the original IRQ dispatcher at 0x02000631 (Thumb entry). It started in state 5, provided input halfword 0x1234 and a six-byte synthetic send buffer AA BB CC DD EE FF. Each call wrote JOYCNT=2 at 0x04000140 and the host word at 0x04000150, then read the actual reply from 0x04000154. No IRQ routine was replaced. The receive trace returned zero for a skipped sequence, accepted a changed repeat, produced staging bytes 34 12 CC DD EE FF and returned 0x34124B43 on completion. The outbound trace returned 0xBBAA0000 again for a mismatched acknowledgement, then 0xDDCC0100 and 0xFFEE0200 for sequential echoes. It completed with the same 0x34124B43. Exact word-for-word fixtures are in tests/transfer.rs. The reference run used synthetic registers and RAM, no private save or physical cable. It demonstrates the data phase, without establishing GameCube software behavior, electrical timing, full handshake, persistent flash effects or MMO trust.

The SAT-R/sa2 reference is pinned to 2dd3f17d30cd3c156e508aac0017620ae485b9ba. Relevant sources are chao_garden/src/joy_bus.c (Git blob 8c59f83be83fad7e846ef688562c0257aa93724b), chao_garden/asm/code.s (7ec760b4772bbab9d4e25282324f248728521c07) and chao_garden/include/save.h (14833d791e1846d56db545b21ece0dae36fbf8f6). ASOE moves four helpers and uses different RAM globals. Its JoyBus initialization loads the cartridge game-code word through the pointer at payload 0x52C and the literal 0x45425347 at 0x534. These are cartridge-specific observations; the reference's region derivation is not assumed. The CHAO/Chao constants at 0x734, 0x774, 0x7A8 and 0x9A4 are now tied to handshake/polling and completion paths in this executable.

Run cargo test --test transfer -- --include-ignored with SONIC_ADVANCE_ASOE_ROM set to the local fingerprinted source.
