# Sonic Advance ASOE: shared Rust port

Desktop and browser now step the same `NativeRun`. The browser loads a private ASOE ROM into memory, renders the English title in Rust and can enter Neo Green Hill Acts 1 and 2 from the practice selector. The title's Start event is verified; the original main menu does not yet follow it. Desktop can load the first twelve ordinary act slots. Loading a slot does not prove its completion.

This continuation is stacked on desktop PR #13, which is based on catalog PR #4. It incorporates the verified title and browser presentation from PR #14 at `54e04a0d6505296400d9aa8e34bad1d34115350e`. Its flat/upright player and second gameplay loop were not imported. Both of that branch's reported terrain regressions now have tests against the original game. The same native dust clips (746/747) are driven by the shared sprite VM.

| System | Implemented here | Still open |
| --- | --- | --- |
| [Terrain](STAGE-EVIDENCE.md) and [Sonic](PLAYER-EVIDENCE.md) | Original stage and collision graphs, four-direction sensors, collision layers, dry run/jump/roll/spindash, ordinary single-player camera | Full traversal through every act, water/inverted gravity, other characters and abilities |
| [Entities](ENTITY-EVIDENCE.md) | All 54 layouts; layer switches, [springs](SPRING-EVIDENCE.md), upright spikes, [item monitors](ITEM-EVIDENCE.md), [checkpoint/goal posts](POST-EVIDENCE.md), rings and [HUD](HUD-EVIDENCE.md) | Enemies, bosses, remaining hazards and platforms, scattered rings, complete results and special modes |
| [Music](AUDIO-EVIDENCE.md) | Score and voice readers; [offline stereo synthesis](MIX-EVIDENCE.md) for Neo Green Hill and ring effect | Hardware waveform fidelity and interactive audio device playback |
| [Saves](FLASH-EVIDENCE.md) and [transfer](TRANSFER-EVIDENCE.md) | Read-only native slot selection and Tiny Garden data-phase handshake comparisons | End-to-end hardware exchange and validated save writing |
| Tiny Garden | [Chao Memory](TINY-GAME-EVIDENCE.md) and [Rock-Paper-Scissors](TINY-RPS-EVIDENCE.md) game logic tested against native execution | Garden room/care/shop, full minigame presentation, lifecycle and integration |

For commands and reproducible evidence see [validation](NATIVE-VALIDATION.md). The stand-alone practice runtime makes no network reward, save or account mutations. No native assets, ROMs, flash images, generated WASM or screenshots are published.
