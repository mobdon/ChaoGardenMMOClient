# A3VE embedded Tiny Garden evidence

This is a version-specific asset and opaque-save catalog. It does not run Tiny
Garden, implement Chao Ball, or establish parity for native game mechanics.
ROMs, decoded payloads, images and native saves remain local. Pinball mechanics
are a separate workstream.

## Identity and address spaces

The supplied A3VE revision 0 ROM is 8,388,608 bytes, SHA-1
`92b5c19cb4dee8fdfc15a8abdc699822d5373042`, SHA-256
`5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b`.

Its embedded garden is a type `0x10` LZ77 stream at **ROM offset `0xD0970`**.
The ROM literal at `0x1D19C` holds `0x080D0970`. Strict decoding consumes
152,552 bytes (`0x253E8`), ending at `0xF5D58`, and yields 188,716 bytes
(`0x2E12C`). Alignment padding is excluded from the consumed size.

| Digest | Value |
| --- | --- |
| Stored stream SHA-256 | `72fd1b2645fc1358ad131a697dac1c53b4ae57f195759ff78e1535ebda70d29f` |
| Decoded garden SHA-1 | `8462861fe92477148220310332282337a69cf030` |
| Decoded garden SHA-256 | `33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb` |

All offsets below refer to the **decoded garden** unless labeled ROM. A garden
pointer is `0x02000000 + offset`; it is not a cartridge pointer. Some garden
resources match Advance 2 bytes exactly, but their offsets have different
shifts in different regions. No common offset adjustment or Advance layout is
used to interpret A3VE.

## Compressed presentation resources

The bounded aligned scan finds 13 LZ77 streams in the garden. Their consumed
lengths, output lengths, source/output hashes and literal references are recorded
in [catalog.json](../catalog.json). The references below are literal positions;
a literal alone does not establish runtime reachability or an animation name.

| Stream offset | Stored bytes | Decoded bytes | Literal reference | Established use or limit |
| --- | ---: | ---: | --- | --- |
| `0x12738` | 215 | 512 | `0x8E5C` | Presentation stream; semantic name unknown |
| `0x12810` | 232 | 640 | `0x8E68` | Presentation stream; semantic name unknown |
| `0x128F8` | 260 | 640 | `0x8F08` | Presentation stream; semantic name unknown |
| `0x1614C` | 3,878 | 10,848 | `0xD4FC` | Minigame OBJ load to `0x06010000` |
| `0x17214` | 8,109 | 17,760 | `0xD498` | Minigame BG load to `0x06000040` |
| `0x20934` | 2,553 | 7,680 | `0x1D3C` | Garden BG load to `0x06008000` |
| `0x21858` | 2,902 | 10,784 | `0x1D34` | Garden BG load to `0x060055E0` |
| `0x223B0` | 1,737 | 5,120 | `0x1D2C` | Garden BG load to `0x060041E0` |
| `0x22A7C` | 234 | 1,344 | `0x8F10` | Presentation stream; semantic name unknown |
| `0x22D68` | 4,121 | 9,696 | `0x1D24` | Garden BG load to `0x06001C00` |
| `0x23D84` | 204 | 720 | `0x1914` | Work-RAM decoded resource; semantics unknown |
| `0x24390` | 7,312 | 21,088 | `0x1D58` | Garden OBJ load to `0x06012D80` |
| `0x26020` | 3,357 | 6,144 | `0x1D60` | Garden OBJ load to `0x06011580` |

The same scanner also accepts one type `0x30` RLE-shaped candidate at `0xE644`
(60,570 stored bytes, 196,650 decoded bytes, no pointer references). It overlaps
other regions and remains **structural/unknown**; it is not a fourteenth proven
presentation asset. Neither scan completeness nor successful tile decoding proves
that every native sprite, animation script, tilemap, UI or audio dependency has
been identified.

## Chao Ball candidate

The garden selection code compares a record ID with `4` at `0x3A92`, then reads
the secondary field at record offset `0x1A` at `0x3AAE`. A nonzero field selects
Thumb pointer `0x0200D295` from literal `0x3AD4`, entering the loader at `0xD294`.
The zero branch selects `0x0200A8A5` from `0x3AEC`. This establishes two native
paths; their menu labels still need runtime confirmation.

The `0xD294` path loads these A3VE resources. Local tile inspection shows Chao,
umbrella, rings and circular objects, supporting the **Chao Ball candidate**
label. It does not prove native input, board, scoring, reward or timing rules.

| Resource | Span | Source pointer literal | SHA-256 |
| --- | --- | --- | --- |
| OBJ palette span | `0x1606C`, `0xE0` bytes | `0xD4F4` | `15927abd5d354edf7c87c1282f4b55c124430d985bf935c5945453181a7331d9` |
| BG palette span | `0x17074`, `0x1A0` bytes | `0xD494` | `1dfb6add24d0c66b29e5b5a27d9cf88599017a198495e5d7168ac6c873f6d33a` |
| OBJ LZ77 source | `0x1614C`, 3,878 bytes | `0xD4FC` | `c187d348641b37fdf9afcbf067d32c1330cd8efcbe47749d781fec7bab4798d0` |
| OBJ decoded tiles | 10,848 bytes | Destination literal `0xD500` | `7fe825cbf93cad4f3344c14b80e33188947c2d49d9995e62608bb3cf14ccf0b9` |
| BG LZ77 source | `0x17214`, 8,109 bytes | `0xD498` | `d3a6c072ea314f12db73fac21ab64048dc1f50a8af1ba9e666dde3a8c5576187` |
| BG decoded tiles | 17,760 bytes | Destination literal `0xD49C` | `9c0387ae7e61eb585ac7d78bcf138bcae5684dae9097ab6d8fba9b037440c56b` |

## Garden palettes and Chaos forms

The garden BG palette is at `0x22B68` (`0x200` bytes, literal `0x1D20`), and the
OBJ palette is at `0x23E50` (`0x200` bytes, literal `0x1D50`). Their SHA-256 hashes
are `3d8663b493740046758985db3c5897c3683d518bee45c8e932c66cb974f76e2f`
and `af6cd3b3c5efb363e25dc86f562c788d9b674346c148db96bbf779bb3c6d68be`.

The palette update routine at `0x1D68` reads an index at `0x1D86`, compares it
with 13 at `0x1D88`, and branches on unsigned lower-or-same at `0x1D8A`.
Indices 0 through 13 select 32-byte banks at `0x24050 + index * 32` (base literal
`0x1DEC`). Indices above 13 use the RAM palette at `0x030015B0` (literal
`0x1D9C`). Both paths target OBJ palette RAM `0x05000200`. The 14-bank source
span is `0x1C0` bytes with SHA-256
`774acea3bc91e04bcea0346943681dd12491104a82c333bd161caa2e49e908ee`.

This proves a **display palette index**, not that the index equals the native
Chao type code. The type-to-index mapping, sprite selection, animation selection,
and Neutral/Hero/Dark Chaos values remain unknown. No unused Chaos sprite or
native shared-sprite fallback is claimed from this palette branch alone.

## Native flash records

The A3VE loader at `0x248` has SHA-256
`25cdd363c8dfbc6b6605bc11614c6f378c039cc52788cc4360a5bdd6941a2e92`
over `0x248..0x3AC`. Its literals at `0x2B0`, `0x2B4` and `0x2B8` establish
marker `0x1965`, record length `0x858`, and checksum length `0x856`.

| Record offset | Length | Verified interpretation |
| --- | ---: | --- |
| `0x0` | 2 | Little-endian stored checksum |
| `0x2` | 2 | Little-endian marker `0x1965` |
| `0x4` | 4 | Little-endian unsigned generation |
| `0x8` | `0x850` | Opaque bytes retained verbatim |

CRC covers record bytes `0x2..0x858`: reflected polynomial `0x8408`, initial
`0xFFFF`, final inversion (CRC-16/X-25). The native routine at `0x185C..0x189C`
has SHA-256 `0d55d7507b6273da19737f8f0300d4cc8331d4a3f8792b0b7ad80c5d9147ef29`.
Its table pointer is at `0x189C`; all 256 words at `0x1F4E0..0x1F6E0` match the
generated polynomial table, SHA-256
`638dbc5f735cced09b3d0d767a72a8800baeb1b3bb399a514af61b3ea7be8a01`.
The identical table also begins at ROM `0xF5D58`.

The native loader adds 10 to the sector counter at `0x26E`; comparison with 5
at `0x2D0` and the loop branch at `0x2D2` establish sectors 10 through 15.
The reader bounds input to a 64 KiB flash image and examines one `0x858` record
per 4 KiB sector. Invalid markers or checksums are skipped. The first valid
record is selected even at generation zero: the signed negative sector sentinel
is tested at `0x296..0x29C` before the first-selection path at `0x29E..0x2A4`.
Subsequent generations are loaded at `0x2BC` and compared at `0x2BE`; the unsigned
lower-or-same branch at `0x2C0` preserves the first equal generation. Generation
wraparound receives no invented timestamp ordering.

Tests cover synthetic records, corruption, exact input bounds, generation zero,
ties and unsigned maximum generation. **No native save fixture was supplied**;
there is no source-save roundtrip claim. Unknown save fields remain opaque, and
a valid native checksum does not authorize MMO stat or inventory changes.

## Transfer code and remaining dependencies

The JoyBus initializer at `0x480..0x590` has SHA-256
`95209ade487569aaaa849dffd660451f6de87f94e21b059c311466c8e70e803b`.
It references these native hardware literals:

| Literal offset | Value | Identified register or datum |
| --- | --- | --- |
| `0x538` | `0x04000134` | RCNT |
| `0x53C` | `0x04000158` | JOYSTAT |
| `0x540` | `0x04000154` | JOY_TRANS |
| `0x544` | `0x04000140` | JOYCNT |
| `0x548` | `0x020006B9` | Thumb handler pointer |
| `0x57C` | `0x080000AF` | Cartridge header byte source |
| `0x580` | `0x004F5341` | Protocol magic literal |
| `0x588` | `0x00425347` | Protocol magic literal |

Transfer packet lengths, Chao/item/ring field boundaries, handshake states,
failure/retry behavior and successful-transfer/save ordering remain unverified.
Native JoyBus code is not the MMO transport protocol. Animation descriptors,
sprite assembly, text/UI naming, audio dependency graphs and the Chao type to
display lookup require further independent tracing before they can be labeled
complete or reused as game mechanics.
