# Tiny Garden boot handoff (ASOE)

The Rust entry point is garden::prepare_launch. Pass the retail ROM, an in-memory loaded save structure (not raw flash), and the current 32-bit frame count. The returned bytes cover the Garden program in EWRAM immediately before soft reset.

ASOE SHA-256: afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020. The compressed program starts at file offset 0x9be78 (114900 bytes). Its 138752 decompressed bytes hash to 989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0 before configuration.

Independent parity: execute retail Thumb instructions at ROM offsets [0xe082,0xe0a6) in Unicorn, mapping the ROM at 0x08000000, preloading the verified program at 0x02000000, the loaded save at 0x03005160 and frame word at 0x03001b24. The routine stores the save score (+0x428), zero-extended language byte (+0x19), and frame plus checksum (+0x42c, wrapping) as little-endian words at program offsets 8, 12 and 16. The Rust test checks the entire resulting image against three native digests, including overflow. No real save was used.

Handoff: next trace the reset entry through the first interactive Garden screen, then record the Garden-to-minigame transition and RNG consumption before Memory's deal. Existing Memory and Rock-Paper-Scissors logic and traces are documented in TINY-GAME-EVIDENCE.md and TINY-RPS-EVIDENCE.md. The boot configuration does not prove either minigame receives the launch seed unchanged. No browser entry, care system, server settlement or save write is connected by this change.

## Garden input after reset

Run tools/retail-replay.py with the local ASOE ROM, pinned mGBA DLL and --garden-input to reproduce tests/retail-garden-input.json. From a cold boot, the replay selects Garden, holds Right on frames 1701–1720, presses A on 1721 and releases on 1722. The fixture records frame, held, inverted held, newly pressed, repeated, countdown, initial delay, repeat interval and framebuffer SHA-256. This is evidence of input reaching the retail Garden after reset; the frame hashes are references for a future Rust renderer.

The Rust reader used by both minigames matches the ASOE decompressed program at offsets 0xb2c (initialize) and 0xb50 (poll). An independent Unicorn run hashed all 11 input-state bytes after each of 1024 polls with soft reset inhibited at IWRAM 0x0300276f bit 6. Preserve the unsigned comparison of old and new key masks: switching from Right to A records a new press but no repeat. The reader signals the reset chord; system teardown/reboot is not implemented.
