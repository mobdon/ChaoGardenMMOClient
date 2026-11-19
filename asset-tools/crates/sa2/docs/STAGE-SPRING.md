# Stage spring tasks

The Rust port implements the in-range ObjectSpring and ObjectSpringB task updates and their shared VacumePlayer launch routine. The source is [roeming/SA2B, commit 874a480, src/OBJECT/o_spring.c](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/src/OBJECT/o_spring.c). Upstream [marks this translation unit matching for the GSNE8P GameCube build](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/configure.py#L333). The expected main.dol SHA-1 is e4c50f32242a3cd36cdc8ae776661601a52130a2. PC stage layouts, where used by a host, have a distinct provenance.

StageSpring::step executes one scheduled update. A caller provides the collision service's selected player slot, live state of the previously selected player on a touch tick, the native transformed local vector (0, 5 + scale.y, 0), and angle lookup values for animation. Native matrix calls in source order are unit, rotate Z, rotate X, rotate Y, calculate point. The caller processes SpringCall values in order. A SetPosition entry represents a direct write to the selected player's task position. Input represents SetInputP(slot, code, 0). Function8002fb2c preserves the upstream unidentified function and its arguments.

Both springs initialize their collision shape with ID 4 and set collision-work flag 0x40. Normal ticks maintain the compression wave and register collision. Contact within the first two counter increments resets the counter. A later hit with a recognized slot enters Touch, suppresses collision registration for that tick, and launches on the next tick. Without a valid selected player the task stays in Touch and emits the sound on each execution.

| Source task | Launch | Observable differences |
| --- | --- | --- |
| ObjectSpring (A) | Calls SetSpringVelocityP with the transformed local vector. | Sound precedes the player update. Recovery step changes to 18 degrees at phase 290. |
| ObjectSpringB (B) | Calls SetVelocityYAndRotationAndNoconTimeP with scale.y + 5, native angles, and truncated nonnegative scale.x. | Sound follows the player update. Recovery step changes to 15 degrees. The slow movement branch also places the player at spring position plus the rotated vector, with the vector's Y scaled by 0.8. |

The stage host supplies player collision, matrix and trigonometric services, player-motion callees and rendering. The implementation returns sound and motion calls without playing audio, inventing collision results or moving a controller after launch. The A and B render callbacks and their original art are outside this port.

## Reproduce the C comparison

Use the compiler and checkout variables in [STAGE-GAMEPLAY.md](STAGE-GAMEPLAY.md), from the sa2 crate directory:

```powershell
cargo test --test spring_trace -- --include-ignored
```

The test extracts both complete task bodies and the shared player launch body from the pinned upstream file, compiles them with a host C shim and compares 16 scenarios over 75 ticks each. The C and Rust traces matched in all 1,200 updates. Each row compares modes, player slot, timers, recovery phase, player position, and the observed order of engine calls. Fixture assertions also compare launch arguments. Floating fields are compared to the nearest 0.001. The shim injects the same transformed vector and host trigonometry on both sides. This verifies the callback logic separately from retail matrix, trigonometry, collision and player implementation.
