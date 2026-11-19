# Offline story stage loading

The Rust Campaign index opens the three single-player story arrays in the supported Steam executable. For a chosen **level action** it resolves the authored character and level, the original single-player start row when present, the native object table, the normal SET placements, and the named static landtable when available. It can build a collision world at native scale and resolve a placement to its object definition. The mounted result distinguishes missing resources from unimplemented gameplay. It does not run a stage to completion.

From the client worktree, with the local installation and SA Tools 786 configuration paths:

```powershell
$tool = 'asset-tools/crates/sa2/target/debug/sa2-assets.exe'
$exe = Join-Path $game 'sonic2app.exe'
& $tool --source-root $game fan-levels $exe .local/story-levels.json $config
& $tool --source-root $game fan-stage $exe .local/story-city.json $config 'Hero Story Sequence' 1
& $tool --source-root $game fan-stage $exe .local/story-route280.json $config 'Dark Story Sequence' 35
```

The ordinal counts each event slot as one action and stops at that entry's first sentinel. For example, Last Story's Final Hazard is action 10. Reading a level by ordinal is a local seek. It never advances a save, acknowledges an outstanding gameplay result, or issues Chao rewards.

## Installed campaign evidence

These are the actual ordered **level IDs** in the story tables (events and credits omitted). The local test includes every corresponding ordinal and character, including different characters for reused boss stages.

| Sequence | Level dispatches | Authored level IDs in order |
|---|---:|---|
| Hero | 24 | 13, 60, 16, 20, 9, 10, 19, 3, 5, 17, 7, 70, 23, 28, 25, 63, 64, 24, 32, 33, 22, 29, 30, 42 |
| Dark | 22 | 11, 61, 18, 21, 14, 26, 27, 12, 20, 8, 62, 4, 19, 70, 6, 67, 44, 33, 43, 29, 40, 42 |
| Last | 3 | 36, 65, 66 |

All 49 dispatches mounted or reported their precise resource gaps. The result contains **94 layer mounts from 84 distinct SET files**, including both normal kart SETs. Across those dispatches, all **24,162 placements** resolve against their selected native definitions. Each placement retains layer identity, source offset, packed ID interpretation, transform and parameters. Definitions include original names, flags, load distances and function addresses. A resolved definition does not implement its function.

**43 of 49** dispatches have a start row for the exact authored character. Big Foot (60), Hot Shot (61), Flying Dog (62), Biolizard (65) and both kart dispatches (70) have no matching general single-player start row. No character substitution or invented start position is used. Final Hazard (66) has an authored pose and a level section, but its configured BossLast2 object-list path lacks a matching mapping in ObjectLists.ini. No normal SET or static landtable is declared/available there.

**44 of 49** dispatches mount verified static terrain from **40 distinct levels**. All 40 built a Rust collision world. Sonic vs Shadow 2 (42) has no static landtable in either story; Final Hazard and both kart courses also have none. Some boss tables contain only visual entries; the existence of a landtable does not imply a solid floor or a port of a boss routine. The original five Cannon's Core sections are not five independent Last Story dispatches: the story selects 36. In-level character handoffs are outside this loader.

## Kart routes

Both kart actions dispatch stage 70, but Hero selects MechTails and Dark selects Rouge. SA Tools 786 PC_SA2/CART.ini names two distinct eight-byte descriptors in the executable: Route 101 at RVA AF82BC and Route 280 at AF82C4. Each gives a preferred pointer and a count of ordered byte-sized part IDs. The normal character-specific SET files resolve against the original 13-record Kart Object List at RVA C0A230. These are offline associations of installed assets; no hard-mode, minigame or base SET is substituted.

| Route | Course part bytes | SHA-256 of part IDs | SET placements |
|---|---:|---|---:|
| Route 101 | 321 | e93d8da2d30b59a53f3e3750323da91d6e3bea470a99975cf5d4438d91b79c1c | 34 |
| Route 280 | 369 | 08154159fc8b6a97bf113308b85e7f93959d818a938d07e55ee025e79a2e1aea | 29 |

For an independent reference, split the two kartcourse sections of CART.ini with SA Tools 786 split.exe binary and the installed executable; hash the decimal lines of each exported course as bytes in order. The local test compares both complete byte sequences to these reference digests. The offline kart geometry query below assembles both tracks. Driving physics, timing and completion remain unverified.

## Rust interface and checks

Campaign::new takes the EXE bytes and the literal contents of ADVERTISE.ini, sa2lvl.ini and ObjectLists.ini. All four revisions are checked. Campaign::levels returns authored level dispatches. Campaign::mount takes a sequence, action ordinal and a caller-supplied resolver for installed resource basenames under resource/gd_PC. Returning None records a missing file; supplying unrelated bytes under a requested SET or landtable name fails. The loader reads normal layers in original placement order and matches LevelGeometry by configured relative path to the specific stage_land symbol, never by picking a nearby stage ID.

MountedStage::resolve(layer, index) returns the original placement and optional definition. MountedStage::collision_world() bakes the explicitly matched terrain on demand; the resulting world supports geometric ray queries. MountedStage::replay_object also runs the verified shared ring and spring callbacks across compatible stages, with caller-supplied observations (see [Shared stage callbacks](SHARED-STAGE-TASKS.md)). The mount does not schedule tasks or implement general player movement, dynamic contacts, mission logic, transitions, boss AI, rendering, or save progression.

Run cargo test --locked --test fan_stage_local -- --ignored from this crate with SA2_GAME_PATH and SA2_PC_CONFIG pointing to the installation and SA Tools PC_SA2 config directory. The installed test checks all authored level dispatches, distinct SETs and resolution counts, exact kart course bytes, start and terrain coverage, all 40 collision-world builds, a City Escape ray, unavailable resources, a wrong SET, and rejection of an event ordinal. Only code, source identities, and observed counts are stored here.

Big Foot�s boss task later places player slot zero at (-90, 0, 0) during its [intro callback](BIG-FOOT.md). The general start-row count above is unchanged.

Both kart mounts also load their character-specific 40-byte story vehicle profile and five nine-byte rank-time rows from the Steam executable. Each row retains the level byte and the four D/C/B/A minute-second pairs; the two unidentified physics floats remain unknown1 and unknown2. Route 101 uses the Tornado car profile (speed cap 10), while Route 280 uses the Bat car profile (speed cap 12). Both have a separate top-speed field of 15. The mounted campaign test compares all named fields, thresholds and raw table hashes against independent SA Tools 786 split.exe binary exports of the two story physics and two rank-time sections in CART.ini. These are tuning and ranking inputs; steering, engine placement rounding, finish detection and rank assignment still require runtime verification.

Run `kart-raycast EXE OUTPUT.json DATA_DLL ROUTE REQUEST.json` with route 101, 280, beginner, standard or expert and a request such as `{"origin":[0,100,-200],"direction":[0,-1,0],"maxDistance":500}`. Coordinates are native units. The query joins the story course descriptor to the DLL's collision pieces, starts at the course origin with zero heading and searches the positioned triangles for the nearest geometric hit. It returns the zero-based course occurrence, piece ID, strip, triangle, original indices, hit point, distance and normal, or null for a miss. Placement uses mathematical sine/cosine. A hit does not establish tire contact, kart suspension, steering or a finish.

`tests/kart_rays_reference.py` takes the CLI, game directory, split course directory, split model directory and a private working directory. It builds independent rays from the SA Tools course lists, exported endpoints/yaw and Basic model triangles. Starting, middle and final occurrences on each of the five courses matched their source triangle and distance within 0.005 native units.

The kart-courses command (EXE OUTPUT.json) reads the five PC_SA2/CART.ini course descriptors. The racing menu courses contain 88 (Beginner), 112 (Standard) and 123 (Expert) piece IDs. They are separate from story dispatches. The Standard and Expert lists include pieces 25 and 26, whose native table entries have displacements but null collision roots. They move subsequent pieces into place and contribute no static triangles. The ray report lists each such occurrence as [zero-based index, piece ID]. Dynamic collision, vehicle controls, lap counting and race completion are unverified.

`kart-vehicles EXE OUTPUT.json` reads all 15 kart physics records mapped in CART.ini. The two story vehicles, six standard karts, placeholder and six alternate/bonus karts retain their table order, source RVA, 40-byte source digest and ten float values. The story mounts use the same decoder. Unknown1 and Unknown2 retain their source labels; these parameters alone do not determine how steering, drift, braking or collision response is integrated each tick. The ChaoKart record is native bonus vehicle data.

To reproduce: split all kartphysics sections of PC_SA2/CART.ini with SA Tools 786, then run `python tests/kart_vehicles_reference.py OUTPUT.json SPLIT_PHYSICS_DIR CART.ini`. The checker compares the name, slot and configured address and reconstructs the original bytes from all named values.

`kart-objects SET OUTPUT.json EXE` resolves the 13-entry native Kart Object List for any of the eight pinned setCart files. The 204 placements across those files include the base, three Mini sets and alternate story sets; they do not all run together. Each object ID indexes its executable callback definition. Position, raw angles and the three parameter floats retain their source values.

The same report includes the two standalone Basic collision meshes mapped by PC_SA2/CART.ini: JUMPDAI (6 vertices) and CHECKGATE (48 vertices). These are in model-local coordinates. GATE and GATE2 have distinct callback addresses; the names do not establish which callback uses CHECKGATE, how it transforms the mesh, or when a checkpoint counts. Ramp launch behavior is likewise unverified. To repeat the comparison, split the two basicmodel sections with SA Tools 786, set SA2_GAME_PATH and SA2_KART_OBJECT_SPLITS (the directory containing both sa1mdl files), then run `cargo test --locked --test kart_objects_local -- --include-ignored`.

`kart-ramp-init CartD.rel OUTPUT.json TASK_POSE.json` reproduces the post-allocation block of the installed BATTLE JUMP PANEL callback. Input supplies the current task position and full signed yaw, for example {"position":[100,0,-400],"yaw":16384}. Module 80's slot 1 name and executable relocations identify section 1 + 0xBC98. After the work allocation succeeds, instructions at 0xBD04–0xBDEC copy a 56-byte node, clear its two low flags, set translation and yaw, zero pitch and roll, issue two external calls and initialize task-work +0x28. The result represents the attachment pointer as a section and offset; it does not expose an emulator address.

Run `python tests/kart_ramp_ppc.py CLI CartD.rel PRIVATE_DIR` with Unicorn installed. The test executes the original instructions for three poses and compares every initialized node word, both call argument lists and the final work field. The calls themselves are not executed. This establishes ramp setup given the task pose, not SET-to-task conversion, engine collision registration, a kart launch, or checkpoint credit.

`kart-ramp-tick CartD.rel OUTPUT.json OBSERVATIONS.json` replays the active JUMP PANEL contact loop (section 1 + 0xBDF4 through 0xBF2C). The timer decreases by one per update and clamps to zero. When contact bit zero is set, the task checks each observed contact, in order, against six non-null registry entries (helper 0x1714). Every match sends the local vector [0, work+0x24, work+0x20] through the yaw transform and invokes the kart response. The timer gates only the later 0x5B90 call; a nonzero return resets it to 30. Multiple contacts can therefore receive a response in the same tick.

The report records transform inputs and subsequent conditional calls, not final kart velocity. Contact generation, the matrix result, the internals of the kart response at 0x5524 and external call effects are outside this replay. The request supplies post-response kart fields and the indexed course byte when required. See `tests/kart_ramp_contact_ppc.py` for 15 runnable inputs and the comparison with the actual contact loop, registry and secondary helper instructions; run with CLI, installed CartD.rel and an ignored temporary directory.

The secondary helper at 0x5B90 returns zero for a cleared work+0x44 field or a signed work+0x48 slot of 8 or above. For mapped slots 0�7 and a nonzero work+0x44 it returns one, resetting the panel timer. Negative slots precede the verified table and are rejected. Helper 0x979C reads a signed course byte: 50�71 inclusive passes 0 to the imported routine at 0x8006C97C; other bytes pass -127. The subsequent call to 0x8006B710 selects its first argument from the native two-column table (ramp column 0), re-reading the slot after the first call. The replay accepts an optional changed slot for this reload, otherwise retaining the observed one. Call effects and the source of the live course index remain external.

`kart-ramp-response CartD.rel OUTPUT.json RESPONSE_INPUT.json` replays helper 0x5524 given the transformed vector received from the panel. When kart work+0x44 is nonzero it clears work+0x30 and +0x68 and copies the three vector words to +0x7C. Its nested yaw update (0x30DC) leaves task yaw untouched when both horizontal components have absolute value below 0.5; otherwise it scales the external angle result by the module's binary64 conversion constant, truncates and wraps the adjusted yaw to 16 bits. With work+0x44 zero, the helper writes a scalar at +0x264 and calls a turn helper using +0x274 and a 0x4000 limit. Null output fields mean no write or call.

The transformed vector and imported magnitude, angle and turn results are explicit observations. This does not integrate subsequent vehicle position. Run `python tests/kart_ramp_response_ppc.py CLI CartD.rel PRIVATE_DIR` with Unicorn to compare both branches and untouched fields against native instructions with those imports supplied.

`kart-gate-tick CartD.rel OUTPUT.json OBSERVATIONS.json` replays the active story GATE2 block (section 1 + 0xB74C–0xB81C) after the entry cull. While the latch is clear, only the first reported contact is tested against the six registered kart tasks. A match sets the latch, adds 60 times the truncated task parameter at +0x24 to the timer word, and increments the primary kart's +0xC4 counter even when a different registered kart made contact. Both words use 32-bit wrapping arithmetic. Further ticks with the latch set skip those writes. The final global flag controls the task byte at +3. The trace records four call sites in order without executing their effects. This does not establish when the engine generates a contact or finishes a race.

Run `python tests/kart_gate_ppc.py CLI CartD.rel PRIVATE_DIR` with Unicorn to compare seven traces against the installed instructions, including repeated contact, a registered kart other than the primary, ignored contacts, fractional parameters and counter overflow.

`kart-race-gate-tick CartD.rel OUTPUT.json OBSERVATIONS.json` replays the active racing-menu GATE update (slot 3, section 1 + 0xB0E4–0xB234). Unlike GATE2, this block does not test a contact. Its helper at 0x9164 reads six signed kart counters at work+0xC4, taking their maximum with an initial value of -1. In the ordinary timed branch a changed maximum restarts the gate timer at 600 and selects state byte 5, even on the tick that the timer expires. The other branches handle the observed phase and override globals. Field and state-byte names remain descriptive; the actual rendered gate, race finish logic and upstream counter changes are separate.

`tests/kart_race_gate_ppc.py CLI CartD.rel PRIVATE_DIR` compares ten active updates, including the original six-kart helper, against Rust. Cases cover phase-change call arguments, signed-byte comparison, override precedence, timer zero versus expiry, and 16-bit counter wrap. The external call is observed only.

`kart-progress-tick CartD.rel OUTPUT.json OBSERVATIONS.json` reproduces one occupied kart's section 1 + 0x885C–0x8D60 update. The native loop tests candidate segments in order. A nonzero work+0x44 permits nine forward candidates and then two reverse candidates; otherwise it tries three forward candidates. The query index uses signed remainder. The stored piece index behaves differently: a one-piece forward crossing resets it to zero, whereas a multi-piece crossing increments the counter and retains the advanced index in this block. For reverse crossings the mode controls whether to clamp to the start or decrement the counter. Work+0xC8 retains the signed maximum of its old value and the resulting +0xC4 counter.

The request supplies helper 0x9220's segment-test results in call order; missing or unused results are rejected. Geometry determining those results, debug draw effects, subsequent ranking and race completion are outside this command. Run `tests/kart_progress_ppc.py CLI CartD.rel PRIVATE_DIR` with Python and Unicorn to compare 24 updates against native instructions, including forward and reverse crossings, missed probes and counter wrap.

`kart-segment-geometry CartD.rel OUTPUT.json OBSERVATIONS.json` reconstructs the first half of the native segment test (section 1 + 0x9220). The request contains task_position, nullable work_position and segment_xz (four native [X,Z] pairs from one 32-byte live record). A missing work pointer or no X/Z displacement returns a definite miss, even if Y changed. Otherwise the routine projects movement onto Y=0 and constructs a triangle: extend the first-to-fourth pair span by three spans at each end at Y=-20, and use the third pair at Y=100. The second pair is not used by this block. This is crossing-test geometry, distinct from road collision triangles.

The moving case returns the inputs to the imported geometry routines. Their final hit result and the creation of the live segment table are not established by this replay. Run `python tests/kart_segment_ppc.py CLI CartD.rel PRIVATE_DIR` with Unicorn to compare early returns and the binary32 triangle, origin and direction against the installed instructions.

`kart-crossing-directory CartD.rel OUTPUT.json EXE` reads the local X/Z quads and
maps each runtime segment index to [piece occurrence, quad within piece]. In
`kart-progress-tick`, the historical fields `course_length` and `piece` mean
runtime **segment count** and **segment index**. Route 101 has 321 authored
pieces and 482 segments; Route 280 has 369 and 537. Beginner, Standard, and
Expert have 123, 160, and 190 segments respectively. The source-local directory does not
supply world coordinates or determine whether a moving kart crossed a segment.

The local oracle `tests/kart_crossing_ppc.py CartD.rel OUTPUT.json Courses/`
uses the independent SA Tools course splits and executes CartD 0x7388–0x7428.
It compares the occurrence writes and every 32-byte source quad presented to
the transform helper. It intercepts that helper; this does not verify its
rotation, translation, intersection tests, or level completion.

The same report includes `projected_segment_xz` in segment order and
`projected_occurrence_poses` in piece order. CartD 0x72a4–0x7368 takes the
previous piece displacement and turn from its 108-byte model row to place
the next occurrence. Rust retains the full accumulated signed yaw, projects
with mathematical sine/cosine, and rounds transformed vectors and accumulated
translations to single precision. These quads can be passed to
`kart-segment-geometry` for offline inspection; they are not observations of
the original engine. The four external math calls starting at 0x93ac remain
unported, including the final hit decision.

Run `python tests/kart_projected_crossings.py OUTPUT.json Data_DLL.dll` with
pefile installed for a second layout calculation using the PC export and
double precision. Across the five installed courses the largest observed
distance between the two projections was 0.01834 game units (Route 280).
This comparison does not establish a tolerance for real gameplay contacts.

`kart-standings CartD.rel OUTPUT.json OCCUPIED_SLOTS.json` computes the active
race ordering after the progress update. Supply an array in native slot order,
ending before the first empty slot (maximum six). Each row contains `segment`,
`counter`, `segment_length` and `distance_to_next`. The last two fields are
results of the imported vector-length calls at 0x8e90 and 0x8e9c: the first
uses the current and next quads' **second** X/Z pairs; the other uses the
kart position and the next quad's second pair. The next index wraps to zero.
This command does not derive distances from the mathematical course projection.

The result contains one single-precision score bit pattern and one place per
occupied slot. Place 1 leads. The native halving-gap sort can reorder equal
scores; zero-length segments can also produce unordered scores. The local
comparison `tests/kart_standings_ppc.py CLI CartD.rel PRIVATE_TMP_DIR`
executes the original scoring, sorting, and place-write instructions for
empty, solo, tied, over-distance, degenerate and signed wraparound inputs.
It checks NaN classification rather than payload. This covers rank assignment,
not the subsequent leader-change effects or race finish.

`kart-field-update CartD.rel OUTPUT.json FIELD.json` continues from standings
through CartD 0x913c. The input has a contiguous occupied slot list with each
slot's standings inputs under `standing`, previous `place`, `work_44`,
`first_ticks`, `sixth_ticks` and `cooldown`. The field also supplies
`race_clock`, `suppress_place_calls`, `restrict_finish`, `finish_latched`
and `post_finish_ticks`. The result contains updated work fields and calls
in native order as [slot, routine, argument or null].

A counter of at least three dispatches the finish calls if the restriction
byte is zero or the slot's work word at +0x44 is nonzero. Processing continues
through the remaining slots, so several can dispatch in one tick. On the next
latched tick this pass only advances the post-finish timer. The preceding
progress update is separate. The output models dispatch, not the internals of
the result tasks or a verified stage clear. The local oracle
`tests/kart_field_ppc.py CLI CartD.rel PRIVATE_TMP_DIR` intercepts the calls
and compares their order, arguments, places, latch and timers against the
installed field loop; it supplies sorted indices for known-score fixtures.

The restricted finish call at 0xd4f8 requests a task whose update is 0xd540.
`kart-finish-menu CartD.rel OUTPUT.json INPUT.json` steps its logic. Supply
`state` (signed byte `mode`, byte `selection`, unsigned 16-bit `timer`,
and word `flag_3ce24`), two native `input_words`, and the observed
`confirm_mask`. Starting at timer zero, mode 0 opens the selection on update
301. In mode 1 direction 0x40 takes priority over 0x80, then confirmation,
then idle timeout. The two confirm branches request distinct native services;
idle timeout requests removal after 3601 idle updates. The output records
calls, not their side effects, and does not label either option as a story
clear. This reader does not render the display callback or run the scheduler.
`tests/kart_finish_menu_ppc.py CLI CartD.rel PRIVATE_TMP_DIR` executes the
original callback, checking state, call order, and arguments on both timer
boundaries, direction conflicts, confirmation, wraparound and inactive modes.

`kart-finish-display CartD.rel OUTPUT.json resultCart.prs INPUT.json` follows
the restricted result task's display callback (0xd734) through its sprite
helper (0xd2dc). Supply signed `mode`, byte `selection`, the first two kart
`places` and `viewport_flag`. The report lists draws in order with the
initialized frame centers, original sizes, UV rectangles, screen positions,
scale, priority and draw flags. Mode zero compares places (ties take the
second branch); mode one draws a heading and two choice frames.

The original archive loaded at 0xd49c contains one 256×256 GVR,
`moji2p_big_kart`. The report hashes the decoded RGBA including original
alpha. The offline oracle `tests/kart_finish_display_ppc.py CLI CartD.rel
resultCart.prs PRIVATE_TMP_DIR` compares the native draw arguments and
frame records for six cases. It stops at the engine sprite renderer; viewport
internals, blending and final framebuffer pixels are not verified.

When a field slot supplies its native kart index (`work_48`), the same field
update resolves each call to CartD 0x5c38 into a `cue_requests` entry.
Finish, first-place and sixth-place calls use columns +8, +12 and +16 of the
24-byte row in section 5 at 0x3924. Negative IDs produce no engine request.
An omitted index stays unresolved, including for a slot with a known place.
The local field oracle runs the original selector for all 16 rows and three
selectors, intercepting the engine at the cue submission. IDs represent
requests, not a verified mapping into a localized audio bank or playback.

`kart-story-target CartD.rel OUTPUT.json INPUT.json` ports the task named
CartLimousineExec, with `operation` set to `spawn` or `tick`. Spawn uses the
current crossing quad's third X/Z pair and the parent task's Y, starts with
2000 units of lead, and adds 100 to Y in mode 1. Active ticks read the parent
work X and float at +0x7c. Lead changes by (7 minus that float), up to 4000.
Exactly 100 does not trigger the latch. On the first tick below 100, modes 0
and 1 request cues 0x701 and 0x708 respectively. That tick still uses the
computed lead; later active ticks follow at a fixed 100-unit X offset.
Y and Z are unchanged by these updates. The latch is shared native state;
spawning does not clear it. This command does not assign a story clear,
advance the parent vehicle, or render the target.

Run `python tests/kart_story_target_ppc.py CLI CartD.rel PRIVATE_TMP_DIR`
to compare spawn and chained updates with the original callback. The test
supplies successful allocation and intercepts the cue service. The installed
event_adx archives did not provide a named directory establishing a mapping
from kart cue IDs to localized clips; matching numeric indices alone is
insufficient to establish playback.

`kart-pursuit-clock CartD.rel OUTPUT.json INPUT.json` handles the controller
prelude at 0x832c after a target update. Pass the target state's `latch` as
`pursuit_latch`; this takes priority over `race_finish_latch`. Each caught
update calls 0x800629e4, increments the signed elapsed word and sets the
controller flags. Only an increment to exactly 300 adds the call to
0x8005eb24(0). Without either finish latch, the remaining countdown is
decremented unless engine byte +9 is nonzero and byte +0x42 is zero.
A task removal request precedes all these branches. The separate race-results
block is reported as a continuation; its writes are not included here.

The native helpers' effects and the shared timing block at 0x861c are outside
this command. In particular, identifying the delayed call does not establish
a saved mission clear. Run `python tests/kart_pursuit_clock_ppc.py CLI CartD.rel
PRIVATE_TMP_DIR` for the instruction comparison, including simultaneous flags,
the two updates surrounding the delayed call, and integer wraparound.

With an optional `race_results` context, `kart-pursuit-clock` also executes
the race-finish branch through 0x85dc. The result lists proposed writes
relative to the original in-memory record base; no save is opened or edited.
At elapsed update 60 the first slot must exist, place first, and pass the
single-player check. Mode 5 compares signed minute/second/centisecond bytes
and replaces only a strictly slower stored time when the engine permits it.
Other modes >=2 mark their indexed completion byte and submit the time.
The service return supplied in `check_return` is needed only on a write path.
An active result menu holds update 200 at 199. At update 300 the branch sets
the result flag and requests engine action 8 unless engine byte +0x23 is 1.
This is distinct from the pursuit catch path. Record persistence and subsequent
engine transitions are unverified. Run `tests/kart_record_results_ppc.py CLI
CartD.rel PRIVATE_TMP_DIR` to compare against instructions with scratch memory.

When `kart-pursuit-clock` reaches shared timing it accepts an optional
`lap_clock`: its `clock` contains a tick word, the current three signed
display bytes and the two three-entry lap arrays (`first`, `second`).
Supply `inhibit`, `cumulative`, `first_counter`, `first_highest` and optional
`second_highest`. A finish latch or inhibit word freezes the clock. Otherwise
it advances one tick and truncates to centiseconds at 60 ticks per second.
The first kart can record cumulative time or individual splits; the second
records cumulative times only on the split path. The returned display bytes
are suitable as time inputs on a later record update. The native split helper
round-trips the first subtraction through three signed bytes on lap three.
Negative indices into the lap arrays are unsupported.

The countdown at state +0x0c is separate from the elapsed ticks at +8.
Consumers test the countdown at 0x2704 (vehicle state), 0x98d4 (timeout
branch) and 0xdaac (pursuit target). This pass does not perform those tasks.
Run `tests/kart_lap_clock_ppc.py CLI CartD.rel PRIVATE_TMP_DIR` for the
installed instruction comparison.

`kart-timeout CartD.rel OUTPUT.json INPUT.json` follows the controller's
remaining countdown into the vehicle at 0x26fc. Operation `vehicle_countdown`
uses `remaining`, `vehicle_state` and `mode`. When remaining is nonpositive
and the vehicle is not already in state 10, it enters 10 and creates the
time-over task. State 10 suppresses a duplicate spawn on this path.
Operation `spawn` also supports the constructor's other caller: positive
remaining starts the timer at 120 without the expiry cue. With zero or
negative remaining, it starts at zero and requests cue 0x706 or 0x70d in
modes 0 or 1. Operation `tick` takes `timer` and `engine_halfword_64`.
After increment, values strictly above 180 request engine action 8 when
that signed halfword is below one, otherwise action 4. The task itself
does not consume the request or prevent it from recurring next update.

The comparison `tests/kart_timeout_ppc.py CLI CartD.rel PRIVATE_TMP_DIR`
runs the vehicle branch, constructor and task updates against the installed
PowerPC code with a successful allocation. It checks cue and action IDs;
the action service's effects and the scheduler are not verified.

`kart-drive-input CartD.rel OUTPUT.json INPUT.json` replays observations
from one kart's selected controller row. Supply its 60-word `history`,
write `cursor` and ordered `frames`. Each frame contains `buttons` (word
at +8), `stick_x` (signed halfword at +0x1c) and `work_28c`. The return
includes the final ring and cursor so another batch can continue it.

CartD 0x5360 records the stick and advances the cursor. The steering path
at 0x3220 sums ten entries starting at the **new write cursor**, stepping
backward, then multiplies by the source float at section 4 +0x200. This
differs from averaging the ten most recent samples. The 0x3034 helper tests
mask 4 for work +0x110 and either bit of 0x402 for +0x114; a signed +0x28c
above ten suppresses the latter. These are selected controller records and
intermediate vehicle inputs, not physical controller bindings or integrated
kart steering. Run `tests/kart_drive_input_ppc.py CLI CartD.rel PRIVATE_DIR`
to compare 134 frames through the original helpers in both slots and banks.

Each frame can also include a `drive` context for the driving routine at
0x3188. Supply the current heading, work word +0xa4, race place, multiplayer
flag, X/Z velocity, and the four active physics values (acceleration, brake
power, deceleration and speed cap). For nonzero X/Z velocity, optionally supply the
observed `magnitude_return` from 0x80114eac. Zero velocity bypasses that call.
The same computed steering and button flags drive the heading subtraction
and the local displacement: acceleration takes precedence over braking,
and multiplayer place scales acceleration and braking. The returned
`motion` contains the heading, updated work words and local vector before
CartD transforms it at 0x33a4. These are intermediate tick results; imported
matrix rounding, collision and final world position are not verified.
The input oracle also compares these heading and displacement writes for
all 134 samples, including both button flags held together.

With a `drive.position_step`, the report also computes the task position
at 0x3428. Supply `position`, `velocity_y`, `weight` and `work_74`.
An optional `transformed_vector` is the observed result of the matrix helper.
Without it, Rust projects the local Z displacement using the updated heading;
the private three-angle record at section 5 +0x3bc0 starts at zero and this
path writes only Y. If the magnitude result is omitted for a moving kart,
Rust also projects planar length mathematically. The report distinguishes
both projections from observed engine results. Engine trigonometric and
magnitude rounding have not been verified.

The controller adds the world displacement to position first. Then it adds
vertical velocity minus weight to Y, with quarter weight only for positive
vertical velocity and nonzero work +0x74. The instruction comparison now
checks the position bytes after this arithmetic for 134 cases with supplied
or projected vectors. Matrix and vector-add imports are represented at the
call boundary; these positions precede the remaining vehicle and collision
updates and are not a verified trajectory.

Optional `drive.drift` follows the normal driving routine through its drift
entry decision. Supply the 30 motion-history samples (newest first),
the active profile's drift speed threshold, vehicle state, +0x10c flags and
+0xa8. Rust computes the offset of the newest sample from the 30-sample
mean. When the rotated `local_offset` is observed it also reports the native
+0xf0 and +0xf8 writes; absent that observation those fields remain null.
The vector-add/subtract engine imports are represented arithmetically.

Speed must exceed the profile threshold and the updated +0xa4 must be
positive before flag bit 0 is set. With acceleration held and absolute raw
stick above 20, the vehicle enters state 2, saves the signed stick at +0xa8
and clears +0xa4. Exactly 20 does not start a drift. The nested drift output
contains the final +0xa4; the outer value is the earlier pedal update. This
does not implement the subsequent state-2 driving routine. The input oracle
also executes the position-ring reads, native split thresholds and state writes.

`kart-drift-windup CartD.rel OUTPUT.json INPUT.json` runs the state-2
callback selected by the dispatch table at section 5 +0x3b94. The five
windup updates read their angular increments directly from the installed
table. Direction uses the signed stick saved at entry (+0xa8), not the
current stick. The fifth update resets +0xa4 and selects state 3. The
shared 0x399c routine rotates planar velocity, scales its local Z by
drift handling, normalizes it and restores speed (with a reduction when
acceleration is released), then advances position. The report labels its
math as projected: engine atan2, vector rotation, length, normalization
and vector addition are modeled. Later callbacks can change position.

`tests/kart_drift_windup_ppc.py CLI CartD.rel PRIVATE_DIR` runs the
original state-2 callback for both five-tick directions and compares each
state, heading, local velocity and position before later collision updates.
The test uses modeled engine imports and supplied velocities, not a game
trajectory or evidence of exact engine trigonometric rounding.

`kart-drift-step CartD.rel OUTPUT.json INPUT.json` also handles state 3:
add a `sustained` object with the current signed `stick_x`, profile
`speed_threshold`, `work_fc`, and (when +0x74 is nonzero) the observed
`angle_service_return`. Without this object it runs state 2. In state 3
the new heading is compensated in +0x68 before applying the current stick.
The shared drift velocity routine then updates local velocity and projected
position. Local sideways speed below 1 increments the recovery counter;
otherwise it resets. A counter reaching 20, zero velocity, or full 3D speed
below the native fraction of the threshold selects normal state 0. Otherwise
the next state is 3. These outputs precede separate vehicle collision tasks.
The PPC comparison exercises 14 state-3 updates in addition to both windups,
with engine math modeled and the angle service return supplied.

`kart-contact-step CartD.rel OUTPUT.json INPUT.json` builds the movement
query issued at 0x2d0c after normal or drift motion. It takes the previous
work position and proposed task position, current velocity, work words
+0x28c and +0x44, and player slot. Without a supplied `result`, it returns
the query only. With the engine's signed `return_word` and
`resolved_position`, the next callback copies that position even for a
zero return, and recomputes velocity from resolved minus previous position.
The full return word becomes +0x74. Nonzero contact resets +0x28c; a miss
increments it. First contact after more than 30 missed updates requests the
original effect and, when +0x44 is nonzero, cue and controller services.

The imported resolver 0x800c1488, later wall response and effect playback
are not implemented by this command. Static course raycasts do not substitute
for a movement query. `tests/kart_contact_ppc.py CLI CartD.rel PRIVATE_DIR`
compares the request bytes, branch choice, outgoing effect arguments and
state writes against the original callbacks with supplied resolver results.

Optional `wall` observations extend the movement response through 0x2fa0:
supply contact normal, existing impulse, +0x70, cooldown (+0x290), mode,
vehicle state and engine byte +0x23. For nonnegative +0x70, a nonzero
contact with absolute normal Y below 0.2 restores the query's start position
(optional `result.returned_from` if the engine changed it), zeroes vertical
velocity and replaces horizontal impulse. The callback then adds all three
impulse components and damps only X/Z. Wall feedback, gated route cue,
timeout dispatch and the 180-update cooldown write are reported in order.
`align_to_normal` indicates the engine angle-service path; its rotation
outputs are not calculated. Terrain resolution remains external.

The subsequent history writer at 0x2fc4 passes work velocity (+0x7c), not
world position, to 0x5220. For driving history use `motion_history` in
newest-first order; `positions` is accepted for compatibility. Feeding
world-space positions to that calculation would misrepresent this caller.

`kart-scripted-motion CartD.rel OUTPUT.json INPUT.json` advances the movement
callback for vehicle state 8 (0x422c) or 9 (0x426c). Carry forward position,
angles, work_68, work_dc, vehicle_state and both module globals (global_21760,
global_22494). The globals are shared between tasks. State 8 shifts X and fixes
all three angles. State 9 shifts Z and increments the shared counter; an old
signed value above 120 clears global_22494 and returns the vehicle to state 0.
Neither callback selects a route. Both dispatch paths still run 0x5774 and the
contact pass after movement. The command covers only movement before those calls.

Supply optional `audio: {work_44, profile}` to also resolve the following
CartD 0x5774 sound dispatch. The updated state selects the path: a disabled
work_44 exits immediately; states 8/9 submit the profile cue, task-relative
handle and updated position to engine 0x8006b188. An enabled kart leaving
state 9 for 0 instead reaches the normal sound path at 0x5820, reported as
unresolved. This describes engine requests, not playback. The horizontal
speed computed on entry does not affect these scripted cue arguments.

After a contact with history enabled, pass the same returned
`response.tail.ring` to the next contact as `tail.ring` and to the next
normal drive step as `drift.ring`. The drive read uses the newest 30 raw
velocities without advancing the cursor. For standalone captures, supply
`drift.motion_history` instead. Each ring belongs to one kart history slot.

`kart-timeout` operation `vehicle_post_move` covers the shared update at
0x2648 through entry to 0x2814. Supply the wall response cooldown, remaining
clock, vehicle state, mode and frame (see the local PPC comparison for its
field layout). A contact that writes 180 carries 179 past this update. Only
modes <=1 enter the countdown and route boundary checks. With time remaining
the boundary is strictly past segment_count minus 20: engine byte +0x23 equal
to 1 resets the kart pose and segment; otherwise the kart shifts X and goes
back to segment_count minus 25. The former requires the observed magnitude
return and the latter requires the 0xd968 task return if no handle exists.
Neither path proves a completed stage or runs the following vehicle routines.

At the non-reset route boundary, `frame.limousine_allocation` can now replace
the opaque `d968_return`. Supply a nonzero allocated task handle, the new
segment index (count minus 25), its four X/Z points and the current pursuit
latch. The constructor uses point 2 and the parent height. The returned
`limousine.target.state` feeds `kart-story-target` tick directly. The parent
stores the same handle in work_ec; a nonzero existing handle skips creation.
Allocation and scheduling remain external; the constructor records the
original exec, display and delete callbacks as offsets in CartD text.

`kart-target-display CartD.rel OUTPUT.json INPUT.json` resolves the pursuit
draw submission. Pass operation `display`, the target state's position, mode,
and the current module-wide shared_frame. The zero mode uses the static
object; other modes submit a motion at the old frame and advance the shared
frame after the draw. Keep that clock across tasks and displays. Texture names
come from native registration and object, texture-list and motion offsets
refer to CartD section 5. The command describes submissions; it does not
render the objects. Operation `delete` reports the callback's two null writes
on the child. The original does not clear the parent's vehicle handle here.

`kart-target-textures CartD.rel OUTPUT.json MODE` follows the native texture
registration to the sibling source archive, validates its SHA-256 and texture
count, and reports each decoded entry's global index, dimensions, RGBA hash
and nonopaque pixel count. The output does not contain texture pixels.
Independent ArchiveTool PNG conversions matched all 9 cartLim and 21 limtails
entries byte for byte after RGBA decode (17,573 nonopaque pixels total).
This verifies texture content, not UV assignment or rendered appearance.

`kart-target-model CartD.rel OUTPUT.json MODE` reads the original object
hierarchy selected by the draw call. Nodes are in child-first order; each
parent is an index into that list. Positions, signed game angles, scales
and evaluation flags are local to the node. Attachments remain section-5
references. Mode 0 selects the president's limousine (11 nodes); nonzero
mode selects the Tornado (53 nodes). All 64 nodes match their corresponding
PC Data_DLL exports for parentage, transform bits, flags and attachment
presence. This comparison does not establish mesh or skinning parity.

The animated pursuit report also follows the motion pointer passed to the draw
call. Each Tornado node carries its original position, angle and scale keys
with frame indices. The 10-frame motion has 159 channels and 326 keys; every
key word matches the corresponding PC motion export, including signed game
angles and signed zero in float values. The limousine has no submitted motion.
These are authored keys; fractional-frame interpolation and skinned draw output
are not verified by the key comparison.

Each pursuit attachment now includes its authored bounding sphere and type-41
vertex chunks, in source order, with original index offsets, flags, positions
and normals. No coordinate or unit conversion is applied. The independent PC
export comparison matched every position and normal word: 1,458 vertices on
the limousine and 1,183 on the Tornado. These are vertex streams; polygon
connectivity, material selection and UVs are not implied by this comparison.

Pursuit geometry also reports the ordered polygon commands, including native
material and texture words, diffuse RGBA when specified, and strip flags.
Each strip records its original winding, per-corner vertex index, signed UV
words and triangle indices into that strip. The independent PC export comparison
matches all polygon command words, 6,351 corners and 4,383 nondegenerate
triangles across both models. Caller-side rendering and material state across
draws are still outside this report.

Each strip command now includes the most recent authored material values in
model traversal order: diffuse and ambient RGBA, specular RGB/exponent, blend
flags and texture selection words. Unset fields remain null; the report does
not invent engine defaults. Resolve a UV strip's textureId against the index
in the named kart-target-textures bank. A retained texture ID on an untextured
strip does not imply texture sampling. All 121 strip material snapshots match
a separate walk of the PC exports; this does not verify final compositing.

Optional `frame.dispatch` continues vehicle_post_move from 0x2814 to the
last engine call (0x8000f4ec). The updated global_24d60 takes precedence
over the pursuit latch. A set latch alone selects state 8; a set global
with work_44 enabled enters state 7, clears control and work_68, selects
point 1 on the next two wrapped segments and initializes speed and angle.
Supply the quad for segment +2 and the observed engine angle return, plus
the magnitude return for nonzero velocity. This covers entry into state 7,
not subsequent state 7 steering or the final engine callback.

`kart-guided-drive CartD.rel OUTPUT.json INPUT.json` advances state 7 from
0x42c8 through return. Carry the state-7 entry guidance (or previous tick
guidance), the addressed segment quad, vehicle state fields and active
profile acceleration, deceleration and weight. Engine angle, direction,
distance and random results are explicit inputs. The distance check uses
the pre-movement horizontal offset; reaching the target segment skips this
check. Only an advance consumes a random result. The output includes the
updated guidance pair, pose, cached ground position and yaw. This is the
movement callback before contact resolution, not a whole vehicle tick.
