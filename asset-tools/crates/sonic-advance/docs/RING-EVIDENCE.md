# Native ASOE ring layouts and local collection

Verified November 18, 2026 against the same fingerprinted ASOE revision-0 ROM
used by the asset catalog. `docs/native-rings.json` records all 18 compressed
extents, source/decoded hashes, dimensions, counts, and independent coordinate
hashes. It contains metadata only.

The root is ROM `0xB7F4C`, 18 pointers / 72 bytes, SHA-256
`18a55e39c209e87181aab8bfbabdabd030b0aa7f768e9765db97a1200bd79511`.
The first layout is ROM `0x6C8D8C`: 969 RLE bytes decode to 1,886 bytes,
51 by 6 regions and 291 rings. All 18 layouts contain 3,837 position records.
Every ordered world coordinate matches its corresponding pinned reference CSV;
matching coordinates establish layout parity, not gameplay execution parity.

The first compressed SHA-256 is
`50a06a7609e24ca43a1b8ccc2cdc2e1d330c074572717e8494eaa89cc51a3987`;
decoded SHA-256 is
`fdf280edc42746ec4e505ce5618828f5a6ff3fbb82d6734eca19e9e60445132f`.
Ordered little-endian signed i32 `(x,y)` pairs hash to
`3e02b6cfb65a35132218f1ba779eb5974aec4b09b94230894bcd222686e87f5d`.

The RLE output contains a length word, two region counts, a u32 offset per
region, and two-byte `(x,y)` records terminated by an x byte of `0xFF`.
Nonzero list offsets are relative to decoded byte four. Regions span 256 pixels;
record coordinates use eight-pixel units. Seven first-stage records have local
x=32: treating that legitimate boundary coordinate as corrupt loses real rings.
The reader validates pointer/table separation, lengths, termination and budgets.

The local upright collector follows the SA1 region-window arithmetic and
inclusive ring rectangle `(-8,-16)..(8,0)`. Sonic uses a six-pixel half width
and the actual current half height: 14 standing, 9 jumping/spinning. Review
found and corrected an initial fixed-height implementation that would have
expanded jump pickups by five pixels. A regression test covers this distinction.
Removal is keyed by decoded record address, preserving native shared-list
removal when a list is aliased. It never mutates the ROM or the decoded layout.

Magnetism, scattered rings, hurt-state eligibility, extra lives, scoring and
multiplayer collection are unfinished. The count is ephemeral practice state;
it cannot update the canonical MMO wallet, save, profile or rewards.

Structural references are from SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`:

| Reference path | UTF-8 text SHA-256 |
| --- | --- |
| `src/game/shared/stage/rings_manager.c` | `fe99525a59b16084231b05784002aba9b78137149de5fce478e4cb099014e73f` |
| `src/data/sa1/entity_tables.c` | `d8921767fe3c14fc12ed1779262ccc8ff15a11e27b5f3b06dbae2a8b821e5297` |
| `include/game/shared/stage/entity.h` | `98a31f08289919f1eb731630ad6b8fdb5d44e4eaf8c1bcda6b0aa221c9638bf4` |

Each CSV path/text hash is in the metadata manifest. No reference implementation
or CSV contents, native placement bytes or original art are included in Git.
