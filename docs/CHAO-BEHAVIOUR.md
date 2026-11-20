# Chao resident behaviour presentation

The server (`src/behaviour.rs` in ChaoGardenMMOServer) picks what an opted-in
resident Chao is doing and publishes it as `watch.behaviour`:
`kind` (`idle`, `wander`, `sit`, `sleep`, `play`, `happy`, `startled`),
`variant`, `phase` (`enter`, `loop`, `exit`) and server-time bounds. Weights and
durations are server **placeholder tuning** until the decoded native controller values replace them. The native behaviour controllers and
activity timers are mapped (CAT-LIFE-03) but not decoded, so nothing here
claims native selection logic.

## Client side

- `src/chao-behaviour.js` maps each kind, phase and variant onto a native clip
  from the local `chao-behaviour.glb`. Frames are sampled from server time with
  the clip's original table timing (StartFrame, EndFrame, PlaySpeed at the
  60-frame reference). Enter and exit play once; loop repeats.
- Behaviour never runs while the Chao is moving, mounted, swimming or stale.
  `wander` always uses the existing locomotion. Without the local GLB, or
  with no clip for a kind, the Chao falls back to locomotion idle.
- Faces go through `registerChaoFaceApi({ setChaoFace, resetChaoFace })` from
  the appearance work. `BEHAVIOUR_FACES` holds provisional, unverified
  ChaoEyes/ChaoMouth values; a ported native controller's own values win.

## Building the local clip library

`scripts/chao-behaviour-map.json` records which native motion state presents
each behaviour, with the evidence for the choice. The builder checks every row
against the local manifest (state index, native name) and source hashes, and
keeps the original hierarchy and accessor bytes:

```powershell
node scripts/build-chao-behaviour.js
```

Output stays local in `.local/imports/chao-behaviour.glb` with a proof JSON.
The server serves it at `/api/assets/chao-behaviour.glb`.

## Limits

- The state map is a presentation choice by native clip name and label. It is
  not the native controller's own selection.
- The steady-head policy (VIS-14) still applies during behaviour clips.
- Fruit seeking, toys, social play and voices are not wired yet.
