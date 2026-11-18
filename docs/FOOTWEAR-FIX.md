# Animated footwear and chest attachment fix

November 18, 2026. Client branch `codex/footwear-motion-fix`, based on
`435f6a3689c2f25ebc1c6526d2075d59321c7b28`. This is a browser presentation
change using the existing Rust authority and protocol.

## Latest presentation follow-up

Current branch `codex/music-controls`, base
`a7a8d093838d66cc5fa190aaf71a60a5ad410de9`. Earlier sections below preserve
historical checkpoints; the current mirrored fit supersedes the separate
left/right shoe fitting.

The user's newest corrections require exactly mirrored boots, the same fit in
the title portrait, smooth whole-body skating with less head bob, and gloves
that stay on the visible palms. These follow-ups are implemented in the current
client source; the older clips below do not establish their browser acceptance.

The runtime now reuses the original RIGHT Shadow shoe alias/model78 for both
feet. One actor-local rest fit supplies the common0.90 length ratio and uniform
scale magnitude; the left placement is the exact X reflection of the right.
Each remains parented to its own native006/013 foot. Original right jet models
96–99 reflect with that shell. This explicit mirroring adaptation preserves
the original right-model geometry/materials and all native Chao joints; it is
not a claim that the distinct native left model was loaded unchanged. See
[MIRRORED-GEAR-FIT.md](MIRRORED-GEAR-FIT.md).

The three-form `.local/reviews/mirrored-shoe-proof.json` checks207 boot and48
jet vertices per side, native Walk/Run alternation and skating. Rest mirror
error is0 for Neutral/Dark and3.73e-9 for Hero; verified source toe yaw is below
8.06e-7°. Production helper mounts are recorded in
`.local/reviews/mirrored-shoe-mounts.json`. The title-image source uses the same
mirrored right shoe, with independently measured1.676e-8 vertex error; final
title-image and in-game front silhouettes were subsequently checked in the
actual browser. Both now show the same mirrored pair.

Visible native arm/palm geometry belongs to003/010. Their meshless children
004/011 are distal bind markers, and the skating overlay rotates them
independently. Gloves now capture and mount on003/010, using004/011 only to
find the original distal-palm direction. The original rigid glove fit and
Chao rig remain intact. `scripts/verify-native-glove-contact.mjs` executes
production socket/mount helpers with the actual original left/right glove
props across all three forms and2,166 frames. Its ignored
`.local/reviews/native-glove-contact-proof.json` records maximum contact drift
3.43e-8 world units and relative-matrix error3.58e-7; the former leaf-socket
baseline drifts up to0.04654 world units.

The latest gear checkpoint passed21 client tests and the production build.
The slower skate blend/cadence and18° head/24° torso limits are documented in
[SHADOW-SKATE.md](SHADOW-SKATE.md). Actual browser/service validation of the
combined latest gear, material, music and Watch controls has a bounded browser
checkpoint below. Wider terrain, multi-client and transition acceptance stays
open. No backend rollout or canonical-save migration is established by these
source proofs.

### Current browser/service checkpoint

On November 18 the actual browser at isolated port 5176 used the frozen Rust
service and a disposable admin Chao with 20 rings. Music played the existing
native hub MP3 after a button click. Muting, changing volume while muted,
independent NPC voice enablement, and reload persistence passed. Arrow keys
adjusted the focused slider without moving the Chao; W on the slider left
server position unchanged, then W on the canvas moved Z from 6.000 to 7.526.
The revised controls use blue/gold menu styling and a custom ring-shaped
slider thumb. The 1280×720 header had no page overflow.

Watch start changed the server snapshot to the Neutral Garden with active
Watch. Take control confirmed manual mode and focused the canvas. Gloves were
equipped through the real service, both gear slots survived explicit Save and
reload, and the browser rendered the mirrored front stance and original
title portrait. Music stayed muted. The fixture later loaded copied native
terrain files as well, avoiding a flat-server/native-renderer mismatch.

The tip/hand material change restores native bilinear filtering and clamp
addressing. It does not establish that those opaque body surfaces had an alpha
leak: their original pixels were already fully opaque. The front silhouette
shows the solid native body and intended flame/emerald translucency; additional
view/terrain acceptance remains open.

Fresh checks: 21 client tests/build; real disposable Rust proof 14 checks,
60 samples/58 observer snapshots; actual three-form shoe proof; 2,166-frame
native glove-contact proof; and 648-frame Shadow retarget proof all passed.
Local screenshot: `.local/screenshots/mirrored-gear-and-sound.png`. Browser
testing changed only disposable profiles; the canonical 139-ring Chao was
observed through Tiny Garden without care, purchases or minigame mutations.

## Current follow-up integration

The later `codex/native-presentation-fixes` branch replaces the historical
adapted shoe sprites with original Shadow jet meshes 92–99. See
[NATIVE-SHOE-EFFECTS.md](NATIVE-SHOE-EFFECTS.md) for Rust preparation, exact
source and unresolved retail emitter/timing parity. The approved head flame
is unchanged.

Each shoe now uses its actual native heel-to-toe direction, removing about
11 degrees of inward yaw. Uniform length fitting is 0.90 instead of 1.06,
making both boots 15.1% smaller on every axis. Native joints and sole curvature
remain unchanged. The cyan chest gem center sits another quarter of its depth
inside the measured skin, halving its nominal exposed depth.

Final client tests pass 15/15 and production build passes. The smaller fit
passes three-form motion/rig checks and 1,086 production-mount samples with
toe yaw below 0.000001 degrees. The existing disposable real Rust protocol
proof passed 14 checks with two clients; subsequent actual browser input
verified the refined presentation at Walk 0.798, Run 1.236 and Skate 1.994
units/s. Native jet IDs activate only during skating. Browser warning/error
logs were empty.

New ignored evidence: `.local/screenshots/native-footwear/evidence.json`
contains final renderer hashes and 209 timestamped actual UI frames.
`.local/screenshots/chao-refined-walk-run-skate.mp4` is the resulting
9.708-second 1280×720 clip, resampled to 24 fps with gaps between takes trimmed.
`front-refined.png` in the evidence folder shows the deeper gem/smaller shoes;
`live-garden.png` shows the canonical Chao after integration.

The two prepared shoe aliases were installed through the existing asset
routes after backing up the originals in
`.local/backups/native-shoe-originals-20261118/`. A fresh Rust/SAIO/PBR
reproduction matches both installed hashes exactly. Existing backend/frontend
processes stayed running. Chaos retains 139 rings, gear, Watch movement and
muted audio; no backend rollout or profile migration occurred. Broad terrain
contact and exact native emitter draw matrices/timing remain open.

## Result and original source

The original Chao Walk and Run119 already alternate their feet. Shoes were
attached to static leg pivots 005/012 while the actual native articulation is
on 006/013. The visible native foot geometry was hidden beneath those static
shoes. Footwear now captures its bind-relative placement on 006/013. The
original Shadow shoe basis and uniform prop fitting remain in use.

The Shadow skating overlay now resolves desired foot orientation directly on
006/013, retaining its world-space bind adaptation and angular limits. It
does not compound a parent overlay with a separately animated child foot.
No native joint positions, parents or proportions are changed by this fix.

The cyan chest emerald is centered on a ray intersection with the captured
bind-pose torso skin, embedding approximately half its depth. It retains the
native torso 001 attachment, original model/texture and depth-occluded bloom.

The historical heel jets reused the original sixteen-frame Chao flame atlas as an explicitly
adapted effect. These are not claimed to be extracted Shadow jet meshes.
Crossed transparent planes follow each animated foot, fade with the existing
skate blend and immediately disable for unavailable/stale authority, missing
shoes, mounted, swimming or airborne state. Gear disposal removed those jets
without disposing the shared atlas/material used by the head flame.

The atlas-based shoe jets described above are historical. The source now uses
the original Shadow models92–99 when locally prepared shoe aliases contain
them; old aliases receive no fabricated fallback. See
[NATIVE-SHOE-EFFECTS.md](NATIVE-SHOE-EFFECTS.md) for exact sources, reproducible
Rust conversion, original material/alpha evidence and remaining visual gates.

## Verification

- `npm test`: 14/14 client tests passed, including actual attachment motion,
  torso triangle intersection, overlay transforms and jet lifecycle.
- `npm run build`: passed; existing runtime asset-resolution and large-bundle
  warnings remain.
- `node scripts/verify-chao-footwear.mjs`: local retail-asset proof passed on
  Neutral/Hero/Dark Chaos. Each form sampled 121 Walk and 121 Run frames plus 165
  steady skating frames. Original hierarchy/proportions remain intact. Native
  near-one scale channels retain float32 variation within 2e-6.
- Old static sockets move less than 2.8e-7 native body units; fitted shoes on
  actual foot joints move independently. Opposite-foot vertical lift
  correlation ranges from -0.968 to -0.993. Foot pitch arcs mean fore/aft center
  correlation alone is not a valid alternating-stride test.
- `node scripts/verify-footwear-service.mjs`: 14 checks passed, 59 owner samples
  and 58 observer snapshots using new disposable Rust-owned profiles. Checked
  gear acknowledgment/visibility, bounded Walk/Run/Skate input, jumping,
  moving Walker, stop/dismount and a real disconnected-client freshness reset.
- Real UI controls in the actual `createGarden` renderer recorded Walk 0.798,
  Run 1.236 and Skate 1.994 units/s. Both heel jets activate during skating and
  remain absent for walking/running and after shoe removal. No hidden motion
  or camera state was forced. Browser error/warning logs were empty.
- The front view shows the chest gem embedded in the body. Head flame alpha,
  opaque body, seven native orbiting gems and their depth occlusion remain.

The disposable service and browser fixture used the frozen Rust executable
SHA-256 `51a07f9c382ecb70785654297180349dd2e34f677d5921818ea2b9a6ecec3100`.
They exited after verification. This is no fresh server source-build claim.
The existing live backend process and canonical Watch routine were preserved;
the saved Chaos remains in its garden with 139 rings and existing gear.

Ignored local evidence:

- `.local/reviews/chao-footwear-proof.json`: original asset hashes and all axes.
- `.local/reviews/footwear-service-proof.json`: source/runtime hashes and checks.
- `.local/screenshots/footwear-fixed/evidence.json`: exact adapter hashes,
  timestamped 198-frame UI capture and segment timings.
- `.local/screenshots/chao-feet-fixed.mp4`: fresh 8.375-second 1280x720 clip,
  resampled to 24 fps from real screenshot timestamps; three segments concatenated.
- `.local/screenshots/footwear-fixed/chest-front.png`: embedded chest front view.
- `.local/screenshots/footwear-fixed/without-shoes.json`: native run/no jet guard.

## Remaining work

Broad slope/contact tuning and all native movement transitions remain open.
This fix does not merge the separate native JumpUp/JumpDown/reverse137 proposal.
Read-only review of its clean commit
`86f41f9ca914068b712797528b924627c206c749` confirmed missing/invalid heights
now reset safely. Duplicate server timestamps still reset airborne history
instead of being deduplicated, which can suppress the next landing. Native
locomotion and skating share one water flag, but the legacy scene fallback
still uses a different heuristic. That proposal also needs to retain 006/013
targets when eventually rebased. It remains unmerged pending its own acceptance.

No server rollout, save migration, TypeScript authority replacement or full
game-engine port is included.
