# SA2 PC numbered stage SET placements

The Rust reader consumes one installed `resource/gd_PC/setNNNN*.bin` placement file at a time. Retain its filename and SHA-256 when exporting. The installation has 209 numbered files containing 58,592 records. All 209 have a 32-byte header with a big-endian u32 count, followed by exactly that many 32-byte records. The reader ignores the other 28 header bytes. Cart course files use a separate naming scheme and are outside this reader's verified scope.

Each record contains a big-endian u16 (low 12 bits stage-local object-list index, high 4 bits clip level), three big-endian u16 rotation words, three big-endian f32 coordinates, and three big-endian f32 parameters (called Scale in SA Tools). Object code may assign special meanings to any rotation or parameter. Coordinates retain native axes and units. Preserve file order to distinguish two placements with identical transforms.

For City Escape, SA2 Mod Loader's `SA2Variables.h` declares `CityEscape_ObjectArray` at VA 0x109E830 with 102 `ObjectListEntry` records and the list head at 0x109EE90. `SA2Structs.h` defines that entry's flags, list, distance, function and name. These addresses describe the mapped PC executable. The SET file contains indices into the stage table and carries neither function pointers nor behavior implementations. Use the matching stage definition when resolving an index.

## Installed City Escape sources

Stage 13 has the five files below. The base, hard (`hd`) and two-player (`2p`) sets are distinct. The `s` and `u` suffixes denote separate placement files. SA Tools labels the value 8 as additional objects for SA2 `_u` files. Matching suffixes alone do not establish an engine loading schedule or hard-mode fallback.

| Filename | Placements | Distinct IDs | Installed SHA-256 |
|---|---:|---:|---|
| set0013_s.bin | 516 | 52 | 9e7ab581b5b85dd09a4eac1d5b34afd4785b49f17575cb78e4c3c814cf05ff14 |
| set0013_u.bin | 898 | 26 | 96cc77030033fefb76330558a444c87c0e18693294977b60b7d071a64d3dfa32 |
| set0013_hd_s.bin | 476 | 50 | bc585db0ea4ecdf11d69ecd8ef8b6deff48ae9bef8c713669de5746e26bf9b57 |
| set0013_2p_s.bin | 286 | 34 | 6a69bdbe649b8a5f0929a9f8d5fab093beabdcdaa4002c4b6dc8a5688457b6af |
| set0013_2p_u.bin | 486 | 21 | 37028d2b7fc37c2ef8492c683d31c910443dc04f512ab27f526038030ab824e2 |

## Independent comparison

The local SA Tools 786 `SAEditorCommon.dll` (SHA-256 `23b5e6350aacce851bdfe2a8ba90689c05319062f1ddf26bff8d2f8a543b40f8`) decoded all five files with `SETItem.Load`. Instantiate `EditorItemSelection` and initialize `LevelData.ObjDefs` to an empty list before invoking this editor API headlessly. Hash the decoded properties in file order as little-endian u16 ID, u16 ClipLevel, three u16 Rotation words, then six little-endian f32 Position and Scale components. The ignored Rust integration test checks these independent digests for all 2,662 placements. Set `SA2_SET_DIR` to the installed `resource/gd_PC` directory and run `cargo test --locked --test set -- --ignored` in this crate.

The field and ID interpretation follows X-Hax/sa_tools commit `cc05c849b1b7017fb0b519932863e206fa34077a`, `Libraries/SAEditorCommon/DataTypes/SETItem.cs` (blob `f0ede477bc9d91dc97e18dec9fc5e7845e884ff9`). SET placement decoding does not establish object simulation, model binding, or collision behavior.

The reader caps one input at 65,536 placements (2 MiB of records) before allocating the output and rejects nonfinite coordinates or parameters with the placement index. This prevents JSON from silently replacing native nonfinite values with null. The largest installed numbered file contains 944 placements; all 58,592 installed records have finite floats.
