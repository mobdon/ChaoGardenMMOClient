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

Next integration: implement the native opening and carousel graphics, then pass the
original input registers to Carousel from the shared host. The browser still pauses
at Single Player. Sound, post-confirmation scene routing and multiplayer selection
are outside this implementation.
