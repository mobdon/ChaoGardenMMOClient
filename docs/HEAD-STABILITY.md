# Fixed Chao head presentation

November 18, 2026. The user requested no head bob, including after the previous
reduced-sway and actor-local adjustments. The current presentation locks the visible native head
frame **016** to its captured actor-relative standing position and orientation
through idle, walking, running, skating and stops. It follows actor movement,
heading and terrain height. Seated Walker pilots retain their cockpit pose.

Native torso motion, animated neck pivots 014/015 and foot grounding previously
moved the skull. Native Idle rotates 015, Walk rotates 016 and Run119 rotates
015; locking only 014 or reducing the Shadow head rotation leaves visible bob.
The final constraint solves 016 through its current parent after native
animation, Shadow adaptation and visual foot grounding. Its original pose is
restored before the next animation update. There is no gait threshold or fade
that lets bob return on entry or exit.

Only the head frame's presentation position/quaternion is compensated. Original
geometry, scale, hierarchy, limb motion, gear, effects and Rust authority remain
unchanged. No game-format reader, game system, save or protocol changed.

Validation: 26 client tests and production build pass. Run
`node scripts/verify-steady-head.mjs` with the existing ignored native inputs.
Its 11,880-frame proof exercises all three Chaos forms, Idle/Walk/Run119 and
all three skate bands with actual mirrored shoes, foot grounding and actor
turns. All 21 head/eye meshes remain steady from the first frame: maximum
visible geometry-center error is 1.96e-7 world units and orientation error
9.66e-6 degrees. All other local channels, scales and parents are unchanged;
native feet continue moving and mounted restoration passes. Native data and
the full receipt remain ignored at `.local/reviews/steady-head-proof.json`.

The current browser reports `data-head-stabilized=true`, blend 1 and the actual
actor-relative head position. This alone did not establish a steady rendered
view: the user's repeat report was correct. Actual draw-matrix investigation
ruled out stale child matrices and skinning. The camera then applied a second,
slower interpolation to the already interpolated authoritative actor. An
isolated native-garden downhill pass measured 0.042728 world units of focus
height error despite the head being fixed relative to the actor.

The camera now tracks the rendered actor directly in X/Y/Z, retaining orbit,
zoom and collision controls. Read-only diagnostics project an actual head
mesh's cached draw matrix, without recomputing that mesh after rendering.
The final native-garden downhill pass measured zero focus height error and
less than 0.001 pixel of head-center vertical variation after the turn, while
the actor moved over the slope. This supplements, rather than replaces, the
11,880-frame native rig proof. Broader camera collision/terrain/device behavior
remains separate acceptance. See SLOPE-PRESENTATION.md for trace paths and
the independent skate transition correction.
