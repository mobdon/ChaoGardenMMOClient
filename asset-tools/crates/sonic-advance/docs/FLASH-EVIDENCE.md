# US cartridge flash selection

The read-only Rust inspector accepts a 65,536-byte raw flash image. The example also verifies the ASOE revision 0 cartridge SHA-256 before interpreting the image. A save file carries no authenticated cartridge identity. Its slot markers and checksums establish structural validity for this reader.

~~~powershell
cargo run --example flash -- "$env:SONIC_ADVANCE_ASOE_ROM" "path/to/local.sav"
cargo test --test flash
~~~

The JSON output contains physical sector numbers, record validity, validated Garden generations, selected sectors and prospective rotation metadata. Both inputs are opened for reading. The tool does not emit record bodies, save contents, character attributes or credentials.

| Physical sectors | Native validation | Selection |
| --- | --- | --- |
| 0–9, game | The first little-endian word is 0x4F524950 (PIRO). The word at 0x42C equals the wrapping sum of 267 little-endian words in bytes 0..0x42C. | Probe sector zero. If its marker is absent, start scanning at one. A later missing marker stops the scan. A bad checksum on a marked slot is skipped. Use the last valid sector reached. |
| 10–15, Tiny Chao Garden | Little-endian marker **0x0817** at +2. The halfword at +0 equals the reflected 0x8408 CRC of bytes +2 through +0x857 inclusive, initialized to 0xFFFF and complemented at the end. | Inspect all six slots. Use the greatest unsigned 32-bit generation at +4. The earlier physical sector wins a tie. |

Each physical sector is 0x1000 bytes. Bytes after the native record are outside its checksum. The inspector labels a record erased when its complete native record span is 0xFF. It reports physically valid game sectors after a gap even when the cartridge ignores them during selection.

The native game skips an unchanged write. For a changed game save it targets the slot after the selected one, rotating from 9 to 0, and erases the slot after the target before programming. The first target is zero when no record was found. Garden increments its generation modulo 2^32, advances cyclically through sectors 10–15, and may try up to five destinations if programming fails. Its unsigned load comparison means that a surviving generation 0xFFFFFFFF beats a newly written zero. The inspector reports the first prospective destination without attempting any writes.

## Source and native execution

Reference source is SAT-R/sa2 commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba, specifically src/game/sa1/save.c and chao_garden/{src/save.c,include/save.h}. The Garden source uses **0x1965** for its build. The US executable loads **0x0817** in both its loader and writer. The exact US binary controls that constant.

* Main ASOE ROM at offset 0x12C5C executes the selector, with game marker at literal 0x12CB8 and checksum offset at 0x12CBC. The checksum routine starts at 0x12F78. The loaded save sector index is at 0x0300508C.
* The verified Garden stream starts at ROM offset 0x9BE78 and decompresses into EWRAM at 0x02000000. Its loader starts at payload offset 0x204. At 0x236 it reads the next sector, at 0x23C it compares against literal 0x26C, at 0x244 it calls the CRC routine at 0x148C, and at 0x27C the unsigned lower-or-same branch preserves the earlier record. The input size and CRC span are at 0x270 and 0x274. Its writer at 0x3AC uses marker literal 0x424 and the same checksum routine. The reflected lookup table is at 0x18374.

Six synthetic 64 KiB images were passed to both original Thumb selectors in Unicorn. The native game selector ran from 0x08012C5C through return, with only its ReadFlash entry at 0x08098FB4 replaced by a byte copy from the synthetic image. The Garden selector ran from 0x02000204 up to 0x02000290, the end of its six-sector scan. Its flash identification and interrupt setup returned immediately, and its ReadFlash entry at 0x0200DEAC copied bytes. Both checksum routines and selection branches executed unchanged. The game result was read from 0x0300508C and the Garden relative index from R8. The Rust tests reconstruct the same images, check their independent SHA-256 values and assert these observed physical sector outcomes.

| Synthetic scenario | Game | Garden |
| --- | ---: | ---: |
| Entire image erased | None | None |
| Missing game sector zero and corrupt slot two, Garden generation tie | 3 | 11 |
| Game gap after zero, Garden maximum generation versus zero | 0 | 10 |
| Game full after erased zero, only Garden sector 15 valid | 9 | 15 |
| Corrupt game zero and later gap, only corrupt Garden record | 1 | None |
| Corrupt final game slot, highest valid Garden generation in final sector | 8 | 15 |

The CLI smoke check on the second synthetic image reported game sector 3 and Garden sector 11. No private player save was used. These results cover structure, read selection and prospective destinations. They do not establish hardware flash failure timing, JoyBus transactions, Chao identity, save migration or server progression.
