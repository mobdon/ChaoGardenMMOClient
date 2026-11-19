# Fixed stage ring callbacks

The Rust stage object implements fixed, in-range SET rings for the GameCube release of Sonic Adventure 2 Battle (GSNE8P). The source is the [roeming/SA2B reconstruction at 874a480](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/src/OBJECT/o_ring.c#L401-L660). Upstream [marks o_ring.c matching](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/configure.py#L334) and [records the reference main.dol SHA-1](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/config/GSNE8P/build.sha1) as e4c50f32242a3cd36cdc8ae776661601a52130a2. The callbacks have distinct collision contracts.

| Callback | Contact input | Active frame behavior |
| --- | --- | --- |
| Ring | CCL hit bit plus non-null CCL_IsHitPlayer task, represented by RingHit with its resolved player index and item flags. The ring registers a radius-5 sphere. | Calls CCL_Entry, including on pickup. |
| o_ring_M1 | First of the consecutive available player collision centers enabled by the player mask and strictly less than 14 units from the ring. | Uses the explicit center test. Slot zero takes precedence if both qualify. |

Initialization consumes a task tick. A pickup on a later active tick emits a request for the four native particles. For player 0 or 1 with item bit 0x4000 clear it also requests AddScore(10), AddNumRing(slot, 1), AddMechHP(slot, 0.1) and SE_CallRing(slot), in that order. The host applies those requests inside its stage session. When the spin gate is open yaw advances by 0x16c even on the pickup tick. The next scheduled tick calls DeadOut for the SET-backed task. The Rust Removed state represents a task the scheduler has already removed.

The caller supplies ordinary CCL contact, spin gating and scheduling. The implemented path assumes a fixed ring with no magnetic shield on either player. Terrain-following rings, range-out and respawn, damage-scattered rings, effect rendering, the collision engine, reward callees and saves are outside this callback implementation. The source reference is GameCube. PC retail instruction parity remains unverified. Porting this task establishes no Chao service or persistent reward behavior.

## Reproduce the C comparison

The regular test checks representative execution rows. The optional oracle extracts both complete callback bodies directly from the pinned upstream C source into a temporary translation unit. A host C shim supplies engine services for the fixed-ring scenarios, stubs rendering and shadow/attraction, checks the exact reward arguments and call order, and records observable calls and state. The host C and Rust outputs matched across nine scenarios and five updates each. This comparison executes reconstructed C on the host, without a retail process, native PowerPC math library or disc. Distance fixtures use axis-aligned centers.

From the sa2 crate directory with a clean checkout of the pinned upstream commit and a host C compiler:

```powershell
$env:SA2B_REFERENCE = 'C:\path\to\SA2B'
$env:CC = 'C:\path\to\zig.exe'
$env:SA2B_ZIG_CC = '1'
cargo test --test stage_gameplay_trace -- --include-ignored
```

For GCC or Clang point CC at the compiler and leave SA2B_ZIG_CC unset. No extracted native files are required or produced.

On pickup, shared Ring replays also report the four ordered MakeParticle requests from o_ring_0 at the ring position before any magnetic advance that tick. Each carries the native position, velocity and scale argument. The core accepts a native angle lookup; the replay labels its host sin/cos approximation. The installed Green Forest test checks the pickup at layer 0, placement 7. These requests do not implement the particle renderer.

For ordinary Ring placements with SET scale.x == -1, the shared replay now follows the terrain branch of o_ring.c (initialization and active cases). Supply the stage-global collision timer at initialization in terrain.timer (0 through 8). When GetShadowPos is called, terrain.shadow supplies height and yaw_after. A due probe consumes terrain.reference_position; only distances strictly below 400 query the shadow. A height change strictly below 8 moves the ring vertically. Larger changes, including exactly 8 in either direction, request a Tobitiri task; supply terrain.allocated and, on success, terrain.random. The original SET task retires even if allocation fails. Rotated no-shadow markers and magnet selection disable later probes. Terrain queries, scheduling of the spawned child, and the global timer shared with other ring tasks are external to this single-placement replay.

A successful terrain scatter preserves its child position and yaw for the same Tobitiri flight core used by damage rings. To schedule its first successful work allocation, supply a frame child object with op "launch", the current shared probe timer (0..8), gravity vector, pitch, roll, spin_random and paused flag. Later child frames with op "tick" supply gravity, paused, range_out and optional hit [slot,item]. Supply probe: {"surface":null} on a floor-query miss, or a surface with coordinate, pitch and roll on a hit; omit probe when no query runs. Child execution follows the parent update when scheduled, and continues after the SET parent is removed. Omitting child leaves its work pending. The replay reports pickup credit requests without modifying inventory or saves.

Dropped-ring replays report pickupAndRemovalCalls in the order used by Tobitiri: a credited hit requests one ring, ring sound and 0.1 mech HP. On the next in-range collection update the task requests four particles at its current position, then destruction. Expiry requests destruction alone. The stage adapter and terrain child share this translation. These are engine requests; applying rewards, mixing sound and drawing particles remain separate.
