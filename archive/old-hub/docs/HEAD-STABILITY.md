# Natural Chao head presentation

## Current policy — November 18, 2026

The latest user direction is natural head movement while walking, a steadier
head while running, and a slight bob matching each skate stroke. The prior
all-gait fixed head looked unnatural and is superseded by this policy.

Native Idle and Walk retain their original head position and rotation. Run
eases the visible native head frame **016** toward its captured actor-relative
standing position and orientation over 0.18 seconds. Returning to Idle or Walk
releases that correction over the same interval; the settled pose then uses
the complete native animation again. Seated Walker pilots immediately restore
the original head pose and bypass the standing correction.

Skating adds a small vertical stroke along the actor's local up axis. Its
offset is `sin(2π × skatePhase) × 0.002 × skateBlend × headLockBlend` in world
distance. The phase comes directly from the actual source-adapted skate
cadence, not elapsed wall time or the underlying Run119 animation. Actor-world
up-axis length converts that world distance to actor units, preserving the
0.002-world-unit maximum when uniform actor scale changes. There is no added
head tilt or horizontal sway. The easing, head constraint and skate bob are
explicit presentation adaptations, not claims about original retail behavior.

The helper restores the prior native local position/quaternion before the next
animation update. After native animation, the skate overlay and foot grounding,
the scene passes the actual gait, skate blend and source phase into
`update(dt, { mounted, gait, skateBlend, skatePhase })`, then solves through the
current parent. Original geometry, hierarchy, scale channels and non-head
limbs are preserved. Rust movement, progression, saves and protocols do not
change. Direct camera tracking of the already interpolated actor remains in
place; intentional walk/skate bob must not be mistaken for camera lag.

Current validation: seven focused helper tests pass. They cover exact natural
Idle/Walk channels, settled Run/eye stability through parent animation and
grounding, smooth entry/exit, native-phase matching and partial-blend weighting,
world-amplitude bounds at actor scales 0.6/1/1.7, mounted restoration and missing
rigs. The current actual-native proof passed 45 trials and 14,310 frames across
Neutral/Hero/Dark Chaos: 4,110 exact-native natural frames, 420 transition
frames and 9,780 settled lock frames. Maximum expected world-position error
was 9.41e-8, settled orientation error 5.40e-6 degrees and source-phase bob
error 6.66e-8 world units. Maximum measured stroke was 0.002000041 world units,
within floating-point roundoff of the 0.002 cap at all three tested scales.
Non-head local channels were unchanged, native feet remained animated and
immediate mounted restoration passed. Run the proof with
`node scripts/verify-steady-head.mjs --output .local/reviews/steady-head-gait-policy-proof.json`.
The report remains ignored/local at that path.

This is a CPU transform proof with actual native rigs, motions, mirrored shoes
and production flat-ground foot fitting. Fresh browser/service acceptance,
rendered neck silhouette, nonuniform/tilted actors and wider terrain/device
coverage remain separate. Earlier all-gait zero-bob measurements below do not
establish acceptance of this changed policy. Helper diagnostics expose
`mode`, `blend`, `bobAmplitude`, `bobOffsetY`, normalized `phase` and the measured
actor-relative head position; bob amplitude/offset are world distances.

## Historical all-gait lock — superseded

Earlier on November 18 the user requested no head bob after reduced-sway and
actor-local adjustments. That revision locked the visible native head
frame **016** to its captured actor-relative standing position and orientation
through idle, walking, running, skating and stops. It follows actor movement,
heading and terrain height. Seated Walker pilots retain their cockpit pose.

Native torso motion, animated neck pivots 014/015 and foot grounding previously
moved the skull. Native Idle rotates 015, Walk rotates 016 and Run119 rotates
015; locking only 014 or reducing the Shadow head rotation leaves visible bob.
The final constraint solves 016 through its current parent after native
animation, Shadow adaptation and visual foot grounding. Its original pose is
restored before the next animation update. That revision intentionally used no
gait threshold or entry/exit fade; the current policy above replaces that rule.

Only the head frame's presentation position/quaternion is compensated. Original
geometry, scale, hierarchy, limb motion, gear, effects and Rust authority remain
unchanged. No game-format reader, game system, save or protocol changed.

Historical validation: 26 client tests and production build passed initially;
the later terrain presentation revision passed 32 tests/build. The earlier
11,880-frame proof exercised all three Chaos forms, Idle/Walk/Run119 and
all three skate bands with actual mirrored shoes, foot grounding and actor
turns. All 21 head/eye meshes remain steady from the first frame: maximum
visible geometry-center error is 1.96e-7 world units and orientation error
9.66e-6 degrees. All other local channels, scales and parents are unchanged;
native feet continued moving and mounted restoration passed. Native data stayed
ignored/local. The prior output pathname `.local/reviews/steady-head-proof.json`
was reused during the new-policy probe and is not an untouched historical
receipt; the earlier metrics above are preserved as historical evidence. The
current policy has the separately named receipt documented above.

That browser revision reported `data-head-stabilized=true`, blend 1 and the
actual actor-relative head position. This alone did not establish a steady rendered
view: the user's repeat report was correct. Actual draw-matrix investigation
ruled out stale child matrices and skinning. The camera then applied a second,
slower interpolation to the already interpolated authoritative actor. An
isolated native-garden downhill pass measured 0.042728 world units of focus
height error despite the head being fixed relative to the actor.

The camera now tracks the rendered actor directly in X/Y/Z, retaining orbit,
zoom and collision controls. Read-only diagnostics project an actual head
mesh's cached draw matrix, without recomputing that mesh after rendering.
The historical native-garden downhill pass measured zero focus height error and
less than 0.001 pixel of head-center vertical variation after the turn, while
the actor moved over the slope. This supplements, rather than replaces, the
11,880-frame native rig proof. Broader camera collision/terrain/device behavior
remains separate acceptance. See SLOPE-PRESENTATION.md for trace paths and
the independent skate transition correction. The new intentional walk/skate
head motion means an all-gait screen-space range below 0.001 pixel is no longer
the acceptance criterion. The recorded-observation replay harness also retains
its old zero-motion criterion until explicitly updated for this policy.
