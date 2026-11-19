# Single Player character carousel: input handoff

The Rust Carousel starts on the first active selection tick, after the opening.
It accepts the pressed and repeated GBA key registers separately. Its public
angle is in 1/256 turns between adjacent characters (1024 units per revolution).
It returns a single confirmation or back event. A completed secret input sequence
is reported on confirmation; this reader never enables a companion or writes a save.

The reference is the same USA ASOE ROM and mGBA core identified in MAIN-MENU.md.
With tools/retail-replay.py LOCAL_ROM LOCAL_DLL --character-input, the original
game cold-boots, enters Single Player, and initializes the selection task at
IWRAM 0x030026F4 on this route. The fixture captures frames 1415–1600 inclusive.
Each row is [video frame, input pulse, task age (+0x24), angle (+0x28), settled
slot (+0x2A), requested slot (+0x2B), direction (+0x2C), steps left (+0x2E),
partner-sequence progress (+0x39)]. All inputs on this route are single-tick
pulses separated by releases. This tests every tick of a left/right conflict,
queued turns, return to Sonic, and the CPU Tails code through confirmation.
It does not establish the outcome of spawning the companion.

ASOE file offsets (exclusive ends):

| Resource | Range | SHA-256 |
|---|---|---|
| Partner input check, Thumb code and literal pool | 0x5B7B0..0x5B822 | a4f5d235faf5c231cf5610925cfd636a7c9d857239c97b30696fd329e79e06fa |
| Key sequence including two zero halfwords | 0x681514..0x681528 | 7cbda7b748fc704d396cf7476377c01c348226ab9d63c7ad65a58be09efd3b8c |

The code's pointer at 0x5B814 addresses the key table at 0x08681514.
Behavior reference: SAT-R/sa2 revision 2dd3f17d30cd3c156e508aac0017620ae485b9ba,
src/game/sa1/ui/character_select.c, Task_805A060 and CheckCPUPartnerUnlock.
Run cargo test --release --offline --locked --lib character_select to compare
the Rust state to the published numeric trace; the test needs no game files.

Single Player now opens the native carousel in the browser. The Rust screen draws
the opening fade, BG1 tile strips, original character portraits, window brightness,
arrows, scrolling header and character nameplate. Left/Right select, Z/Space
confirm, and X returns to the menu. A/S supply the GBA L/R shoulder buttons.
Confirmation plays the original animation and fade for all four characters.
In a fresh browser session solo Sonic then starts the existing Act 1 runtime.
The runtime starts at its playable spawn: the native act introduction is pending.
Other characters and CPU Tails stop after the confirmation fade; they never load
Sonic in place of the requested character. Course select for an existing save,
the return fade and audio remain pending.

Run the same replay tool with --character-video to regenerate only hashes:
200 consecutive complete displayed frames (1401–1600 inclusive), including
entry, settling and turns in both directions, against the fingerprinted local
mGBA core. Compare them with cargo test --release --locked --offline --test
character_screen -- --ignored and SONIC_ADVANCE_ASOE_ROM set. The final video frame
precedes the visual response to confirmation. This is emulator video parity
on this route, not a hardware or all-input equivalence result.

Animation 722 supplies character portraits; 723 supplies the backdrop. Four
backgrounds use 600 4bpp tiles each in vertical strips of 8, 8, 8, 4 and 2 tiles.
During rotation the four source variants display in order 3, 0, 2, 1. UI table
entries 39–43 supply the bands, arrows, four names and scrolling header. The UI
elements share the arrow palette except for the four character-specific band palettes.

| Source region | ASOE file offset | Bytes | SHA-256 |
|---|---:|---:|---|
| Portrait scripts and variant pointers | 0xF6B54 | 600 | b784e679ca82b24dd64e97ddb3061c855b28b5a9d6b39dd32e16e0f31c1e77fc |
| Background scripts and variant pointers | 0xF6DAC | 160 | 9632dd4309a8ca907a4f0c133b5094b683f8f0c3b140c79b81d73ee00ec2679d |
| Background variant 0 | 0x3D33B4 | 19200 | 978612b0ce020d2918b1f35a23adee393465b60180d96acc0476f7895df1ba6b |
| Background variant 1 | 0x3D7EB4 | 19200 | 36eb658ef023988b1a29fbacfc0839f79858445a7b3e0187444696bc9c3108ec |
| Background variant 2 | 0x3DC9B4 | 19200 | 30d73c80751645f4c9bef23919c944e1d4c37cdbdcb55380174c91db6c81150f |
| Background variant 3 | 0x3E14B4 | 19200 | 7669123b34996c0256c24446e5854b555de2c20b2d6fd56cf0e2a496a602bca8 |
| Background palettes | 0x11B8B4 | 96 | 4f559ee55e8eeb6c76e7c963dc41d6e92220ad77d190226d8265e3cbd81a3cc3 |
| Band tiles | 0x670B78 | 1024 | 84b66e773ba5b35fccc931eed2126426893390afc1d2446eb2e37d868d2535df |
| Band palette windows | 0x670F78 | 26 | a730e16c65f93f6585b04570ccc4816de2f67d758757c56b44b97c998783adfb |
| Arrow tiles | 0x670F98 | 1024 | f18d95bb36b606323bce41518d560f2cabce027b71294fcea687201a606d18b5 |
| Shared object palette | 0x671398 | 32 | 4e98c3db478a026c7bdc64df880895eeaed50c4580eff00e6d12c82587f6a938 |
| Character names | 0x6713B8 | 12288 | f5738ba611faf5513b120f9f8f637671766ce61a0fbc589bed827d76b2e75302 |
| Header letters | 0x6743D8 | 2048 | 159edfc0258ddf0e71e9ae9caf5bafc8b027e4f3fbbfce512f990b70c9266640 |
| Header backing | 0x674BF8 | 256 | 3d6876a0146de8576eb2395a858de1213d1b92c65b779df3a331cfd5a4584546 |

Confirmation evidence: tools/retail-replay.py accepts --confirm-sonic,
--confirm-tails, --confirm-knuckles and --confirm-amy. Each local cold boot uses
the first 0–3 Right pulses at frames 1480, 1520 and 1560, followed by A at 1600.
The Rust test compares every full BGR555 video frame from 1601 through 1657:
57 per character, 228 total. It covers the odd-numbered animation 722 variants,
outward motion of the side portraits, disappearance of the arrows, slowing header,
and final fade. The confirmation header tiles occupy ASOE file span
0x674D18..0x674F18 (exclusive end), SHA-256 e726d23c277c23447ab4da7f4b007294dec43e035d50b88bdb519a8a07f7fb7e.

The retail Single Player task starts level 0 when the selected character has
zero unlocked levels; otherwise it calls course select. See Task_805AAF8 in the
pinned reference above. The browser currently has no loaded flash save and only
the solo Sonic player is supported. Chromium exercises fresh Sonic from title to
Act 1, then crouches, charges and releases a spindash without using the practice
selector to enter the act. This check does not certify the missing stage intro.
