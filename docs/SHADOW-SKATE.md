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

The separate `src/shadow-skate.js` helper now implements the adaptation. It enters skating at 1.55 world units/second and exits below 1.30; the present Rust dry-ground maximum is 1.994. It blends over 0.18 seconds. Cadence follows authoritative snapshot displacement divided by 1.8, bounded to 0.35–2 while moving. Motions 172/173 blend below 1.8; 173/174 blend above it, with last-to-first quaternion interpolation across the cycle boundary.

To fit the different anatomy, the helper caps rotation deltas at 30° for the body, 35° for the head, 75° for upper arms/hands and 50° for feet. These are deliberate retarget limits rather than changes to original source clips, which can have much larger bind-relative rotations. Ordinary native Chao Idle/Walk remains underneath, with skating disabled for idle, airborne, mounted and invalid/stale movement states. Snapshots older than 350 ms and implausible teleport displacement cannot provide skating speed.

`scripts/verify-shadow-skate-runtime.mjs` produces ignored `.local/imports/shadow-skate-runtime-proof.json`. The current CPU proof passes 648 sampled frames across Neutral/Hero/Dark Chaos; original local positions, scales and parent relationships remain unchanged for all 60/60/61 loaded rig nodes. It records 150 distinct foot-pose signatures per form and retains both original Idle/Walk clips. This verifies transforms and eligibility logic; live browser inspection remains the visual acceptance step.
