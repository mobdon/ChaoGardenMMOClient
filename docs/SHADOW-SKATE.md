# Shadow skating source for the owner's Air Shoes

Updated November 18, 2026. Genuine Shadow motions are extracted and sampled for a rotation-only adaptation to the existing Chao. This does not replace the Chao with Shadow, copy Shadow's limb lengths or grant movement speed. The owner must still have approved footwear; movement and equipment remain server-owned.

## Reproduce and verify

Using the existing project-local SA Tools 786, SAIO2.2.2 and Blender setup:

```powershell
node scripts/extract-shadow-skate.js
.local/tools/blender-5.1.2-windows-x64/blender.exe --background --factory-startup --python scripts/convert-shadow-skate.py
node scripts/verify-shadow-skate.js
```

All extracted, sampled and converted game data stays in ignored `.local/`. Installed archives are copied before splitting; source hashes are checked again by the independent verifier. No original game file, Chao save or Chao rig is modified. The conversion does not even load a Chao model.

| Local output | Contents |
| --- | --- |
| `.local/extracted/shadow-skate/manifest.json` | Original archive/executable hashes, selected motion references and original Shadow animation-state table. |
| `.local/imports/shadow-skate-motion.json` | Runtime-readable source samples, bind references, semantic correspondences and explicit adaptation limits. |
| `.local/imports/shadow-skate-conversion-report.json` | Native clip frame ranges, moving source nodes, rig check and output hash. |
| `.local/imports/shadow-skate-verification.json` | Independent structural/source verification results. |

The narrow asset route is `/api/assets/shadow-skate-motion.json`. It returns only this prepared local JSON; it does not expose arbitrary files, archives or source paths.

## Genuine motion identity

The official [TERIOSMTN mapping](https://github.com/X-Hax/sa_tools/blob/master/GameConfig/PC_SA2/MDL/TERIOSMTN.ini) labels the selected resources. `teriosmtn.prs` contains Shadow-specific clips; `plcommtn.prs` supplies common clips. The source model is native Shadow model65 from `teriosmdl.prs`, with 62 imported bone nodes.

| Source motion ID | Official mapping label | Converted samples | Role |
| --- | --- | --- | --- |
| 18 | Common Brake / `cmn_stop0` | 6 | Brake reference |
| 27 | Common Turn Around / `cmn_stop1` | 8 | Common turn reference; no selected Shadow state points here |
| 169 | Shadow Stand / `sd_wait_a` | 8 | Bind/standing reference |
| 170 | Shadow Alternate Stand / `sd_wait_b` | 8 | Alternate reference |
| 171 | Shadow Speedwalk / `sd_run0` | 31 | Native pre-skate reference |
| 172 | Shadow Skate (Jog Speed) / `sd_run1` | 31 | Genuine skate cycle |
| 173 | Shadow Skate (Run Speed) / `sd_run2` | 32 | Genuine skate cycle |
| 174 | Shadow Skate (Sprint Speed) / `sd_maxrun` | 31 | Genuine skate cycle |
| 175 | Shadow Turn Around / `sd_stop1` | 8 | Shadow turn reference |

Original `AnimationInfo` rows are independently read from `ShadowAnimList` at `0x00A188C8`, 249×16 bytes, identified by [SA2Variables.h](https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Variables.h) and [SA2Structs.h](https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Structs.h). State7 selects motion171; state8 selects172; state9 selects173; state10 selects174. State27 selects175. Preserve the distinction between a state index and a resource motion ID.

The original state rows retain motion mode, next state, transition rate and animation speed. States8/9/10 contain rates0.12/0.15/0.20, whereas direct states172/173/174 contain1.4. Consuming movement-dependent timing is not fully traced. **60Hz is the conversion reference sampling rate, not a claim that every native clip always plays at that rate.** The renderer's speed threshold, blending, cycle choice and cadence are explicit adaptations.

## Data and retarget contract

The JSON has nine complete sampled clips, all 62 source-bone world rotations/positions and a 16-bone semantic subset for runtime use. Quaternions are normalized `xyzw`, sign-continuous within a clip, in **right-handed glTF Y-up model space before the loader's handedness/root conversion**. Per clip: `nativeId`, `name`, `role`, `fps`, `frameCount`, `durationSeconds`, `nativeStates`, and `frames`. Each frame has `timeSeconds`, semantic `rotations`/`positions`, plus `rotationsByNode`/`positionsByNode` for the complete original hierarchy. `bones` and `sourceBones` include each node's `restWorld` orientation, `restPosition` and parent ID.

| Semantic subset | Native source nodes |
| --- | --- |
| Body, head | 4,44 |
| Left upper arm, forearm, hand | 37,38,40 |
| Right upper arm, forearm, hand | 56,57,59 |
| Right thigh, shin, heel, toe | 7,9,13,14 |
| Left thigh, shin, heel, toe | 17,19,23,24 |

These semantic names are retarget correspondences derived from the native hierarchy and bind-space limb positions. They are not invented names claimed to occur in the original bone data. Shadow's nominal forward is−X, checked against native heel/toe direction, with up+Y. Aligning it with Chao forward+Z uses a+90° Y rotation, supplied as `alignmentQuaternion`.

For source sampled orientation `S`, source bind orientation `R` and alignment `A`, compute `D = A × S × inverse(R) × inverse(A)`. Apply `D` to the captured target Chao bind orientation. Resolve the desired target model orientation against its already-animated parent to obtain the local rotation. Keep the original target positions, scales, hierarchy, mesh, weights and rest pose. Source positions provide audit/trajectory evidence; **never assign them as Chao bone translations**.

After alignment, Shadow's right heel13 corresponds to the Chao's negative-X foot side (node013); left heel23 corresponds to the positive-X side (node006). Left/right native prop labels and the numeric hierarchy must be verified together when inspecting actual boot contact; filenames alone are insufficient.

## Runtime boundary and acceptance

Keep ordinary Chao walking/running below the chosen speed threshold. Blend into the genuine Shadow-derived skating cycle only for the authorized owner wearing Shadow Air Shoes, moving on foot and grounded. Derive speed and cadence from authoritative snapshots. Idle, airborne and mounted states retain their existing native Chao/vehicle actions. Native clips cannot grant velocity, collision, acceleration, flight, ownership or rewards.

Data verification checks all original hashes, native intermediate hashes, nine clip ranges, all quaternion norms, sign continuity, finite world positions, complete source hierarchy and actual left/right foot excursion in each skating cycle. Source rig matrices remain unchanged.

Browser acceptance must still inspect threshold crossing and hysteresis, slow/fast cadence, starts/stops and turns, left/right boot contact, arm/hand props, eye/flame/emerald attachments, return to idle/air/mech actions, two-client ownership presentation and unchanged Chao proportions. The different anatomy means retarget amplitude limits and pose blending are adaptations requiring visual judgment. This source-data audit does not certify them by itself.

## Integrated presentation and transform proof

Ordinary Chao running now has a separate genuine source asset for the requested Walk → Run → Skate sequence. `scripts/build-chao-locomotion.js` selects only `alm_run` from the verified activity library and compacts unused animation buffers into `.local/imports/chao-locomotion.glb` (67,812 bytes), served by the narrow `/api/assets/chao-locomotion.glb` route. Native state119 identifies `animation_002D8310`, `common/alm_run.saanim`, `LoopProperty=1`, `EndFrame=5` and `PlaySpeed=0.21`. It is not the distinct `alm_run_stopped` clip.

`scripts/verify-chao-locomotion.js` independently confirms all 120 animation samplers/channels for 40 native nodes, 60 geometry accessors and five embedded images remain byte-identical to the source, with all 55 glTF nodes and original rest/hierarchy retained. The output SHA-256 is `3d74428d1236fd331a41b85de993123274f6348b006b3bd27e554bf114ae07bc`. This preserves the original source data; binding its rotations and bind-relative translations onto the existing Chaos skeleton is separate runtime adaptation and must preserve the target's own proportions.

The runtime Run helper applies native state119's base rate `0.21 × visualSpeed / 1.2`, with its existing rate bounds, rather than playing the short source cycle at raw conversion speed. Ordinary Walk/Run cadence uses a 0.24-second exponential filter of observed server speed. Raw speed still chooses the gait: enter Run at 1.05 world units/second and exit below 0.95. Stale, airborne and mounted state clear that visual filter. The demonstration pace inputs remain Walk 0.4 (0.798 units/second), Run 0.62 (1.236, below the skating exit threshold) and Auto 1 (1.994); the filter never changes input or authoritative velocity. `scripts/verify-chao-run.mjs` passes across all three native Chaos forms with 21 distinct poses per form and 80 position/quaternion channels; it preserves target scales and parent relationships. Ignored evidence: `.local/imports/chao-run-runtime-proof.json`.

The separate `src/shadow-skate.js` helper enters skating at raw observed speed 1.55 world units/second and exits below 1.30; the present Rust dry-ground maximum is 1.994. The whole-body overlay eases in over 0.42 seconds and out over 0.28 seconds. A separate 0.32-second exponential speed filter builds the stroke cadence and continuously blends motions172/173 below visual speed1.8, then173/174 above it. Cadence reads the original state8/9/10 rates0.12/0.15/0.20 from the motion metadata and ramps from zero as visual speed rises. Phase stays continuous across speed changes, including last-to-first quaternion interpolation. Applying those native rates to this MMO's speed bands is an explicit presentation adaptation; the retail movement-dependent timing remains untraced. The former `speed / 1.8` timing is historical and is no longer the runtime contract.

To reduce the user's reported excessive head bob, the helper retains35% of the source head rotation before an18° cap, and caps torso rotation at24° while preserving the skating lean. Upper arms/hands retain75° caps and feet50°; their strokes are unchanged by this refinement. Native Shadow sprint-head samples contain about42–47° pitch and up to25° roll, whose full amplitude exaggerates the Chao's larger head. These are anatomy adaptations, not edits to source clips, native positions, scales, hierarchy or proportions. Native Chao Idle/Walk/Run remains underneath. Airborne, mounted, swimming, missing-footwear, unavailable-motion and invalid/stale states clear the overlay, filtered speed and phase immediately. Snapshots older than350ms and implausible teleport displacement cannot provide skating speed.

`scripts/verify-shadow-skate-runtime.mjs` produces ignored `.local/imports/shadow-skate-runtime-proof.json`. The updated CPU proof passes648 sampled frames across Neutral/Hero/Dark Chaos; original local positions, scales and parent relationships remain unchanged for all60/60/61 loaded rig nodes. It measures the overlay's rotation delta on actual feet006/013, excluding underlying Walk and the static005/012 pivots, and records160 distinct overlay foot-pose signatures per form. Peak full-overlay rotation in that sample is24.000003° body and16.110564° head. The eight focused movement/skating tests also cover eased entry/exit, native metadata timing, continuous phase, invalid frame time, immediate blocked exits and anatomy bounds. The complete client checkpoint passed20 tests and built successfully; later gear/UI changes require their final combined check. This is source/CPU evidence; the new slower cadence, reduced head bob and updated attachments still require actual browser acceptance.

## Historical live Walk → Run → Skate demonstration

The earlier November18,2026 live preview was driven with ordinary browser key input, selecting Walk, Run and Auto pace in three real takes. Read-only canvas diagnostics reported `Walk` at0.798 units/s, `NativeRun119` at1.236 with rate0.216, and the former `ShadowSkate174` cadence1.108 at1.994 units/s and blend1. These recordings predate the slower native-rate mapping, head stabilization and exact-mirror gear follow-up above; they do not accept those new changes. The owner remained on foot with native Air Shoes, Hammer Gloves and the original Chao rig. Idle and airborne fallback were checked. New browser checks must cover the complete updated transition, head movement, glove contact, mirrored boots, swimming, mounted transitions and two-client presentation.

All182 captured1848×1252 PNG frames and phase-local timing are retained under ignored `.local/screenshots/movement-clip/`, with `frames.json` recording phase and diagnostic proof. The7.73-second `.local/screenshots/chao-walk-run-skate.mp4` export is1280×868 and preserves the source aspect ratio; the matching GIF is720×488. Exporting these recorded frames trims gaps between the three takes without fabricating animation or claiming one continuous input take. The live profile remains the same canonical138-ring Chao.
