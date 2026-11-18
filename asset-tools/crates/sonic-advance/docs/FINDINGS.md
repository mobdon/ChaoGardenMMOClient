# ASOE revision 0: verified findings and remaining unknowns

Verified November 18, 2026, against the supplied local USA ROM. This is a tooling
and bounded catalog port. It establishes neither a full game port nor complete
native Tiny Garden behavior. No native save fixture was supplied for this lane.

The source is 8,388,608 bytes, title `SONICADVANCE`, game code `ASOE`, maker `78`,
revision 0, with valid fixed byte and header complement. SHA-1:
`d842afa7dd1e84de08addb94a51506f1bcafd551`. SHA-256:
`afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
Archive identity and hashes for the exact research texts are in `provenance.json`;
private source/archive paths are deliberately omitted.

`catalog.json` contains 667 bounded spans with exact byte offsets, lengths,
SHA-256, address space and evidence. A span may be a root, table, child header,
intrinsically sized resource, or explicitly labeled candidate range. Overlapping
and aliased spans are retained. Counts are catalog records, not distinct images,
sounds, reachable resources or decoded native systems. `opaque` means bytes are
preserved while their full meaning is unresolved; it does not mean unused.

| System | Verified ASOE location and representation | Remaining unknowns |
| --- | --- | --- |
| Sprites | Six-pointer root at ROM `0xD868C`, 24 bytes. Animation index `0xFE3A8`, dimensions index `0x10ACB4`, OAM index `0x119A2C`: each 906 slots / 3,624 bytes, including 140 null slots. All non-null pointers are bounded. | Per-animation frame/dimension/OAM extents, OBJ mapping/matrix modes, identity changes, runtime reachability and unused classification. Null slots are not evidence of hidden playable content. |
| Palettes | Root points to ROM `0x11A854`; bounded candidate bank span 14,848 bytes up to the 4bpp bank. 137 palette-span conversion digests match independent BGR555 expansion. | Full bank extent is inferred from pointer ordering; palette loading/bank selection and original display color response remain separate. Shared-palette map pointers remain explicit `None` with their requested count. |
| Tiles | Root points to 4bpp ROM `0x11E254` and 8bpp `0x465074`. Base map tile spans use intrinsic `tilesSize`; 65 UI candidate spans match independent 4bpp-index output hashes. | Exact full OBJ bank lengths, extra animation frames, per-call 4bpp/8bpp mode and complete resource identity. The 4bpp pointer-to-pointer span is a bounded candidate, not a proven full array size. |
| Maps | Root ROM `0x4A6C40`, 89 pointers / 356 bytes; 89 common headers parse. Foreground prefixes use 36-byte MapHeaders and intrinsic dimensions; backgrounds use 28-byte Tilemaps. | Index width and rendering mode for each layout, full metatile/collision graph, original composition and screen association. Candidate layouts remain opaque rather than guessed u16 assets. |
| Animations | Root slots as above; first variant points to ROM `0xD86A4`, 768-byte terminal-delimited script. Rust command decoding reconstructs its bytes exactly. Unknown packed operands remain raw words. | All variant-list lengths and complete control-flow graph, timing/execution parity, exact identities across region changes. The parser stops at jump/end/change and never executes branch targets. Negative opcode -13 remains unsupported until verified. |
| UI/font | Table ROM `0x680A38`, 65 rows × 28 bytes. All five scalar fields across all rows match the pinned structural locator. Example font candidate: tiles `0x658078` / 8,192 bytes; palette `0x65A078` / 32 bytes. | Exact array lengths where only adjacent pointers bound a span, glyph/text encoding, language/index identity and original layout/caller behavior. Source names are hypotheses by corresponding row, not proof of ASOE language coverage. |
| Audio | Song table ROM `0x6D76AC`, 308 rows × 8 bytes. All selectors and dummy-header aliases match the locator; 180 non-dummy headers have intrinsic `8 + 4*trackCount` sizes and bounded track/voice pointers. Zero-track dummy at `0x6D804C` is four bytes. | Human track names, sequence/pattern graph, instrument/sample extents, effects/envelopes and playback/mixing parity. Structural decoding does not render audio. |
| Tiny Garden | ROM `0x9BE78`, 114,900 compressed bytes → 138,752 bytes. Exact payload fingerprint and complete CRC table verified. Opaque program bytes can be borrowed or extracted locally without modification. | Internal garden sprites/palettes/maps/animation/UI/audio/minigame tables, native care/shop/item rules and all native minigame mechanics. Main-game UI tables do not establish garden art coverage. |
| Saves | Garden CRC table at decoded offset `0x18374`, 512 bytes, all 256 words match reflected polynomial `0x8408`. Library marker at decoded `0x17DFC`, 10 bytes. | ASOE save sectors, envelope, generation selection, checksum span/seed/finalization, field meanings and real-save roundtrip. The mathematical X-25 helper is expressly not an ASOE save validator. A2NE native-save constants are not used. |
| Transfers | Four exact literal candidate spans in garden-decoded space at `0x734`, `0x774`, `0x7A8`, `0x9A4`, four bytes each. | Literal presence establishes no JoyBus protocol, message/Chao/item/ring fields, eligibility, reachability, handshake, failure recovery or save ordering. No import into MMO progression is provided. |

The garden compressed-span SHA-256 is
`35fe9c39f06f246adbe3e806532438152f9b6c16c5efd9a2cf5928f32f082bf2`.
Decoded SHA-1 is `e2a1beec8519d36eae6478cbf3732942ac692023`; decoded SHA-256 is
`989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0`.
The CRC table SHA-256 is
`638dbc5f735cced09b3d0d767a72a8800baeb1b3bb399a514af61b3ea7be8a01`.
`garden-decoded` offsets are relative to that decoded program, never flattened
to ROM offsets or confused with the independently fingerprinted A2NE program.

## Regional differences and source provenance

The research reference is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, specifically its `sa1` data/types.
It describes the European release; no reference ROM, Europe asset offsets,
source implementation, art or downloaded source files are included here.
`provenance.json` records SHA-256 of the fetched UTF-8 research texts by path.
No source license grant is assumed; external implementation/art incorporation
would require a separate license/attribution review.

The ASOE tables were located independently in the supplied bytes using the full
65-row UI scalar sequence; the animation-prefix → variant-list → animation-root
→ six-pointer sprite-root chain; the repeated background-pointer relationship
and validated map headers; and all 308 audio selector/dummy-alias relationships.
Locator digests and reference text hashes are recorded in `provenance.json`.
The local `inspect` command reproduces every catalog range/hash check without
requiring those downloaded reference files.

Crucially, ASOE has **906 sprite slots and 89 tilemap entries**, while the pinned
European declarations have 908 and 90. At sprite index 906 the next words cease
to be valid ROM pointers. The pointer after ASOE's map index 88, at `0x4A6DA4`,
belongs to a collision table; interpreting it as a 90th Tilemap fails structure
validation. UI and audio also have different relocation deltas. There is no
safe global Europe-to-USA offset shift or same-index identity assumption.

## Verification and next gates

The synthetic suite covers truncated/oversized/malformed compression, overlap,
exact consumed bytes, pointer/range overflow, sparse pointers, file read caps,
palette channel order, tile order, flips/banks/transparency, screen-block maps,
bounded command parsing, opaque operands, shared palettes, four-byte dummy
headers and CRC-table corruption. It contains no retail bytes or native saves.

Opt-in local tests verify every one of the 667 source spans, the garden payload,
every CRC entry, the actual root/table bounds, all 65 UI rows, all 89 map headers,
all 308 song references and the first animation command byte roundtrip.
`tests/graphics-parity.json` holds 202 exact output hashes from the independent
hash-only JavaScript oracle (65 UI index spans and 137 palette spans).
Regenerate from the crate directory with:

```powershell
node tools/reference-parity.mjs $env:SONIC_ADVANCE_ASOE_ROM
```

The oracle writes metadata/hashes to stdout, never extracted pixels. Matching
these hashes proves representation parity for those bounded spans, not original
screen placement, native animation execution or a visual/gameplay playthrough.

Next verified groups should trace garden table roots/callers, then save and
transfer control flow with an explicitly supplied local save fixture; complete
sprite variants/OAM and collision layouts; and follow audio voice/sample/track
graphs before implementing playback. Chaos type → art fallback and native
minigame identity remain open. Memory/RPS/Bounce/Ball/Search MMO adaptations
are not evidence that ASOE contains every minigame or that any full game is
ported. AFK training remains an MMO rule outside this tooling.

Shared workspace/manifest adoption remains a review proposal in
[SCHEMA-PROPOSAL.md](SCHEMA-PROPOSAL.md). This PR changes only this game crate
and requires no runtime protocol migration or changes to other games' crates.
