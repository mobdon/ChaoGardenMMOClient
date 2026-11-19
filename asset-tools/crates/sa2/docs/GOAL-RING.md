# Story goal ring contact

The mounted Rust goal-ring command follows the original placement and level's executable module. In the verified Steam build, 55 distinct normal story SET rows across 28 distinct levels resolve to GOALRING at PC address 0x6C63C0. Each of the corresponding 28 stage modules also has a relocated native object list entry and the same PowerPC contact branch (the local call displacement varies by module). The reader verifies the source SHA, object name, function pointer, branch hash, seven imported call targets, and the collision descriptors. The two centered spheres have radii 25 and 8 in game units.

    sa2-assets --source-root GAME fan-goal GAME/sonic2app.exe .local/goal.json PC_SA2 "Hero Story Sequence" 13 0 198 REQUEST.json

The example selects Green Forest's large goal. Row 349 is a different goal variant; with game byte 0 it takes the child-task path. Example request for a fresh, in-range task whose work allocation succeeded:

    {"game_byte_23":0,"range_out":false,"allocation_ok":true,"frames":[{"kind":"none"},{"kind":"player","slot":0},{"kind":"none"}]}

The fresh-init branch uses the dispatched character, signed SET angle, observed global byte and optional slot_zero_timer. It distinguishes deferred initialization, child-task selection, range rejection and allocation failure. The parent collision executor consumes an observed collision query result (none, unidentified task or player). It emits the native call order, chooses 0x1013 or 0x1014, latches the contact, resets the task scale Y, and avoids a second activation. Imported main-game calls are identified by address; emitting a call does not execute its implementation. The optional probe with center and radius reports placement-space sphere separation; it is not accepted as a retail hit or completion.

The optional test runs the installed City Escape REL contact instructions, with deterministic external callees, at section 1 offsets 0x2DA4C..0x2DB48, and the init decision starting at 0x2D24C. All 720 fresh-init cases and 280 contact cases agreed with Rust. The comparator does not execute the effects, child task, player collision engine, mission controller or story transition. The module reader was independently exercised on all 28 originals; the CLI test verifies a normal goal, child variant, unrelated placement rejection and local manifest. Set SA2_SET_DIR, SA2_GAME_PATH, SA2_PC_CONFIG and use a Python with Unicorn PPC support:

    cargo test --locked --test goal_ring -- --include-ignored
