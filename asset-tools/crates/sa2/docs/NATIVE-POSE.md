# Native pose evaluation

`pose.rs` evaluates source model hierarchies and supported SAANIM transform channels in Rust. The matrices retain native axes, units, hierarchy and scale; no Chao retargeting, body edits, skinning, eye removal, root normalization or browser/world scaling is applied.

The implementation is pinned to the independently executable local conversion reference: SAIO 2.2.2's **SA3D.Modeling 1.2.2**, informational commit **f23c73df68c0c69a96f28746c41b6fc32643340a**. This is a source-backed native-format/runtime slice. Agreement with that conversion library does not establish complete retail executable or gameplay fidelity.

## Matrix and hierarchy contract

`Matrix4 = [[f64; 4]; 4]` stores rows and operates on column vectors. Translation occupies `[0][3]`, `[1][3]`, `[2][3]`; `world = parent * local`. `transform_point` includes translation; `transform_vector` excludes it. Consumers must use a proper inverse-transpose rule for normals when scale is nonuniform.

`evaluate_nodes(&[Node])` returns source child-first, sibling-second order, even if the supplied slice is shuffled. It validates one connected root chain, unique nonzero offsets, complete references, consistent parents, cycles, finite transforms and a 65,536-node limit. Hierarchy traversal is iterative. Overflowing world matrices fail explicitly.

Default Euler composition is `T * Rz * Ry * Rx * S`. In this pinned SA2 reference, the attribute named `RotateZYX` (`0x20`) uses `T * Ry * Rx * Rz * S`, corresponding to Z–X–Y application order. The name alone must not be used to infer the order: later upstream code changed this branch.

The low source attributes remain in every evaluated node. As the verified reference does, evaluation preserves authored transform values when `NoPosition`, `NoRotation` or `NoScale` hints are set. Some native Chao nodes retain small nonzero coordinates under those hints. Zeroing those coordinates would fail reference parity. `SkipDraw`, `SkipChildren`, `NoMorph` and `Clip` remain source metadata: this evaluator produces transforms, without camera culling or draw traversal. `NoAnimate` (`0x40`) excludes a node from motion-part indexing while retaining it and its children in the hierarchy. Quaternion rotation, cached rotation, modifier, envelope and unknown attribute bits are rejected.

## Supported motion sampling

`evaluate_model_pose(&Model, Option<&Motion>, frame)` returns local/world matrices plus sampled position, Euler radians, scale and optional motion-part index. Rest poses report frame zero. Motion poses report the effective nonnegative requested frame and the source frame count; the CLI can separately retain the requested frame in provenance.

- Only linear position, Euler rotation and scale channels are supported. Reserved interpolation bits, spline/user modes, morph/quaternion/camera/light channels, inconsistent descriptors and mismatched animatable-node counts fail explicitly.
- Missing keys retain the node's authored value. One-key channels remain constant. Negative time clamps to zero, and each channel clamps before its first and after its last key. There is no implicit looping or frame-rate assumption.
- Source key order must be strictly increasing, with bounded frame indices and at most two million total keys. The evaluator does not silently sort malformed input or synthesize absent animation.
- The reference distinguishes ordinary node BAMS (65,536 units per turn) from SAANIM's signed full-range **BAMSF** representation (65,535 units per turn for both supported rotation widths). Stored Euler components interpolate directly; shortest-arc wrapping would change intentional full-turn keys.

Calculations use `f64`; the reference uses `System.Numerics` single precision. Comparison therefore uses a documented numerical tolerance instead of asserting bit-identical matrices. Geometry, source hashes, animation timing metadata and original source values remain unchanged.

## Evidence and remaining limits

Synthetic unit tests cover parent transforms and units, noncommuting rotation orders, the BAMS/BAMSF distinction, no shortest-arc wrapping, endpoint clamping, `NoAnimate` indexing, malformed trees, unknown flags, unsupported interpolation and non-finite input. Independent local pose parity is exercised by `tests/native_runtime_local.rs` and its ignored SAIO reference report; see that test's evidence instructions for exact sample counts and tolerances.

The November 18, 2026 local comparison passed for all seven original Chao models: child, Neutral/Hero/Dark Chaos, Tails, Knuckles and Amy. It compared rest and sampled Idle/Walk poses, including negative, fractional and beyond-end requests: **133 poses and 5,320 node transforms**. Maximum absolute local/world matrix element error was **0.0000006108879056**, below the fixed **0.00002** tolerance. The final generated independent reference report had SHA-256 `3423735fc9fc29737567344193d7be566dc8e7bc2aee6620412a9d4214a27df0`; its bundled SA3D DLL SHA-256 was `c013cf4fb37732a211b212694856cfc6b1262a2446683d2394d7585c175cd5a8`. Source inputs were rehashed unchanged by the oracle and checked again by the Rust test. These report hashes identify local evidence and do not embed any source asset contents.

This slice does not implement weighted vertex deformation, a native renderer, motion blending/state transitions, spline interpolation, quaternion or morph animation, facial/evolution controllers, retail animation scheduling or any authoritative game mechanic. It does not replace the current live browser presentation or roll out a runtime migration.

## Pinned primary references

- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/Structs/MatrixUtilities.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/Structs/BAMSFHelper.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/ObjectData/Node.Transforms.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/ObjectData/Node.Enumerate.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/ObjectData/Enums/NodeAttributes.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/Animation/Motion.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/Animation/Keyframes.cs`
- `https://github.com/X-Hax/SA3D.Modeling/blob/f23c73df68c0c69a96f28746c41b6fc32643340a/src/SA3D.Modeling/Animation/Utilities/KeyframeInterpolate.cs`
