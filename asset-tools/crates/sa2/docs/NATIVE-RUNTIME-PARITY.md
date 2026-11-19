# Native pose and collision parity

Verified November 18, 2026. This slice adds executable Rust transform sampling and Basic world-collision queries. The oracle script is validation tooling; it is not a Python replacement for any production reader, converter or game system. No browser/server authority, live preview, installed game, save or canonical Chao state is changed.

## Independent native pose reference

`tests/reference-native-pose.py` delegates model and motion parsing, key interpolation and hierarchy evaluation to the already prepared SAIO 2.2.2 assembly's public APIs: `ModelFile.ReadFromFile`, `AnimationFile.ReadFromFile`, `Keyframes.GetFrameAt`, `Node.UpdateTransforms`, `Node.LocalMatrix` and `Node.GetWorldMatrix`. It does not reimplement the Rust parser or transform algorithms. The assembly is pinned to informational version `1.2.2+f23c73df68c0c69a96f28746c41b6fc32643340a`, SHA-256 `c013cf4fb37732a211b212694856cfc6b1262a2446683d2394d7585c175cd5a8`. The exact version matters: newer source changes the legacy alternate Euler rotation order.

The reference covers the child, Neutral/Hero/Dark Chaos, Tails, Knuckles and Amy models. Each retains its full 40-node hierarchy and original rest transforms and flags. Each model is checked at rest and at nine Idle and nine Walk sample requests, including integer/fractional frames, negative time and a request beyond the last native key. This totals **133 poses and 5,320 evaluated node transforms**. Both local and world matrices, parent relationships, labels and source flags are checked. Two additional synthetic noncommuting rotations distinguish the default and legacy alternate rotation orders.

Rust matches all native references with maximum absolute matrix-element error **0.0000006108879055943817**, below the declared comparison tolerance of 0.00002 native units. The reference assembly computes float32 transforms; Rust retains float64 evaluation. Transposing the reference's row-vector matrices into column-vector representation is the only matrix adapter. No axis conversion, model rescaling, centering, eyelid omission, body-opacity override, limb stretching or retargeting is applied.

Idle has 14 native frames and Walk has 13 in this exact extraction. Negative requests clamp to zero, and channel values clamp to their own first/last key. These tests do not introduce looping, FPS, a blend policy or a native action-state machine. Original transform flags remain metadata exactly as the reference assembly reads them; no extra transform-elision optimization is inferred from the flags.

| Native source | SHA-256 |
| --- | --- |
| `al_ncz.sa2mdl` | `603d6cafc550c70287cf119dc042447bde6feac852485b5926d9bc8b7a851b9a` |
| `al_nchaos.sa2mdl` | `f4efec728a44a7a65524ed40c244bce21383c4912c9e88f5e2235017b3dc21ab` |
| `al_hchaos.sa2mdl` | `3f96bbdb819e8407efb3dc002cddf31bff3daebb0ae5ace3869d2345dabd3aad` |
| `al_dchaos.sa2mdl` | `02274900ae63d0ca650b00da52bb62587efd9a13279dda477423534880f56ebb` |
| `al_tails.sa2mdl` | `76e5ad9ded85f6ecae9ffcbb34a21f6ab04bc969e602ae23b9ae75c67e159495` |
| `al_knuckles.sa2mdl` | `6c6b877bd85f63afeab21ddcc1c7e278b139f1fbb970dfa539a0fcbd16848486` |
| `al_amy.sa2mdl` | `e52023e95267186413571515ce57cd80bf29017ee475957f65aa8a1d4e459643` |
| `alm_tati.saanim` | `d25c5f35a334fe38ba4ca66ec800b5cbddd7d7708ea30ff7ef5713ec919cf005` |
| `alm_aruki_ncz.saanim` | `5bbc1dc9d532b9e4ea1803f18c17ae5d01fffe4f7b42f2b37155e9708b19a995` |

The source-extracted intermediate files have their own address spaces. Their provenance back to the installed DLL remains in the existing extraction records; the tests do not invent an installed DLL span for relocated model or animation bytes. Sources are reread and compared after evaluation.

## Independent world collision reference

`tests/native_runtime_local.rs` compares Rust collision baking to the existing original Blender/SAIO outputs `.local/activities/{race-normal,race-hero,race-dark,karate-arena}-collision.json`. Those outputs come from `scripts/convert-chao-activities.py`, which imports native landtables, evaluates Blender object world transforms and records loop-triangle winding. The Rust production path reads the native landtables directly. Its explicit uniform scale 0.1 matches the earlier canonical reference; the native origin and axes remain unchanged.

All **94 collision meshes, 2,547 reference vertices and 2,898 oriented triangles** are checked across the four worlds. Vertex positions are compared in both directions; triangle correspondence permits cyclic corner rotation but rejects reversed winding. The comparison tolerates reference vertex duplication/reordering rather than forcing Blender's buffer representation into production geometry. Mesh counts, bounds and source solid/dynamic flags are also checked. Every source surface remains in the baked world, including invisible collision.

| World | Meshes | Triangles | Stable vertical queries | Nearvertical/edge probes excluded from height comparison | Maximum vertex-coordinate error | Maximum stable height error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Normal race | 31 | 952 | 61 | 3 | 0.000007828457640357556 | 0.000003179044554268984 |
| Hero race | 31 | 952 | 61 | 3 | 0.000007828457640357556 | 0.0000043513090854929715 |
| Dark race | 31 | 952 | 61 | 3 | 0.000007828457640357556 | 0.0000043513090854929715 |
| Karate arena | 1 | 42 | 8 | 2 | 0.00000017398681650959702 | 0.0000000000000021094237467877974 |

The **191 query comparisons** use an independent vertical-height oracle: solve the triangle's two-dimensional x/z projection, then interpolate its plane height. Production uses a general three-dimensional ray algorithm. Default filtering uses solid, non-dynamic surfaces in both paths. The Blender receipts contain float32 transforms rounded to seven decimals, which makes nearvertical or near-edge height intersections poorly conditioned. The comparator therefore reports and excludes 11 such probe heights: the winning reference triangle must have normalized upward normal at least 0.3 and barycentric interior margin at least 0.02. Their geometry and winding still participate in the complete comparison above. Missing source/reference files, absent reference hits, missing meshes or differing geometry fail; none are silently counted as passes.

| Native landtable | Source SHA-256 | Blender reference SHA-256 |
| --- | --- | --- |
| `objLandTableRace.sa2blvl` | `5ba609a3628db6620751b2a728be530433ea37a14904174e8193a0939614ecef` | `b202b9411624b1d13db396d8291631f2217e9579488fdca007102217b3400147` |
| `objLandTableRaceHero.sa2blvl` | `ffd938e791291bab517d1c132aba84391a0a1f645eb154a52928b7c21731d6c2` | `c8c0674c08b2ff5f455130d46c4cc04e778d750c75ddf268d1acc85d07b54555` |
| `objLandTableRaceDark.sa2blvl` | `a20f7a2276e68a3b3f00e1b47da15564f2beeb0e6482da007a30cf7747b9da30` | `7048f2ecfa7d9418abfa5646349f157976f60fab9d1c021ba2eea8ff7b01c4ab` |
| `objLandTableChaoKarate.sa2blvl` | `3cc13fc381ac30ea1f17043aaeb68d9d352162409e709ae701fb956cba5d49cd` | `2821da18460f56f6973ed696546a436f1a0c9ccf1138a4dbb6796c6e638cf400` |

## Reproduction and boundaries

The consolidated Rust checks passed on this slice: **67 ordinary unit, CLI,
collision and robustness tests**, both explicitly enabled native-runtime parity
tests, `cargo fmt --check` and strict all-target Clippy. The eight earlier
reader/conversion parity tests remain baseline evidence at `c6cd348`; those
readers are unchanged by this slice.

An additional actual-source CLI smoke run evaluated the original child model's
Walk motion at frame 3.5 (all 40 nodes), baked the original Karate arena (one
Basic mesh, 42 oriented triangles, explicit scale 0.1), and queried a downward
ray from `[0,100,0]` with direction `[0,-2,0]` and maximum distance 250. The hit
was `[0,0.4,0]` at distance 99.6, with source entry 27 and original flags
`0x40000001`. Output receipts retained the exact source hashes above and motion
SHA-256 `5bbc1dc9d532b9e4ea1803f18c17ae5d01fffe4f7b42f2b37155e9708b19a995`.
The smoke outputs and manifests remain ignored/local. This exercises offline
source-data commands; it is not a gameplay service round trip.

Run from this worktree with the original ignored tools and conversions prepared. Use a fresh oracle output name; the script refuses overwrites and any output outside this worktree's `.local` directory. Its process disables Python bytecode caches beside reference packages. The supplied interpreter was Python 3.13.9.

```powershell
$original = 'path/to/original/client/checkout'
$python = Join-Path $original '.local/tools/blender-5.1.2-windows-x64/5.1/python/bin/python.exe'
$oracle = Join-Path (Get-Location) '.local/native-pose-reference.json'
& $python -B asset-tools/crates/sa2/tests/reference-native-pose.py --reference-root $original --output $oracle
if ($LASTEXITCODE -ne 0) { throw 'Independent pose oracle failed' }
. (Join-Path $original 'scripts/use-rust.ps1')
$env:SA2_REFERENCE_ROOT = $original
$env:SA2_NATIVE_POSE_REFERENCE = $oracle
cargo test --locked --manifest-path asset-tools/crates/sa2/Cargo.toml --test native_runtime_local -- --ignored --nocapture
```

The final recorded oracle SHA-256 is `3423735fc9fc29737567344193d7be566dc8e7bc2aee6620412a9d4214a27df0`; its script SHA-256 is `d4dc79ca2d34cab7eef6b20cbb5920369402ab3e8a8fa1a67f3e2254c7e7185c`. The oracle captures script, assembly and every native input hash, and the Rust test checks those identities before comparing values. Generated oracle JSON and detailed logs stay ignored/local. Only validation source and this evidence summary are published.

This establishes sampled transform and Basic geometry/query parity for the specified corpus. Weighted mesh skinning, native Chao morphs, quaternion/cache/envelope variants, motion blending, full render/shader equivalence, character contact/step/slide/gravity, moving-object collision, broadphase performance and retail race/Karate/gameplay rules remain separate. No canonical service round trip is claimed: this change does not integrate or replace a running game system.
