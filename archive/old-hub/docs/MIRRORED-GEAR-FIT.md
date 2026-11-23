# Mirrored boots and native palm attachments

The shoe pair now uses **one original right boot**, mirrored rigidly for the
left side. SA2's original left/right meshes and individual native-foot fits
were not exact reflections. The user requested exact visual symmetry, so the
pair deliberately shares the native right geometry, textures, proportions and
one uniform scale magnitude. The native curved sole is unchanged.

`shadowShoeMountRotation()` restores the original right heel13 up and projected
heel-to-toe direction from the source conversion metadata. The right prop is
fitted once against the captured native right-foot bounds, preserving the
approved `CHAO_SHOE_LENGTH_RATIO = .90`. In Babylon row-vector convention:

```text
rightRest = uniformScale * nativeStandingRotation * rightPlacement
leftRest  = rightRest * reflectActorX
local     = desiredRest * inverse(nativeSocketRest)
```

Each boot remains attached to its own native animated foot, 006/013. The left
prop has reflected handedness and uniform absolute scale, including its native
jet geometry. No Chao joint, per-frame counterrotation, sole flattening or
nonuniform deformation is introduced. Exact symmetry applies to rest placement;
the native alternating gait continues to articulate the feet independently.

Both props therefore use original right model 78 and its native jet models
96–99. The runtime left side is a **mirrored attachment adaptation**, not original
left model 79 or jet models 92–95. The conversion tooling still preserves all
eight source effects. The head flame, emerald alpha and native-effect timing
limitations remain as documented in `NATIVE-SHOE-EFFECTS.md`.

Gloves now follow the actual visible palm frame: native nodes 003/010. The old
004/011 leaf nodes have no visible hand mesh and rotate independently during
the Shadow 172–174 overlay, causing their gloves to separate from the palm.
Those leaf nodes remain useful as bind-pose direction markers for the same
distal-palm fit; they no longer parent the glove. The original bind placement,
native joints and rigid uniform scale are retained.

## Reproduce the local proofs

```powershell
node scripts/verify-chao-footwear.mjs --output .local/reviews/mirrored-shoe-proof.json
node scripts/verify-native-glove-contact.mjs
```

Both accept `--asset-root`; footwear also accepts `--shoe-root` and
`--source-root`. The previous `verify-straight-shoe-basis.mjs` entry point now
delegates to the actual mirrored fit proof rather than checking obsolete
separate left/right production bases.

Across Neutral/Hero/Dark Chaos, all 207 boot vertices and 48 native jet vertices
per side match their mirrored partners at rest. Maximum discrepancy is
3.73e-9 world units. The source up direction is retained; projected standing
toe yaw is below 0.000001 degrees. Walk/Run alternating lift and native skate
motion preserve the original hierarchy, positions and proportions.

The glove proof loads both original glove props and executes the actual scene
capture/mount functions on all three native rigs through 2,166 Walk/Run/Skate
samples. Maximum palm-contact drift is 3.44e-8 world units; the old tip sockets
reproduce up to 0.04654 world units of drift. The glove-to-palm relative matrix
remains constant to float32 precision.

These are native-asset and transform proofs. Browser silhouette, contact under
all terrain conditions and canonical service validation remain separate
integration checks. No asset bytes, source joints, server, save or credential
are changed by these scripts.
