# Damage and dropped rings

The Rust core follows DamegeRingScatter and Tobitiri in GSNE8P src/OBJECT/o_ring.c at revision 874a480379260dce01e8988d6ab13c0deddcee4c. The native damage path requests a debit of the player's whole balance and at most 20 spawned ring tasks. Zero rings requests death for a normal character; the two mech base characters return without that request. The host executes calls and initializes only successful task allocations. This code does not own the player's health or wallet.

DroppedRing models the moving, resting, collected and expired task phases. Flight computes gravity, velocity limits, damping, the collision-plane test and bounce, and the delay before a ring registers for player contact. The host supplies the original gravity, trigonometry, selected shadow-query result and any CCL contact. Render work (including flicker, sounds and particle effects on screen), player damage detection, and live scheduling are separate.

For a local stage replay:

    sa2-assets damage-ring GD_PC/stg13D.rel .local/ring.json objLandTable0013 REQUEST.json

An example request, using a test launch five native units above a known City Escape floor:

    {"slot":0,"base_character":0,"rings":34,"position":[4846.6666667,-10025,5081.6666667],"scatter_random":0.75,"ring_index":0,"spin_random":0.75,"frames":422,"probe_depth":150}

This adapter follows a single successfully allocated ring under downward gravity. It uses the baked static geometry and returns the triangle identity for each ray hit; it rejects sloped hits rather than guessing native floor angles. The probe clock starts fresh and assumes preceding rings in the scatter initialized in index order. The player position, random inputs and geometric ray are test inputs; this is not a captured retail trajectory. On the pinned City Escape module the run produced 27 static hits and 166 bounces, entered rest on update 241 and destroyed the expired task on update 422. No player contact was supplied. Original modules and reports stay local.

Validation: tests/damage_ring_trace.rs compiles the complete pinned C damage/flight callbacks in a host harness. It compares 208 balance/character cases and five per-update lifecycle traces: pickup, protected pickup, expiry, pause, range-out, shadow hits and misses, tilted contact planes, registration coordinates, and velocity/position. Enum/call decisions are exact; host matrix values allow 0.002 native units of absolute error. These comparisons do not exercise retail graphics, allocator failure, native matrix approximations or the actual game service. Reproduce with SA2B_REFERENCE, CC, optional SA2B_ZIG_CC, and SA2_SET_DIR configured as in the other stage tests:

    cargo test --locked --test damage_ring_trace -- --include-ignored
