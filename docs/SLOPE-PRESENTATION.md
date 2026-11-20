# Terrain movement presentation

November 18, 2026. Follow-up to the user's repeated visible head bob and skating
stopping when floor height changes. Client base is
`025dcce565d42d22bae3fc558695cbe4f1b92541` on `codex/steady-chao-head`.

The actual frozen Rust authority computes vertical prediction against the
previous floor height, then publishes the new floor and `jumpOffset`. Native
downhill movement can consequently publish small genuine air gaps. The old
client instantly reset skate blend, phase and speed on every such observation,
and changed its underlying native Run to Idle. A 72-sample browser reproduction
contained ten airborne samples at full 1.994 speed with zero skate blend. An
independent ordinary-speed Rust trace reproduced 100–200 ms gaps; a faster
browser trace measured a maximum gap of 0.007444 world units.

The browser now carries an already fully active skate pose across a qualifying
descending near-floor observation: actor and authoritative floor must both
descend, speed must remain at least 1.30, and the reported gap must be finite,
nonnegative and at most 0.015 world units. Only a strictly newer server sample
renews the 0.4-second pose deadline. A continuous slope can renew it; a repeated
old sample cannot. The existing 350 ms freshness guard clears a stopped stream
first. Grounded remains false and native shoe jets stay off throughout a coast.
Rising motion, larger gaps, missing floor history and expired coast leave the
pose through the existing 0.28-second fade. Mounting, water, stale state,
missing shoes or missing source motion clear it immediately. It cannot start
a new skate in air. Native timing, rigs and authoritative position do not change.

The camera separately tracks the already interpolated actor directly, removing
the extra focus lag that made a steady head move on screen. The actual native
shadow texture is projected onto nearby terrain triangles; see
NATIVE-CHAO-SHADOW.md. This is presentation work, not new movement authority,
terrain collision, progression, saves or a retail engine port.

Validation: 32 client tests, production build, 11,880-frame three-form head rig
proof, and three-form native skate proof pass. The skate proof additionally
checks 90 sustained coast frames with 90 distinct phases per form and exits
for a larger air gap. Actual isolated browser captures show zero camera focus
height error, under 0.001 px head-center vertical range after turns, continuous
full-speed slope movement and native contact shadows on the original grass.
The final slope captures did not reproduce positive gaps on every run; the
renewed coast rule is separately tested against the measured gap range and
native motions. Do not label those final grounded captures an airborne-coast
integration proof. An attempted moving-jump browser capture timed out and is
not acceptance evidence; Rust jump and focused pose rejection tests passed.

Ignored evidence: `.local/reviews/slope-down-before.json`,
`slope-down-after.json` (the first total-duration coast attempt, which still
dropped out on a long descent), `slope-down-final.json`,
`slope-down-final-2.json`, `slope-shadow-final.json`,
`head-render-order-audit.json`, `slope-air-service-proof.json`, and
`.local/imports/shadow-skate-runtime-proof.json`. Original inputs and output
pixels remain private/local. Tests use disposable profiles and an explicit
isolated Rust port; canonical state and the frozen live backend stay intact.
