# Native sprite animation execution

`native_animation.rs` executes original A2NE animation words and composes their
original non-affine OBJ images in Rust. It accepts only `VerifiedRom`; the ROM
and embedded garden fingerprints remain mandatory. No ROM/image bytes are
included in this source tree.

The pinned behavior reference is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`:

| Reference | Git blob | Used evidence |
| --- | --- | --- |
| `include/constants/anim_commands.h` | `284bc7dde6cc1cfa358b52207cacad8e6cdb2dce` | Opcode IDs, packed operands and 1,133 slots |
| `include/core.h` | `7da347250fa488b8002227e59a1bf8522e1ade41` | Twelve-byte frame dimensions and sprite/table structures |
| `src/sprite.c` | `396cb0291f8ee4ad5744755f88cf5ee9608a7914` | Command execution, Q8.8 timing, six-byte OAM composition and origin |
| `include/constants/sa2/animations.h` | `55638f02ec05405324d6dfc2cf182789096f9745` | Character stride 91; Idle 0, Walk 9, Jump 10/11, Brake 7 |

The six pointers at ROM offset `0xF40D4` select the animation index
`0x135EC4`, dimensions index `0x14D1C4`, OAM index `0x169C14`, palettes
`0x16ADC8`, 4bpp tiles `0x1709A8` and 8bpp tiles `0x6E9E08`. These are
file offsets, not CPU addresses. The animation index contains 1,133 slots:
991 non-null references and 142 explicit null entries.

| Signed opcode | Meaning | Native words |
| --- | --- | --- |
| -1 | Copy original 4bpp/8bpp tile range to the sprite's OBJ buffer | 3 |
| -2 | Copy native palette bank/color count/insertion offset | 3 |
| -3 | Relative jump measured in 32-bit words | 2 |
| -4 | End, retaining the current image | 1 |
| -5 | Emit original sound ID | 2 |
| -6 | Set/inactivate and mirror a native hitbox | 3 |
| -7 | Wrapping signed sprite translation | 2 |
| -8 | Preserve operands and perform the native no-op | 3 |
| -9 | Change animation and variant | 2 |
| -10 | Explicit error: retail handler returns its code pointer | 4 |
| -11 | Set sprite priority | 2 |
| -12 | Set OAM order | 2 |
| Nonnegative | Display frame index for a native delay | 2 |

`advance()` represents one original 60 Hz update. Native speed `0x10` is 1x.
A positive Q8.8 delay is decremented on its own call; reaching zero does not
also consume the next frame on that call. The native animation-change
`prevVariant=-1` reset on the following call is retained.

The renderer preserves each frame's logical origin; callers place it at
`position - origin + translation`. It resolves original shape/size, tile
index, palette bank, part flips, frame flips and overlapping OAM priority.
Original `DisplaySprite` clears raw template attr0 bit 8 with mask `0xFE00`
before selecting a live affine matrix. Walk templates set that bit, so it
must not be mistaken for an active matrix. This module renders the ordinary
non-affine path. Pixel index zero is transparent.

Validation includes six synthetic tests for truncated commands, invalid
uploads, cursor loops, state changes and OAM modes. Actual-ROM tests execute
400 Sonic Idle updates and all 13 native Idle images. Three independently
decoded primary PNG images match the composed pixels. A further 9,000 updates
exercise all five characters' six Walk variants, Idle, Brake and Jump 10/11:
50 native animation/variant combinations. That broad execution check is not
an independent pixel oracle for every character.

`catalog/native-sprite-parity.json` contains only the primary PNG Git blobs,
dimensions and expected hashes. Reproduce its hashes from local reference
files without Pillow or Rust output:

```powershell
python tests/independent_sprite_images.py --reference-dir <local-reference-folder>
$env:A2NE_ROM = '<local-verified-A2NE.gba>'
cargo test --test source_native_animation -- --ignored
```

The reference PNG exporter floors `channel5*255/31`; this tool's established
RGB expansion uses bit replication. The independent verifier restores the
original five-bit channels first and applies that documented expansion.
Invisible RGB is canonicalized to zero. Geometry, indices, palette words,
alpha coverage and anchors are preserved.

Host bounds are explicit: 256 commands per update, 16,384 cursor words,
32 KiB OBJ tile memory, 256 palette words, 128 parts and at most 1,024×1,024
output pixels. Unknown opcodes, unsupported live affine/mosaic/blend/window
paths and opcode -10 fail explicitly. Variant pointer membership is checked;
this does not establish exhaustive variant counts or every script's grammar.
Sound synthesis, hardware DMA scheduling, global sprite transforms, the full
game loop and gameplay state selection remain separate port work. No MMO
identity, stat, reward or save mutation is performed by this module.
