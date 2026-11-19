# Native PC two-player demo input

`demo::read_raw` provides a borrowed random-access view of a raw two-player BIN. `frame(index)` returns the two recorded controller slots. A cursor advances one complete record at a time; seek can select any record or park at EOF. Pressed masks come directly from the recording, including when seeking. Neither held nor pressed is recomputed from neighboring records.

| Physical file | Complete records | SHA-256 of source |
|---|---:|---|
| demo13_2p.bin | 3,440 | d2a5735a7a280aa379dc7b70fa6e7ae24e89e6c4606e0d0082e3bbe23dd45251 |
| demo46_2p.bin | 3,719 | 0c0223cd20be08e300f1a3207be414f1cf82b77bdde065345a5e8816527ac4ac |
| demo48_2p.bin | 1,600 | 5bcb9c751d72459b52fbb57c60c1d448bd3889927877704d8575720d196211b3 |
| demo53_2p.bin | 3,401 | 2f330d4a8b5dfe69bcf0d01e3c5576c43781c9419f21225c19a57588029ff226 |

Each 44-byte record begins with big-endian held P1/P2 and pressed P1/P2 masks (four u32), followed by signed X P1/P2 and Y P1/P2 shorts. Bytes 24–25 contain the two stick-pushed values. Bytes 26–27 remain unknown. At bytes 28 and 36 each player has a signed i32 angle and an IEEE 754 binary32 magnitude. The Rust view keeps all 32 magnitude bits and offers a numeric accessor. Endianness is supported by the installed data: 17,873 of 24,320 magnitude fields contain the big-endian encoding of 1.0, with 6,126 more encoding zero. The reader preserves unusual field values.

The file boundary supplies the physical record count. The final records in demo13 and demo46 have unusual mask or stick patterns; iteration reports their bytes faithfully. The runtime's attract-mode stop condition, cadence and full player physics are unverified. The PRS files produce fixed-size buffers with no established active length. No PRS playback, padding scan or end marker heuristic is included.

## Independent check

`tests/demo_reference.py` extracts the local SA2 Mod Loader `struct DemoInput` verbatim from `SA2Structs.h` (SHA-256 `8294f874dbba9e3b9094f78f19b87500fb71ad33e665724bd28316b773e644d5`). Its `Buttons` substitute has the same 32-bit representation as `enum Buttons : int` in `SA2Enums.h` (SHA-256 `752f665c5cb0d1dc99aad35ee4abd5a0a883f558ced0cab42935cee2d541bd24`). A host C compiler checks the struct size and provides offsets and widths for every named member. The driver emits little-endian, player-major fields, followed by the two unknown bytes. Rust compares its output with these four independent digests across all 12,160 records and 24,320 player samples. The synthetic test adds signed, high-bit, unknown-byte, NaN payload and seek boundary cases.

Run `python tests/demo_reference.py <SA2Structs.h> <resource/gd_PC> <zig.exe> cc` to regenerate the four digests. Set `SA2_SET_DIR` to the local `resource/gd_PC` directory and run `cargo test --locked --test demo_pc -- --include-ignored`.
