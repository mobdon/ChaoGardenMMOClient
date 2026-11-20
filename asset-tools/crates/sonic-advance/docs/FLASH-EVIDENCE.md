# US cartridge flash selection

The read-only Rust inspector accepts a 65,536-byte raw flash image. The example also verifies the ASOE revision 0 cartridge SHA-256 before interpreting the image. A save file carries no authenticated cartridge identity. Its slot markers and checksums establish structural validity for this reader.

~~~powershell
cargo run --bin sonic-advance-assets -- inspect-save "path/to/local.sav" "$env:SONIC_ADVANCE_ASOE_ROM"
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

## Existing-cartridge Garden save copy

In the browser Garden, select a raw 65,536-byte ASOE flash image with valid main and Garden records and choose **Download reconciled save copy**. This invokes the Rust cartridge reader/writer. It reads the main game's saved score at +0x428; the loaded Garden record at +8 remembers the score already imported. The US loader compares these as unsigned words. Only a positive difference is added to the saved Garden wallet (+0x0c), using wrapping addition then a 99,999 cap. Equal or lower scores leave the saved balance alone. The loader also restores the RNG from +0x18, mirrors rings in big-endian at +0x64, and clears +0x68.

The US writer at Garden program offset 0x3ac sets the marker, increments the generation, snapshots the wallet and RNG, computes the CRC over 0x856 bytes and supplies the resulting 0x858-byte record to the flash program function at 0xe084. The copy models a successful first write and leaves the other 15 physical sectors intact. The browser declines export when generation wrap would cause retail to reload a different sector. The input file is never opened for writing. Practice points cannot be exported into the cartridge wallet.

Reproduce the independent retail run with Unicorn using tools/retail-garden-save.py and the exact locally decompressed ASOE program. It executes loader 0x204, writer 0x3ac, and loader 0x204 again. Only audio, VBlank and flash-device boundaries are replaced; checksum, slot selection, wallet arithmetic and RNG functions run unchanged. A synthetic record at physical sector 15 held 800 rings, generation 17, previous score 100 and RNG 0xfedcba98. Launch score 125 produced 825 rings in sector 10, generation 18. The entire new record hashed to 4d796f24d29c01ff2fee2d7a36b31f60132398715a0921bf05331a6685200c13 and reload preserved the RNG and balance. The Rust regression compares all bytes via that hash. This does not model new-Chao initialization, physical flash failures, a server wallet or completion of the care loop.
