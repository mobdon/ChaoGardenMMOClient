# Native movement PR validation

November 18, 2026. Isolated client branch `codex/native-chao-movement`, based on
published `279a7b7`. Browser presentation only: the existing Rust service owns
speed, jump physics, terrain, equipment and ownership. The live preview,
installed game, canonical profile and server source are untouched.

## Behavior and integration boundary

Native Idle/Walk and Run119 remain the movement foundation. Run enters at 1.05
units/s and exits below 0.95. Authorized Air Shoes enter genuine Shadow-derived
skating at 1.55 and exit below 1.30. Its 0.18-second blend and cadence
`speed / 1.8` remain presentation adaptations. Motions172/173/174 come from the
audited original data and cannot grant movement.

The compact Chao library also contains genuine `alm_jump_up` and `alm_jump_down`.
Native132 frames0–6 at0.2 and133's apex frame6 drive ascent; 135 frames0–4 at0.2
and136's frame4 drive descent. Ground contact triggers137's reverse4–0 recovery
at−0.2. Mapping these poses to server jump stages and holding the apex/descending
frames are explicit adaptations, not retail physics/state-machine parity.
Landing finishes before skating resumes.

Mounted Chao retain native Idle while the existing native Walker controller
handles vehicle actions. Stale, swimming and mounted observations clear air
history so reconnect/dismount cannot fabricate a landing. Existing Run-only
libraries remain usable; missing air clips fall back to Idle and diagnostics
report the missing action. Rebuild the local library to see the new poses.

The small `src/scene.js` patch installs the helper, passes observed snapshot
speed/vertical displacement, and exposes read-only canvas diagnostics. It restores
the previous skate overlay **before** sampling paused native air clips, preventing
the previous ground pose from overwriting the first jump frame. Existing terrain
contact, prop sockets, camera, materials and bloom depth paths are retained.
Integration received the hook proposal. A local review patch is retained at
`.local/native-movement-scene.patch`; integration owns merge and runtime rollout.
Integration's separate boot-attachment fix is not included in this branch.
The exact locomotion asset route exists in the selected frozen runtime; its
server source patch remains a separate unpublished integration dependency.
A fresh server checkout is not claimed to serve the expanded local asset.

## Source and transform evidence

Run `scripts/build-chao-locomotion.js` and `scripts/verify-chao-locomotion.js`
after the existing native extraction/conversion. `CHAO_ASSET_ROOT` may point at
another checkout's ignored inputs; outputs stay under the current worktree's
`.local/imports`.

The three-clip library is 116,432 bytes, SHA-256
`c740b9b69e91d38027cd12bdad6613b809c4e113b31d1c9afcd8577a68f8012f`.
Independent verification compares all 360 animation channels/samplers, 55 glTF
nodes/rest hierarchy, 60 geometry accessors and five images against source bytes.
This packaging step selects existing glTF data; it introduces no new native
game-format decoder or gameplay port.

`node scripts/verify-native-movement.mjs` with the same asset root passes 1,890
sampled frames across Neutral/Hero/Dark Chaos, with 15 transitions per form and
native119/132/133/135/136/137 observed. Each form has 31 distinct Jump poses, 21
Fall poses, 21 landing poses and 160 skating frames. Original 60/60/61-node parents
and scales remain intact. The Shadow overlay changes rotations only and preserves
the underlying native positions/scales exactly. Native Idle/Walk scale tracks
retain their original float32 rounding. Source hashes and individual phase
results: `.local/imports/native-movement-runtime-proof.json`.

## Automated and service checks

- All 17 client tests pass: thresholds, malformed/incomplete Shadow samples,
  cycle seam interpolation, invalid render deltas, native air/recovery,
  mount/stale resets, rig preservation and existing save/source boundaries.
- `npm run build` passes. Existing bundle-size and runtime-local texture notices
  remain; original assets are not bundled.
- A checkout physically nested in `.local/` triggers Vite's existing
  `**/.local/**` deny rule even for its public source. The unchanged full suite
  was run from a temporary source copy using the same dependencies; its boundary
  test passes there. No private-path rule was weakened.
- `scripts/verify-native-movement-service.mjs` passes a real HTTP/WebSocket/input/
  snapshot round trip against frozen Rust executable SHA-256
  `51a07f9c382ecb70785654297180349dd2e34f677d5921818ea2b9a6ecec3100`.
  New fixture profiles use a fresh ignored data directory and an OS-assigned
  loopback port. The final run records 56 owner snapshots and 53 observer snapshots:
  Idle/Walk/Run/Jump/Fall/Land, normal Run below the skate threshold, high-speed
  skate eligibility, stopping, actual mounted displacement with an Idle pilot,
  dismount and clean service shutdown. The observer sees owner gear without
  inheriting it. No live credentials, profile or save is read.

To rerun the service proof, set `CHAO_PROOF_SERVER_EXE` to a verified existing
Rust executable and run `node scripts/verify-native-movement-service.mjs`.
This proves that binary's actual protocol, not a build/acceptance claim for later
server changes. Report: `.local/imports/native-movement-service-proof.json`.

## Visual proof

`scripts/capture-native-movement.mjs --assets <local-import-directory>` runs a
separate asset-only loopback fixture using the real `createGarden` renderer and
original local models, shoes, gloves, flame and emeralds. Supply
`--playwright-module <path-to-existing-playwright-entry>` and `--channel chrome`
if those tools are installed outside the repository. No game-service or save
access occurs. Controlled snapshots are explicitly labeled fixtures; the real
Rust round trip is the separate check above.

The final run passes 20 checks across 30 phases and 90 PNGs. Local review starts
at `.local/screenshots/native-movement/index.html`; `proof.json` contains asset
hashes, canvas diagnostics, rig/material measurements and pixel checks. Captures
cover three Chaos forms, Walk/Run/skate thresholds and cadence, stopping,
jump/fall/recovery, immediate skate-to-jump, native mech transitions, remote
ownership and native garden slope contact.

Bodies retain alpha1/opaque depth writing, the flame keeps additive transparency,
and emeralds retain native0.8 alpha. Opaque meshes remain in the emerald bloom
depth pass. The controlled green emerald test measures 0 green pixels behind
the body versus a visible front control, in 44×44 regions centered on the
projected gem. The actual
native sloping triangle is ray-sampled for fixture height and contact checks;
this rendering check does not establish server terrain/physics parity.

Screenshots, native libraries and raw reports remain ignored/local. Public
source contains reproducible tools and this ledger, without embedded game
assets, saves or credentials. Remaining limits include wider slope/turn/foot
sliding playtests, swimming/climbing/flying mechanics, all 298 activity clips,
retail-state parity, later server revisions and a deployed release.

## Integration with client main

The resolved source keeps the `006` and `013` footwear joints, original shoe
effects, bounded downhill skate coast and existing head and shadow presentation.
Repeated same-room snapshot timestamps no longer reset movement history or
extend freshness. A sampled native jump, fall or landing takes precedence over
the fading skate pose. Skating can still coast over a qualifying descending
terrain gap without reporting contact or lighting the shoe jets.

Fresh isolated checks passed the 1,890-frame native rig proof, real Rust HTTP
and WebSocket movement with two clients, and all 20 Chrome rendering checks
across 30 phases. The Chrome proof used supplied snapshots. Positive-gap
downhill behavior was covered by cadence tests and still needs live browser
acceptance. Native sources and capture outputs stayed local.
