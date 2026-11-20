# Chao resident behaviour presentation

The server runs a port of GameCube SA2B `ChaoMain.rel` behaviour control and
main.dol `MotionControl` for opted-in garden residents (`src/behaviour.rs` in
ChaoGardenMMOServer). Each snapshot's `watch.behaviour` carries the native
behaviour function (`fn`, `address`), the native motion state (`motion.num`
0..621, `frame`, `spd`, and `link` while blending) and the native face numbers
(`face.eye`, `face.mouth`). It is null when the server has no local native
data; residents then use locomotion as before.

## Client side

- `src/chao-behaviour.js` plays the clip for `motion.num` from the local
  `chao-behaviour.glb` at the published frame, and extrapolates between
  snapshots at 59.94 frames per second using the state's own table row
  (LOOP wraps, BACK ping-pongs, other modes clamp). The next snapshot corrects
  it, and extrapolation stops after 30 frames.
- A link shows both poses frozen, the source at its frame and the target at its
  start frame, weighted by the native link ratio.
- Faces go to the appearance module's `setChaoFace(root, { eye, mouth })` via
  `registerChaoFaceApi`. A null number keeps that part's default.
- Nothing is chosen on the client. Mounted, swimming or stale Chao, and states
  without a local clip, fall back to locomotion.

## Building the local clip library

The builder keeps every native motion state in the local manifest whose clip
is in `chao-actions.glb`, keyed by state number, with its table row. No state
is chosen by hand. It checks each clip's source hash and keeps the original
hierarchy and accessor bytes:

```powershell
node scripts/build-chao-behaviour.js
```

Output stays local in `.local/imports/chao-behaviour.glb` with a proof JSON
that lists any state without a clip. The server serves it at
`/api/assets/chao-behaviour.glb`.

## Limits

- Movement speed while walking is the server's existing resident pace, because
  native Chao acceleration is not traced; the walk clip plays at its native
  speed.
- Mouths draw once the appearance work (client PR #431) merges.
- The steady-head policy (VIS-14) still applies during behaviour clips.
