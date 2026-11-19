# Big Foot opening

From the worktree, select Hero Story Sequence action 2 (level 60) using a private request and a fresh output path:

    sa2-assets --source-root GAME fan-big-foot GAME/sonic2app.exe .local/big-foot.json PC_SA2 "Hero Story Sequence" 2 REQUEST.json

For example, `{"work_allocation_ok":true,"frames":[{"player_position":[120,50,0]}]}` exercises the boundary at equality. Each frame supplies the current slot-zero player position. Only outside the boundary, add `vector_result` with the observed result of the imported helper. The callback passes the complete XYZ vector, writes 120 times the returned X and Z, and preserves the original Y. The first task invocation creates the boss; the frame list begins with subsequent stage updates. This is not a player controller.

The pinned boss_bigfootD.rel (module 81) registers its BIGFOOT descriptor in section 1 at 0x6434. The descriptor points to stage startup at 0x6460, teardown at 0x6628 and the stage task at 0x66BC. That task calls the boss constructor at offset zero with position (90, 0, 0) and yaw -0x4000, then installs the stage update at 0x67D4. This is the boss spawn, not an entry pose for Sonic. The successful boss work allocation is 0xC8 bytes; failure frees the boss task without preventing the stage callback switch. On success the boss installs eleven collision descriptors, starts in mode 1, and requests animation 6. The replay covers these decisions; the boss AI, model rendering, animation playback, collision resolution, damage, defeat and transition out of the fight remain open.

With SA2_GAME_PATH, SA2_PC_CONFIG and a Python environment with Unicorn PPC configured, run:

    python asset-tools/crates/sa2/tests/big_foot_ppc.py PATH_TO_SA2_ASSETS

The test executes the original stage task, constructor and state initialization, then the arena update including its local PowerPC square-root routine. It observes imported engine calls and the resource setup helpers. Both work-allocation paths and 202 updates are compared against Rust using the mounted story command. Those comparisons exercise equality, adjacent floats, unchanged heights, non-axis-aligned inputs and nontrivial vector-helper results; they are not an exhaustive floating-point proof. No executable bytes or generated output are checked in.

## Intro and player placement

The general single-player entry table has no Sonic row for Big Foot. The boss code supplies a separate, verified position: on its first mode-1 callback it writes (-boss.x, 0, 0) to slot zero. With the native boss spawn at (90, 0, 0), that places Sonic at (-90, 0, 0). This write establishes neither his yaw nor where the engine placed him before the callback.

Add `"intro":{"updates":451,"helper_task_created":true,"exit":{"query_result":1,"player_position":[-90,0,0]}}` to the request to run from fresh boss setup through the handoff to mode 2. For fewer updates omit exit. These count actual invocations of the mode-1 routine, independent of the stage arena updates; this is not a claim about elapsed wall time or global scheduler timing. The entry sets up the view, writes the player position, requests motion 6, calls the setup helpers, and sets the timer to 450. On the 450th subsequent invocation it changes the view, disposes of the optional helper, handles the native query, and enters mode 2 with timer and submode reset. A positive ordinary query result requires the current player position, since the native exit helper reads slot zero again; the first-frame position need not still hold. The local spawn call uses the adjusted Y coordinate and a zero-based argument capped at two.

The PPC comparison runs the mode-1 routine (section 1, 0xD90) and its local focus and exit helpers. Three complete trajectories compare player placement, camera argument blocks, ordered calls, flags, timers and transition state (1,353 boss updates). Imported calls and the effects of nested setup services remain observed. Combat AI after the handoff and victory are still open.
