# Story goal ring contact

The mounted Rust goal-ring command follows the original placement and level's executable module. In the verified Steam build, 55 distinct normal story SET rows across 28 distinct levels resolve to GOALRING at PC address 0x6C63C0. Each of the corresponding 28 stage modules also has a relocated native object list entry and the same PowerPC contact branch (the local call displacement varies by module). The reader verifies the source SHA, object name, function pointer, branch hash, seven imported call targets, and the collision descriptors. The two centered spheres have radii 25 and 8 in game units.

    sa2-assets --source-root GAME fan-goal GAME/sonic2app.exe .local/goal.json PC_SA2 "Hero Story Sequence" 13 0 198 REQUEST.json

The example selects Green Forest's large goal. Row 349 is a different goal variant; with game byte 0 it takes the child-task path. Example request for a fresh, in-range task whose work allocation succeeded:

    {"game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":[{"kind":"none"},{"kind":"player","slot":0},{"kind":"none"}]}

The fresh-init branch uses the dispatched character, signed SET angle, observed global byte and optional slot_zero_timer. It distinguishes deferred initialization, child-task selection, range rejection and allocation failure. The parent collision executor consumes an observed collision query result (none, unidentified task or player). It emits the native call order, chooses 0x1013 or 0x1014, latches the contact, resets the task scale Y, and avoids a second activation. Imported main-game calls are identified by address; emitting a call does not execute its implementation. The optional probe with center and radius reports placement-space sphere separation; it is not accepted as a retail hit or completion.

The optional test runs the installed City Escape REL contact instructions, with deterministic external callees, at section 1 offsets 0x2DA4C..0x2DB48, and the init decision starting at 0x2D24C. All 720 fresh-init cases and 280 contact cases agreed with Rust. The comparator does not execute the effects, child task, player collision engine, mission controller or story transition. The module reader was independently exercised on all 28 originals; the CLI test verifies a normal goal, child variant, unrelated placement rejection and local manifest. Set SA2_SET_DIR, SA2_GAME_PATH, SA2_PC_CONFIG and use a Python with Unicorn PPC support:

    cargo test --locked --test goal_ring -- --include-ignored

For regular goals (effective X angle divisible by three), add `"lifecycle":{"initial_timer":0,"game_byte_37":0,"task_field_30_present":false}` to the request. The timer and context fields are supplied observations. The replay updates yaw and height before testing contact. With a zero initial timer and game byte 0, first contact leaves the timer at zero; the sixtieth subsequent tick requests cleanup. A nonzero game byte at offset 0x37 skips motion and timer advancement. Replay stops at the first cleanup call; the caller's task scheduler and the called function are outside this replay.

The reader checks the motion and cleanup instruction blocks across all 28 supported modules, reads the spin and rise constants through their relocations and verifies both cleanup call targets. Forty additional comparisons execute the installed City Escape blocks (0x2D674 through the regular motion update and 0x2DCD4 through cleanup), including yaw and timer rollover, gated updates and both cleanup routes. Random effects between those blocks are not executed.

On the contact route with sound 0x1013, the stage-local helper stores 2
in the contacting player's module word. The other player's word is
unchanged. The route with sound 0x1014 bypasses the helper. The reader
checks the helper instructions and both relocations in each supported
module. Each frame reports `moduleSlotWords`; a fresh replay starts from
the two original section-5 words, or from `initial_slot_words` supplied by
a caller replaying an already active stage. This state belongs to the
stage module. Its write alone does not establish a mission result.

To replay the separate stage task once it reaches the word-2 branch, add
`"post_contact_consumer":{"slot":1,"query":13}`. The query is the observed
return from engine routine 0x8001dcf4. Rust reads the installed module's
93-entry jump table. Depending on its entry, the task can pass a code and
its work pointer + 0x3c to a local helper. It then clears the selected slot,
writes 120 to work offset 0x34 and resets the mode byte. Out-of-range query
values skip the helper but still perform these writes. This replay ends at
that reset. Scheduling the task, executing its nested helper and deciding
the mission result remain open. Neither the goal write nor this read is
evidence that the mission controller advanced.

The optional `post_contact_consumer.message` expands that nested call when
the switch selects a code. The installed helper compares the code to the
signed message count. An in-range code selects an indexed pointer; otherwise
the stage passes its section-5 fallback message. Supply the observed
`count`, selected `handle`, `work_address` and `section5_address`.
Only the indexed route takes `indexed: {"pointer": ..., "game_byte_17": ...}`.
The report records the engine address and register arguments. In City Escape
the fallback string identifies an out-of-range message setting. No message
text is exported. Message presentation, the separate main-engine call on
goal contact, and mission completion remain outside this replay.

For a placement that selects `ChildTask`, the optional `child` input supplies
`existing`, `allocation` (null if no allocation runs), and `linked_after`.
The parent only requests a child if none is attached. On success it installs
its own callback and initializes two fields on the returned child's work.
It then reads the parent link again and copies the placement position to the
linked child if present. An allocator return and the re-read link are distinct
observations. The mounted replay reports these writes without running the
child's callback or simulating a level clear.

When the linked child is observed running, add `child_dispatch` with its
signed `mode` and `byte1`. Supply a boolean `range_out` only when byte 1 is
zero. An out-of-range child returns before callback selection. Otherwise the
report gives writes to task offsets 20 and 32, the selected nested task call,
and whether the dispatcher resets the mode after that call. A null callback
write pair means both fields are preserved. The native dispatcher in stages
16 and 18 has different local call displacements; handlers are resolved
from the installed module. The child is not scheduled automatically. For mode 3 the replay follows the
local handler through both engine requests, passing the module's section-5
descriptor offset, 2 and 4 in the second request, then writing mode 4.
For mode 6 it follows the engine call and the clear at task offset 60 before
the dispatcher resets the mode to zero. Other local handlers and imported
engine routines are not executed.

For an active mode-1 child, `child_dispatch.mode_one` accepts the initial
`work36`, `work40` and an array `game_bytes_55` with one observed game
byte for each scheduled handler invocation. The module supplies the float
constants through its relocations. With byte zero, the handler increments
both fields and clamps work40 when it reaches the transition threshold.
A nonzero byte forces the transition and leaves work36 untouched. The
report retains both binary32 bit patterns per invocation and stops at mode
2, without executing that next handler. Eighteen sequences compare the
installed instructions for stages 13, 16 and 18.

For an active mode-2 child, `child_dispatch.contact` takes `flags` (work
halfword at +4) and `cached` (signed byte at secondary work +0x44). Supply
`primary` and `secondary` hit booleans only for queries that execute;
`identified` is the signed engine result when a hit is identified. A miss
on both queries does not reuse a cached player. When neither a hit nor a
usable cached index resolves a player, supply `idle` with `work36` and
`game_byte_55`. The report then follows registration and the gated work
increment through the native return. A resolved player instead stops at
the player lookup. Its index is not evidence of an award or mission clear.

To follow a resolved player through the mode-2 return, supply
`contact.activation`. It takes the observed player flags, position and two
action indices (before and after the first local call). Bit 0x4000 skips
both table lookups and requires absent indices. The report records calls to
the selected module routines, effect parameters and subsequent engine calls.
Supply `linked_task`, signed `byte1`, and `parent_linked` only for the parent
lookup (no task link and byte1 nonzero). The `tail` observation supplies
work36 and game byte 55 at final registration. On accepted contact the task
switches to mode 3. Nested calls are observed at their boundaries; this
does not establish their gameplay effects or a mission result.

For action indices 1, 3, 4 and 7 the trace also expands the selected
callback's nested engine calls in order. The stage supplies their integer
arguments and the exact single-precision float passed in register f1
(`f1Bits`). No resulting change to player stats, score or inventory is
inferred from these requests. The comparison follows the original callback
inside the contact handler for both player slots.


All eleven indirect action routines now run to their service boundaries.
Rows 0, 5, 8 and 10 submit a common request followed by a row-specific
player service. Row 6 passes the child's work position to a local helper;
row 9 returns immediately. Row 2 first requests its player service, then
can submit a parent or task request. For row 2, supply `action_links` on
`activation`: `byte1`, `parent_link` only if that byte is nonzero, and
`task_link` only if no parent link was selected. These are observations
inside the action; the later activation link check is independent. This
replay does not execute the shared engine services.

For row 6, optional `activation.position_list` supplies the raw manager
pointer, the observed task allocation return (`created`) if the manager
was absent, and the node allocation return (`node`) if a manager exists.
Zero means allocation failed. Only a successful node allocation reads
`hook` and `old_head`. The helper initializes a newly created manager's
two callbacks, requests 24 bytes, conditionally installs the global hook,
and prepends a record containing the three original position words, two
stage constants and the previous head pointer. Code and label pointers in
the report are module section offsets. The comparison runs the original
helper inside action 6 with both player slots. Engine task scheduling remains external.


Supply `position_list.schedule` to follow the manager after insertion:
`previous` contains the existing nodes in head-to-tail order (address,
value, limit), and `updates` is the number of scheduled callbacks. The
inserted node is added automatically. Each callback advances the value by
the installed module constant. A node exactly at its limit survives; one
above it is clamped, unlinked and freed. Frames record releases, survivors
and the first empty-list removal request. To observe teardown separately,
supply `cleanup: [owns_manager_slot, owns_global_hook]`. Teardown releases
remaining nodes in order and clears only the slots it still owns. A
removal request alone does not prove the scheduler executed teardown.

To invoke the stage range callback on the surviving list after each
scheduled update, supply a world-space XYZ point in `schedule.query` and
`position` on each previous node. The newly inserted node uses the actual
action position. The callback follows list order, returning true on the
first point-to-point distance strictly below the node's current radius.
Equality is a miss; an empty list returns false. The report records visited
addresses and distance bits. This explicit callback replay does not imply
the global engine slot currently points to it. It uses the shared Rust
point-distance calculation used for board and path geometry. The installed
comparison executes the original range callback with independently computed
distances; exact retail library square-root rounding remains unverified.

Supply `schedule.display_word` to call the stage display routine on each
post-update list with that observed engine word (offset 0x7c). For each
surviving node it sets up the transform, translates to the node, rotates by
the low 32 bits of the word times 0x120 and calls the stage draw helper with
the current radius and radius divided by limit. The trace preserves float
argument bits and the order of the transform calls, including the final
pop. Empty lists emit no calls. Previous nodes need their XYZ positions.
The trace is limited to 8192 displayed records across the supplied updates.
The trace also expands the stage draw helper using the model and texture
list from that module. It records the temporary model field at offset 20,
the scale derived from its original value, both original packed color
arguments, and the ratio passed to the engine. The selected color is
forwarded to the model submission routine as `previousReturn`; the engine
color computation is not replayed. The helper then pops its transform and
restores the model field. The comparison uses the module's initial model
state. The submission wrapper passes the original section-5 source pointer, a
scratch address, the color service return and 65535 to its preparation
routine. It temporarily replaces the model's first pointer with the scratch
address for the engine draw call, then restores the original pointer.
The City Escape comparator runs these writes and observes the pointer at the
draw call and after return. To replay buffer preparation, supply `schedule.display_colors`: one observed
packed color-service return per surviving node per displayed update, in
submission order. Rust reads the module's original XYZ words without float
conversion and writes its packed color word unchanged after each vertex.
The replay checks the source terminator and reports the byte length and
SHA-256 of the prepared stream without exporting its contents. City Escape's
32 vertices yield 524 bytes. The comparator executes the installed preparation
routine, checks its return and the guard words around its output, and compares
the full stream digest. Color computation, renderer and pixels remain unverified.
