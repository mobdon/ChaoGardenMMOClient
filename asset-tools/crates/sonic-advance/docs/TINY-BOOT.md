# Tiny Garden boot handoff (ASOE)

The Rust entry point is garden::prepare_launch. Pass the retail ROM, an in-memory loaded save structure (not raw flash), and the current 32-bit frame count. The returned bytes cover the Garden program in EWRAM immediately before soft reset.

ASOE SHA-256: afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020. The compressed program starts at file offset 0x9be78 (114900 bytes). Its 138752 decompressed bytes hash to 989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0 before configuration.

Independent parity: execute retail Thumb instructions at ROM offsets [0xe082,0xe0a6) in Unicorn, mapping the ROM at 0x08000000, preloading the verified program at 0x02000000, the loaded save at 0x03005160 and frame word at 0x03001b24. The routine stores the save score (+0x428), zero-extended language byte (+0x19), and frame plus checksum (+0x42c, wrapping) as little-endian words at program offsets 8, 12 and 16. The Rust test checks the entire resulting image against three native digests, including overflow. No real save was used.

Handoff: next trace the reset entry through the first interactive Garden screen, then record the Garden-to-minigame transition and RNG consumption before Memory's deal. Existing Memory and Rock-Paper-Scissors logic and traces are documented in TINY-GAME-EVIDENCE.md and TINY-RPS-EVIDENCE.md. The boot configuration does not prove either minigame receives the launch seed unchanged. No browser entry, care system, server settlement or save write is connected by this change.

## Garden input after reset

Run tools/retail-replay.py with the local ASOE ROM, pinned mGBA DLL and --garden-input to reproduce tests/retail-garden-input.json. From a cold boot, the replay selects Garden, holds Right on frames 1701â€“1720, presses A on 1721 and releases on 1722. The fixture records frame, held, inverted held, newly pressed, repeated, countdown, initial delay, repeat interval and framebuffer SHA-256. This is evidence of input reaching the retail Garden after reset; the frame hashes are references for a future Rust renderer.

The Rust reader used by both minigames matches the ASOE decompressed program at offsets 0xb2c (initialize) and 0xb50 (poll). An independent Unicorn run hashed all 11 input-state bytes after each of 1024 polls with soft reset inhibited at IWRAM 0x0300276f bit 6. Preserve the unsigned comparison of old and new key masks: switching from Right to A records a new press but no repeat. The reader signals the reset chord; system teardown/reboot is not implemented.

## Garden background and graphics banks

At frame 1700 on the cold-boot Garden route the following six LZ77 streams match entire contiguous ranges in video memory. Source offsets are relative to the decompressed Garden program. VRAM offsets are relative to 0x06000000. Lengths exclude padding. The last two banks reside in object graphics memory; individual objects and their animation assignments are not identified by that match.

| Program offset | Packed bytes | Decoded bytes | VRAM offset | Packed SHA-256 | Decoded / VRAM SHA-256 |
| --- | --- | --- | --- | --- | --- |
| 0x18FE4 | 0x9F9 | 0x1E00 | 0x8000 | b44e93cc6dfc677891e4f87d0f2fc1c0e2cfee56a760a78726937836cb5f61a4 | 670e48c630f57c9d7fe3b6c279e78b9432ffc33163bb6aca0b33b5337158d4d9 |
| 0x19F08 | 0xB56 | 0x2A20 | 0x55E0 | db953ae601fe110fe3864fce4033fd5501a238b7cc8e31e9672807497e72f9b1 | 74ce59508936bb7f05a8319ea0f338fef1c1d7b521598f3a838fe2c477d72e10 |
| 0x1AA60 | 0x6C9 | 0x1400 | 0x41E0 | c164b2749b47ec856c06fcd55ac6a3f7144067e7964e2384a6d6595924e75375 | b398eb1580836b60e7d730d97d0e12addf5955589a6fde6e333c3dbed1d19ecb |
| 0x1B418 | 0x1019 | 0x25E0 | 0x1C00 | b2f24dcf087845e43efeec3be29537b9d5f44f0ca2384ead6563da5fe1255762 | aff6296ccec895477bbf0515abb627cbf43a710138a43c0ea4707df60ceb0f7a |
| 0x1CA40 | 0x1C90 | 0x5260 | 0x12D80 | 778cdc80982a78211a6023661f92eb040f95fcf1bca3830be34db1d220eb18ae | 226fff7c4ebe55795ed46e1199f28a50ff06cabecee346474e3c228910f847a6 |
| 0x1E6D0 | 0xD1D | 0x1800 | 0x11580 | 5637d33ebef5ef54b0611bbf6f73efd671796e975ca4d5c35922b770872d8a6b | ef9653c638619f5e6ec117cf61cba8058b0b49cec2c8e620dc25bd86adcf75c2 |

BG3 uses the first bank as 8-bit tiles. Its 22×20 map starts at 0x199E0 (880 bytes, SHA-256 05bb04f9013c42a4cca06d9159f5fcd36d6e29ab6abf2ef06bff7c9a16603a36). The program map's tile IDs are relative to this bank; the live map at VRAM offset 0xF800 adds 0x200 and pads rows to 32 entries. The 512-byte BG palette starts at 0x1B218 (SHA-256 3d8663b493740046758985db3c5897c3683d518bee45c8e932c66cb974f76e2f). Run the local retail replay with --garden-background to compute the independent BG3 pixel digest from live VRAM and palette RAM. The Rust background decoder matches this layer before overlay, scrolling or sprite composition.

## Browser practice route

The Rust/WASM menu now opens a local Tiny Garden practice scene. Use the arrows to choose Memory or Rock-Paper-Scissors and Z to open it. Memory shows the deal before Z begins the matching round. In the hands game, use Left/Right and Z to fire. Z returns after a result; X leaves practice, and X at the Garden returns to the main menu. The text view is an adapter for the Rust rounds. The local practice seed is elapsed Garden frames; the exact retail seed history and pre-deal Chao shuffle are not reproduced.

The canvas combines BG3 with the four active OAM entries observed on frame 1700. Their three attribute halfwords, in OAM order, are 0034/4057/14AC, 0059/4050/ECD8, 4004/8070/CD0D and 4004/8090/BD0D. The source OBJ bank starts at program 0x1E6D0 and the palette at 0x1C500. A ROM-gated test compares every color and transparent texel against the four independently decoded live objects. This scene uses their sampled poses; it does not yet animate the actors or draw the retail stats panel.

Run tools/verify-garden-browser.mjs with the local ROM, Chromium executable and Playwright Core module after building the WASM crate for wasm32-unknown-unknown. The test loads the ROM through the page, navigates with keyboard events, reads the Memory preview, completes seven pairs, enters the hands game, fires, waits for the round to stop, then returns through Garden to the main menu. This practice route has no account, save or wallet side effects. Retail care, shopping, save/reload, transfers, minigame art and audio are still open.
