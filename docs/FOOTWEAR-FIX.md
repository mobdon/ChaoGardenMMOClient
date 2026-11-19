# Animated footwear and chest attachment fix

November 18, 2026. Client branch `codex/footwear-motion-fix`, based on
`435f6a3689c2f25ebc1c6526d2075d59321c7b28`. This is a browser presentation
change using the existing Rust authority and protocol.

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
