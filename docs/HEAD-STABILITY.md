# Fixed Chao head presentation

November 18, 2026. The user requested no head bob, including after the previous
reduced-sway adjustment. The final presentation locks the visible native head
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
actor-relative head position. This supersedes the historical reduced-head-sway
description for the final rendered on-foot head.
