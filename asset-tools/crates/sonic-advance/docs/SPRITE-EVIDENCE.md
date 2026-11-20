# Native ASOE sprite execution and composition

Verified November 18, 2026 against the local 8,388,608-byte ASOE revision-0 ROM,
SHA-256 `afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020`.
`SpriteBank::from_asoe` requires that complete fingerprint before resolving
version-specific addresses. No proprietary script, image, palette or ROM bytes
are included in the Rust implementation or synthetic fixtures.

This extends the original terminal-delimited command reader into a bounded
state machine and a normal unscaled OBJ compositor. `SpritePlayer` owns native
Q8.8 delay, Q4.4 speed, selected animation/variant, cursor, hitboxes, image frame,
palette/OBJ-memory contents and command side effects. Native offsets, palettes,
pixel indices, anchors, template flips and part ordering are preserved.

## Verified locations and selection

The six-pointer root remains ROM `0xD868C`. Its animation, dimensions and OAM
indexes are `0xFE3A8`, `0x10ACB4` and `0x119A2C`, each 906 sparse slots. OBJ
palette, 4bpp and 8bpp roots are `0x11A854`, `0x11E254` and `0x465074`.
The source-gated loader preserves native null slots and aliases.

The pinned reference identifies Sonic idle 0, walk 4, run 5, spin/jump-ball 8,
airfall 10 variant 1, ring 707 and collection effect 708. The selected ASOE
program sequences, variant records, dimensions and OAM parts were checked in
the supplied ROM; no global Europe-to-USA relocation is used.

| Slot / variant | ASOE program offset | Bytes before variant table | SHA-256 |
| --- | ---: | ---: | --- |
| 0 / 0, idle | `0xD86A4` | 768 | `8445dbf890ab13fa1e6cd9508cce84d562436009890e8ffc0a36f75a8888e02b` |
| 4 / 0, walk | `0xD8E5C` | 192 | `aa403db60adfca1663f13ea286bb67f92b691d9ad2b964cc920d2f4a6ee34c4b` |
| 5 / 0, run | `0xD8F20` | 112 | `341e0600d73f66ee4f3225081de97e0284ab928b8be44357cbc9cf85d971ba1e` |
| 8 / 0, spin | `0xD904C` | 124 | `608bc84205b08eb54f905bc49db1a85441e6985e5fd2387ba605d1b6789ab8f1` |
| 707 / 0, ring | `0xF502C` | 64 | `32e8505bf4d90c96ea328d1b46873ab9bd4c83c42fdb7b5cd4ef74d6cb573085` |
| 708 / 0, collection | `0xF5070` | 68 | `5015bb9209802191a7ec8d7084c41306c1ae30f1de66467bafe4bcac87d61e98` |

The first idle dimensions record is at `0xFF1D0`: 32×40, anchor (16,24),
two subframes. Its OAM template begins at `0x10BADC`: one 32×32 part and one
32×8 part. All 20 loaded tiles contribute in native one-dimensional tile order.
The walk/run records describe 64×64 canvases with anchor (32,48); transparent
padding is retained rather than shrinking the artwork or changing attachment.
Ring dimensions start at `0x108770`, OAM at `0x1164C8`; its first canvas is
16×16 with anchor (8,16).

Ring 707's program loads 16 tiles from native index `0x15124`, copies palette
68 into OBJ colors 96–111, shows four frames for eight calls each at normal
speed and jumps back 11 words. Collection 708 uses the following native tiles,
emits sound 117 once and ends after those four eight-call frames. Emitting the
sound ID is not audio playback.

## Execution and rendering semantics

- At normal speed (`0x10`), a six-frame record appears on calls 1–6, and the
  following record is selected on call 7. Half speed (`0x08`) doubles the call
  count. Delay is a signed Q8.8 native value; no floating-point clock is used.
- `TILES` updates the beginning of a bounded 32 KiB OBJ-memory representation,
  retaining prior bytes outside the copied range. `PALETTE` updates only its
  specified destination colors. Index-zero transparency is a renderer rule;
  BGR555 palette values themselves remain opaque.
- Branch offsets are signed word distances from the branch command. Programs
  include the same animation's variant group, permitting native shared labels.
  Entries and targets must be instruction boundaries inside that bounded group.
  Animation changes enter the new program immediately and invalidate the prior
  variant as in the reference; the ordinary next call initializes it again.
- Sounds, hitboxes, translations, priorities, OAM order and animation changes
  are exposed as events. Translation uses native signed-16-bit wrapping, and
  active hitboxes mirror with caller-facing flips. All-zero boxes become inactive.
  Opcode -8 is a verified no-op that preserves/emits both operands.
- `DisplaySprite` clears the template's affine-on bit before applying caller
  transforms. Walk/run templates have that bit set; normal rendering clears it
  exactly rather than rejecting the native art or inventing substitute frames.
- Frame-level and caller-facing flips reflect both subframe placement and pixel
  orientation. The attachment anchor changes to width-minus-anchor or
  height-minus-anchor for caller-facing flips. Lower OBJ priority wins overlap;
  lower subframe/OAM index wins when priorities match.

One step is limited to 256 commands, a program group to 64 KiB, a variant list
to 256 entries, OAM to 128 parts and dimensions to 512×512. Range arithmetic,
native table bounds and memory/palette budgets are checked before copying.
A failed step does not commit partial sprite state. Zero-time loops, branches
into operands/outside the group, truncated records, unavailable variants,
palette/OBJ overflow and unknown commands are errors.

## Validation

Run from the isolated worktree with the private source path set:

```powershell
$env:SONIC_ADVANCE_ASOE_ROM = 'C:/private/.local/ASOE.gba'
cargo test --offline --locked --manifest-path asset-tools/crates/sonic-advance/Cargo.toml --test sprite -- --include-ignored
```

All seven sprite tests pass. Five synthetic tests cover delay/half-speed/loops,
end/hidden frames, side effects, selection changes, malformed programs and OAM
template modes. Two opt-in tests additionally exercise the actual source:

- 3,600 ticks and renders across idle, walk, run, spin, fall and ring, including
  native loops and multiple distinct frames. Collection sound/end behavior is
  checked separately. Native horizontal reflection is checked pixel by pixel.
- 35 exact RGBA output hashes match a separately written JavaScript reference
  reader/compositor at calls 1, 8, 16, 32 and 64 for those six animations plus
  collection. The public fixture contains hashes/dimensions/anchors only.
  First idle hash is
  `90318f22d1954f5bd3e6fe97334c93c456a6d7a9aeeb8462712ebe5c876fef2f`.

The independent oracle is local research tooling, not game authority. Its source
SHA-256 is `0e73cd50f07b711b3de1da10864592a465d78d196be20a74f3b48ddf9075fc80`.
Matching host output establishes representation/timing parity for these tested
states; a hardware gameplay trace and complete native engine remain separate.

## Reference provenance and remaining limits

Primary research reference: SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Original Rust code was written from
format/behavior evidence; downloaded implementation files remain local.

| Reference path | SHA-256 of downloaded UTF-8 text |
| --- | --- |
| `src/sprite.c` | `d627275068de1b957fbeb9243291fe66f407e85e47a42d0760c15a13c1067a8d` |
| `include/sprite.h` | `d27363063987dee2ac3a9d841df8d5e73bf773b17f231b1ea6c02048da4f23dc` |
| `include/core.h` | `6b940bf2d0dff3499fe0ae787773a7598a2cb16eee1d791c229d3b7eb1cac0ef` |
| `src/data/sa1/oam_data.c` | `27c60aa1513efece7ae6d3736b92cf587f79e75485044084797ee730bf9ddfa7` |
| `src/data/sa1/sprite_dimensions.c` | `dd2bbf117c006289ffacbb2346d822747fe92a2e27033f09bb79d5c901f11423` |
| `include/constants/sa1/animations.h` | `2bd60bad3f75ff05abbf71af4cdd8d1ebff3ede65380785c2bac9dc85eebc5bb` |

Actual affine/rotation/scale transforms, double-size/hidden template modes,
blend/window/mosaic behavior, global ordering against other actors/backgrounds,
palette effects, VRAM scheduling and sound playback remain outside this normal
compositor. Opcode -10's pinned handler has an undefined pointer-valued return;
the VM rejects it instead of choosing a new behavior or silently enabling the
reference's optional bug fix. Durations above 127, which overflow positive native
Q8.8 delay, are also rejected. Other animation IDs may use unsupported commands,
empty/special dimension records, shared graph arrangements or rendering modes.
The selected Sonic and ring states are verified; this is not a claim of parity
for every reachable sprite or a complete native gameplay port.
