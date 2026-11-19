# City Escape spring playback

The city-replay command runs the reconstructed GSNE8P spring task on one placement from the installed PC City Escape SET. Supply an installed matching executable so the CLI can resolve the placement's stage object callback. A trace supplies observations of services that the spring calls. Each frame is one task execution. This host writes a replay report and does not update a player save.

```text
sa2-assets city-replay SET OUTPUT EXE INDEX TRACE.json
```

The SET index is zero-based file order. For an upright placement whose SET Y scale is 2, the local launch vector (0, 5 + 2, 0) remains (0, 7, 0). A minimal trace for such a placement is:

```json
{
  "frames": [
    {},
    {},
    {},
    {"hit": 0},
    {"player": {"position": [12, 15, 28], "running_speed": 1.25, "item": 0}}
  ]
}
```

For tilted placements supply the observed matrix result. For zero X and Z rotations the host derives the vertical vector directly from SET scale, regardless of yaw. An explicit observation always takes precedence; the report distinguishes supplied_out_point (null when omitted) from effective_out_point. out_point is in the rotated basis and excludes the spring's world position. The callback effectively transforms (0, 5 + scale.y, 0) using the native unit matrix and rotate-Z, rotate-X, rotate-Y sequence. SET rotations are converted from stored unsigned words to sign-extended angles, following the PC [OBJ_EDITENTRY declaration](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/include/samt/sonic/set.h#L45-L58). Animation uses host double-precision sin/cos evaluated at the wrapped 16-bit angle and rounded to f32. The report records that approximation. The host does not reconstruct tilted transforms or apply the launch callees.

An empty frame has no collision hit. A hit of 0 or 1 denotes the slot identified by direct task-pointer comparison. A hit of 2 denotes a non-null CCL result that matched neither player pointer. On the subsequent Touch tick, player supplies the selected slot's current state. An absent player on that tick leaves the task in Touch. A frame with range_out true returns before the task update. The adapter accepts up to 2,048 frames and 1 MiB of JSON; unknown fields, invalid slot values, nonfinite input and inconsistent player observations are rejected. The report records callback state and ordered effect requests for each frame. See [native spring callback coverage](STAGE-SPRING.md) for what the core executes.

## Installed City Escape comparison

On the normal PC layout `set0013_s.bin` (SHA-256 `9e7ab581b5b85dd09a4eac1d5b34afd4785b49f17575cb78e4c3c814cf05ff14`), placement 75 resolves to ObjectSpring. Its X and Z angles are zero, yaw is 4096, and its SET Y scale is 3.5000014305114746. Running the trace above with the installed executable produced the same five callback frames as supplying the independently computed single-precision vector `[0, 8.500001907348633, 0]`. The last frame requested that vector as player spring velocity. This compares two replay paths on installed data; player physics and stage completion were not exercised.

## Controller conditioning

The separate Rust input::pad_clamp operation runs the four-channel Dolphin SDK PADClamp routine reconstructed for this build. It handles the two sticks' distinct octagonal limits, signed-byte extremes, trigger dead zones, and pads reporting a read error. Buttons and analog A/B values pass through. The input test compiles the pinned [upstream C routine](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/src/Dolphin/pad/Padclamp.c) and checks every signed (x, y) byte pair for both sticks and all trigger values. Upstream marks Padclamp.c matching. This establishes the library routine; a call chain from City Escape into it and the player's subsequent analog interpretation remain unverified. The PC replay consumes supplied callback observations rather than controller samples.

With the tools and environment configured in [the ring reference instructions](STAGE-GAMEPLAY.md), run:

```powershell
cargo test --test stage_spring_host
cargo test --test input_trace -- --include-ignored
```
