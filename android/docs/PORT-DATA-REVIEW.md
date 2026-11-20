# Ported data used by the widget

Reviewed November 18, 2026: actual histories of the four game asset workstreams,
including both Advance implementation/review histories, plus their available
GitHub deliveries. Local source was inspected as well; a PR description alone
was not treated as runtime integration.

| Source | Delivery checked | Useful verified data | Remaining boundary |
| --- | --- | --- | --- |
| Sonic Advance 2, A2NE | Client PR #1, `1a46b2f5095f7a5ae36aecedd4a10dd6faa8ba58` | Fingerprint-gated Rust LZ77, palettes, tiles, maps and garden source catalog. Used directly by this exporter. | Complete OAM/animation playback, native Chao type mapping and full Memory/Bounce behavior are not delivered. |
| Sonic Pinball Party, A3VE | Client PR #2, `2aa36c5341c09de04ff0c4ec52ba6ba124839325` | Independent garden payload, graphics/table readers and parity evidence. | Chao Ball graphics/dispatch candidates are not confirmed native runtime semantics. No A3VE offset is borrowed for A2NE. |
| Sonic Advance, ASOE | Client PR #4, `c5ccc2533162c0ad34262dfd4438c3848713e97b` | Exact cartridge and decoded garden fingerprints; bounded main-game readers. | Garden internals/art/minigame tables remain opaque. Main-game UI is not relabeled as garden art. |
| Sonic Adventure 2 + BATTLE | Local clean `c6cd348a4f5d57ae6d1ef3a71264d93d9f338d8c`; no matching GitHub PR found during review | Source-verified textures and Chao UI; existing desktop-model phone atlases preserve canonical Chaos forms. | It is not a GBA Tiny Garden engine or sprite port. The phone atlases are a presentation adaptation. |

The selected static background is A2NE's original 22×20 map, palette and native
VRAM uploads, rendered by Rust into 176×160 pixels. Independent pixel parity,
hashes, reproducible commands and scope are in [TINY-GARDEN-ART.md](TINY-GARDEN-ART.md).
The browser and native Android source check the same PNG fingerprint.

Needs, all seven stats, rings, resident activity, identity and care still come
from the Rust service. Native bytes, local clock values and the cached widget
never grant items or progression. Original GBA stat/save records are not
uploaded as claimed MMO gains. Missing AFK service data remains unavailable.
Memory, RPS, Bounce, Ball and Search open in the full Tiny Garden app; this
background does not establish full native mechanics for those games. In
particular, the supplied source set still lacks the original SADX Search
payload.

No other workstream's source, index, live save or running service was changed.
The widget-owned browser page remains the phone preview. The reviewed full Tiny
UI delivery from client PR #5 (`f4f2f05`) is included as a source dependency;
its five screens now open from this branch with the original English sidebar.
That static layer does not implement native UI state machines or game rules.
Rust accounts draft server PR #15 at `56af22d`
supplies the agreed native pairing/account/lease/intent transport and canonical
offline self; source review and isolated tests do not deploy it to the live
legacy service.
