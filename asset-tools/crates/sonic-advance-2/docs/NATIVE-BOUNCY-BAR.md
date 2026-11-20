# Native bouncy bar and input-route evidence

`native_bouncy_bar` implements interactable31's idle contact, compression and
launch task from SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`,
source Git blob `e61551855b4e7048fcb32eb82674d2cc2add8639`. It uses native
animation538, variants0–3, source hitbox0, native orientation, OAM order18 and
BG priority2. Graphics remain local and are decoded through the existing native
animation reader. Constructor initialization leaves the hitbox inactive until
the first task update.

Descending body contact from above sets character state50, queues transition5
and sound279, and sets IA override0x00400000. Landing speed selects10/15/20
compression ticks. Each compression update adds `landingPosition * 9` to Q8 Y
and zeros vertical speed. The final tick applies the original signed-byte
launch factor and releases IA override plus flag0x100. Native animation
completion returns the object to idle independently of the compression counter.

Player main must consume transition5 by selecting InitUncurl and setting0x100,
then skip callback execution while IA override is set. `init_uncurl` implements
the deferred release entry through the existing Uncurl callback boundary.
Normal animation/presentation still runs during compression. Camera culling
restores the entity marker with the original idle-contact precedence; it does
not invent a cleanup transition for an interrupted launch.

`StageBouncyBars::admit` accepts native kind31 placements in task insertion
order. `tick(player, currentBodyHitbox, camera)` returns transition, sound and
marker-restore events. It must run at native priority0x2010 before player main.
The host supplies actual body animation hitbox0 and preserves sprite attributes.

`native_bouncy_bar_oracle.py` compiles the pinned original idle/launch functions
and original body-hitbox collision locally. **4,096 cases ×24 updates ×20 state
words** match the Rust arithmetic and collision decisions:7,864,320 bytes,
SHA-256 `5589a21e0f995b692be2fd6866b56187cbd40ccb53e179a519fb7011254aa0b9`.
Display/audio are inert services and animation-end is an explicit step23 input;
this is task arithmetic evidence, not a full player/display oracle. All source,
generated C/executable and binary output remain below ignored `.local`.

The separate actual-ROM test executes animation538's four native scripts and
renders their original pixels. Variant0 ends at update index1 and remains
displayable; active variants1/2/3 end at55/40/25, safely after their respective
20/15/10 compression counters. The tested source is the verified A2NE ROM and
embedded assets, with no replacement sprites.

Reproduce with `tests/native_bouncy_bar_oracle.py --references <local pinned
sources> --zig <local zig> --local <ignored .local output> --report
catalog/native-bouncy-bar-parity.json`, then run `cargo test --test
source_native_bouncy_bar -- --include-ignored` with `A2NE_ROM` set. Ordinary unit
tests cover compression timing and inclusive hitbox edges. Source/ROM tests and
strict Clippy cover the reader/task boundary.

## Input-only route fixtures

`catalog/native-input-routes.json` contains physical input segments from an
untouched default runner spawn. The actual-ROM test submits those inputs through
`NativeHarness::tick` and asserts the recorded player/lifecycle state. It writes
no player positions, world state or saves. Native intro/countdown bypass remains
the documented runner adaptation.

The act1 fixture reaches approximately(15054,2664), a red spring beside a tall
right-hand wall, after2640 ticks with59 rings and all3 lives. The act2 fixture
reaches approximately(9687,2049), a narrow lip into a tall wall, after2040 ticks
with37 rings and all3 lives. Both passed a native checkpoint and neither reached
a goal. These are reproducible traversal/geometry observations, **not full-course
or full-game acceptance**. Exploratory search code and native screenshots remain
local; committed tests contain only the small exact-input fixtures.
