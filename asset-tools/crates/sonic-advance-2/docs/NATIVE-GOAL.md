# Native ordinary-course goals and results

`native_goal.rs` ports the original A2NE goal toggle, goal lever task, Sonic
ordinary-act goal callbacks, result bonuses/counting and campaign progression
decisions. It consumes verified ROM records and native Rust animation, player
and profile types. These are original-game decisions, not MMO rewards or
canonical service writes.

Evidence is pinned to SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. The two parity catalogs record exact
Git blob identities and local output SHA-256 fingerprints. Original reference
C, generated C/executables, vectors and the ROM remain under ignored `.local/`.

## Goal tasks

The actual finish trigger is interactable **46**. Interactable **42** is the
decorative lever, animation 532, initial variant 1, activated variant 0.
The scanner admits both with original coordinates/data. The lever uses actual
animation Hitbox 0 for both actors, OAM order 4 and BG priority 1. Selecting
its active variant displays the old frame until the next native update.

Once spawned, a single-player toggle tests only whether the player crossed
its X coordinate and has not already reached the goal. There is no additional
Y or alive-state check. It requests transition 10, stores goal X, and ORs stage
flags 0x21. Grounded campaign crossings award native speed bonuses at Q8
thresholds 640, 1024, 2304 and 2560. Toggle crossing precedes camera despawn;
lever despawn precedes collision. Their original unsigned camera bounds remain.

`StageGoals::tick` leaves newly admitted task dispatch eligibility an explicit
host input, which must follow the native task-list ordering. Its task budget
is 128. Multiplayer finish synchronization remains an explicit error.

## Callback and service boundaries

`GoalController` represents normal-gravity Sonic campaign callbacks for
ordinary acts: cancellation, slowdown, three brake variants, distance bonus,
victory and camera shift. The host supplies current native animation and
executes every `advance_ground` request with native clamp, speed conversion,
position, terrain follow and control lock, without ordinary input acceleration.
This module does not execute those services itself.

The brake callback exposes `check_braking_dust`; `braking_dust_position` emits
the original normal-gravity position every eighth stage frame with the goal
flag (every fourth without it). The host must use the original effect asset.
Audio start/stop requests are explicit. Special-stage transition, other
characters, bosses, time-attack-specific player callbacks and inverted gravity
remain outside this controller. `WaitingForGround` represents deferred host
callback ownership; it is not evidence of a complete native callback graph.

`StageResults` preserves the signed global-ring cast into persistent score,
time thresholds, exactly-seven-special-ring bonus, concurrent 100-point
countdown, A-button skip, sound cadence and original life-score arithmetic.
It counts before accepting completion and emits a save request exactly once.
Ordinary acts select next act or special stage; bosses select course select or
the original Sonic character-unlock cutscene. Final stage 28 unlocks 29;
extra stage **29** advances to 30. Final/extra disable A skipping and wait for
the supplied native BGM completion result.

Fade and BGM completion are explicit service inputs. Mock readiness is not
fade/audio integration. The native fade service is separately implemented with
results presentation. Scene creation and save requests remain host services;
in-memory profiles/flash are not durable storage or a canonical MMO round trip.

## Executed verification

`native_goal_oracle.py` compiles unmodified exact-source function bodies and
native constants/score macros with named-field graphics, task, audio, fade and
scene service stubs. It proves decisions through those boundaries, not native
struct ABI or hardware behavior. `source_native_goal` compares:

- 4,096 crossing/score states, including dead, airborne and off-camera cases;
- 512 constructor states including time-bonus thresholds and signed rings;
- 64 complete 1,300-frame sequences (83,200 states), comparing every bonus,
  score/life/profile unlock, level, scene destination, save request and sound;
- all 122 goal/lever records in fourteen actual-ROM ordinary-act streams,
  independently decoded, plus spawned native toggle checks on those records.

`native_goal_player_oracle.py` executes original cancellation, offset, init,
slowdown, brake, victory and camera-shift bodies. Another 16,384 cases compare
fields, callback phase, score, audio/effect requests and five ground-service
calls. Physics/collision are stubbed and counted, not claimed as route parity.

Run normal module units, then the local source comparisons:

```powershell
$env:A2NE_ROM = '<local verified A2NE ROM>'
$env:A2NE_GOAL_ORACLE = '<local native-goal-oracle directory>'
$env:A2NE_GOAL_PLAYER_ORACLE = '<local native-goal-player-oracle/results.bin>'
cargo test --test source_native_goal -- --ignored
```

These comparisons establish only the listed slices. Full task scheduling, all
campaign routes, hardware timing, audible playback, scene implementations and
user playthrough acceptance remain separate work.
