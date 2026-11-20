# SA2 + BATTLE local Rust asset tools

Standalone, unpublished Rust crate for owner-local Steam PC resources and the
verified SA Tools 786 intermediates. No common workspace/core or runtime files
are changed. This ports asset readers and selected conversions; it does not port
the retail game engine, Chao simulation, missions or device transfer protocol.

The CLI emits native structured JSON, raw archive entries, lossless RGBA PNGs and
native-rate PCM WAVs. A Rust GLB exporter also handles the verified static Chunk
subset (seven cinematic emeralds, the neutral garden palm and five City Escape boards). Other models
still use the existing SA Tools/Blender pipeline.

The full SA2 + BATTLE Rust program is tracked in
[GitHub issue 11](https://github.com/mobdon/ChaoGardenMMOClient/issues/11) and
[FULL-GAME-PORT.md](docs/FULL-GAME-PORT.md). This crate is its resource/evaluation
foundation; the full game is not yet ported.

The [City Escape board guide](docs/CITY-BOARD.md) has commands for the original board GLB, board animation poses, static ground under Sonic�s spawn, and object replay.

The [campaign and stage guide](docs/CAMPAIGN-ENTRY.md) shows how to sample a
native opening camera and open City Escape from Hero Story using a local Steam
installation. It includes the installed-data checks and exact scope of the offline host.

```powershell
# Use the original checkout's already prepared Rust session, or a normal Rust toolchain.
. C:/path/to/original/client/scripts/use-rust.ps1
cargo test --locked --manifest-path asset-tools/crates/sa2/Cargo.toml
cargo run --locked --manifest-path asset-tools/crates/sa2/Cargo.toml -- --help
```

For repeated commands, build once and invoke the resulting `sa2-assets` binary.
Commands take explicit inputs and new output paths; every output must be under
the nearest checkout's ignored `.local` directory. Existing outputs are refused.
Archive names are metadata only: extracted files use numeric names, preventing
archive paths from being interpreted as filesystem paths. Installed inputs are
opened read-only; no saves or installed archives are modified.

```powershell
$game = 'path/to/Steam/steamapps/common/Sonic Adventure 2'
$tool = 'asset-tools/crates/sa2/target/debug/sa2-assets.exe'
& $tool catalog $game .local/sa2-rust/catalog.json
& $tool --source-root $game extract "$game/resource/gd_PC/al_icon.prs" .local/sa2-rust/icons
& $tool texture .local/sa2-rust/icons/00000.gvr .local/sa2-rust/icon.png
& $tool --source-root $game adx "$game/resource/gd_PC/ADX/chao_hall.adx" .local/sa2-rust/hall.wav
& $tool inspect path/to/local/al_ncz.sa2mdl .local/sa2-rust/child.json
& $tool inspect path/to/local/alm_tati.saanim .local/sa2-rust/idle.json
& $tool inspect path/to/local/objLandTableNeut.sa2blvl .local/sa2-rust/garden.json
& $tool transporter path/to/local/PC_SA2/ChaoCommon.ini .local/sa2-rust/transporter.json
& $tool validate .local/sa2-rust/hall.wav.manifest.json .local/sa2-rust/validation.json
```

`prs INPUT OUTPUT` explicitly decompresses PRS to a bounded local binary.
`extract` accepts PRS-wrapped GVM, direct GVM, PAK or AFS and writes a manifest
alongside exact entry bytes; texture/audio conversion is explicit. For external
CI4/CI8 palettes use `texture INPUT OUTPUT.png PALETTE.gvp BANK`. The bank is
relative to the first palette color; its source span/hash and entry count are
recorded. Supplying an unused palette is an error.

Use `--source-root INSTALL` before a conversion command to retain the same
installation-relative source identity as `catalog`. Without it, a single-file
conversion uses its basename as its logical source identity. Consumers must key
provenance by logical identity **and exact source SHA-256**, and include the
output SHA-256 when identifying a generated result with a chosen frame, scale or
query. Native intermediate offsets belong to
that intermediate; a PRS-decoded offset is not a physical compressed-file offset.

Each conversion gets a `.manifest.json` sidecar with full input hashes, bounded
span identities, output hashes and independent representation/fidelity states.
The schema is a proposal scoped to SA2, not an agreed shared API. `validate`
checks structure and relationships; it explicitly reports `contentVerified:false`
because it does not reopen and rehash source/output files. See
[schema proposal](docs/SCHEMA-PROPOSAL.md) and [schema](manifest.schema.json).

Inputs are capped at 512 MiB per direct conversion, PRS output at 128 MiB and
audio PCM at 128 MiB. Native decoded geometry/key storage and total labels have
separate aggregate limits. The installation catalog streams files in 64 KiB
chunks. Reparse/link outputs are refused; these offline tools do not provide
race-proof directory-handle isolation against another process replacing paths.

Native evaluation/query commands now build on those readers:

```powershell
& $tool pose path/to/local/al_ncz.sa2mdl .local/sa2-rust/rest-pose.json
& $tool pose path/to/local/al_ncz.sa2mdl .local/sa2-rust/walk-pose.json path/to/local/alm_aruki_ncz.saanim 3.5
& $tool collision path/to/local/arena.sa2blvl .local/sa2-rust/collision.json 0.1
& $tool raycast path/to/local/arena.sa2blvl .local/sa2-rust/hit.json .local/sa2-rust/ray-request.json
```

`pose` evaluates source-native units and hierarchy at an explicit native frame,
without animation retargeting or implicit FPS/wrap assumptions. Unsupported
interpolation/morph/quaternion variants fail explicitly. Its independent local
oracle is pinned SA3D/SAIO, not a completed retail animation engine comparison.

`collision` flattens supported original Basic collision polygons with their
source flags and node transforms. Scale defaults to 1 (native units); the example's
0.1 is an explicit world-coordinate adaptation, recorded in the output.
`raycast` consumes a typed request such as
`{"origin":[0,100,0],"direction":[0,-1,0],"maxDistance":250,"uniformScale":0.1}`.
Directions are normalized; distance is in the selected world units. The default
query is two-sided against solid, static source surfaces. This is geometric
query support, not native collision response, navigation or a gameplay controller.
Pose motion/query dependencies receive exact input hashes in the local sidecar.
See [pose](docs/NATIVE-POSE.md), [collision](docs/NATIVE-COLLISION.md) and
[independent parity](docs/NATIVE-RUNTIME-PARITY.md) for supported variants and
precise reference limits.

After generating the independent ignored pose oracle as described in the parity
document, run the new checks with
`./asset-tools/crates/sa2/verify-native-runtime.ps1 -ReferenceRoot path/to/original/client -PoseReference .local/native-pose-reference-v2.json`.
The earlier `verify-local.ps1` retains the reader/conversion baseline checks;
the native-runtime verifier adds pose/geometry/query evidence without claiming
those checks execute the canonical gameplay service.

Run all source-backed parity checks with prepared ignored references:

```powershell
./asset-tools/crates/sa2/verify-local.ps1 -GamePath $game -ReferenceRoot 'path/to/original/client'
```

These tests are opt-in and fail if required references are absent. Normal tests
contain synthetic headers, malformed streams and CLI fixtures only. No original
asset bytes, art, audio, ROMs, saves, downloaded tools, generated JSON or converted
outputs belong in Git. The detailed logs stay under `.local/sa2-parity`.

Coverage, parity and limitations are recorded in [native formats](docs/native-formats.md),
[textures/archives](docs/TEXTURE-ARCHIVE-REFERENCES.md), [audio/PE](docs/audio-pe.md)
and [delivery evidence](docs/VERIFICATION.md). Finding BATTLE resources does not
verify entitlement. Models/textures decoded successfully do not establish native
gameplay parity.

For `kart-contact-step`, optional `tail` (requires `result` and `wall`) continues CartD through return at 0x3020. Supply `post_alignment_angles`, `observed_local_velocity` from 0x5498, `observed_raw_magnitude` from the engine, and signed `work_d4`. For work_70 slots 0 and 1, supply `ring` with the matching slot, cursor (0–59), and 60 triples each of velocities and rotations; omit it for negative slots. Reuse the returned ring on the next tick. Copy response.tail.motion_history into drift.motion_history for kart-drive-input: these are the newest 30 raw velocity samples, after the write, not transformed velocities. The engine query, alignment and matrix math are not implemented by this callback comparison.

### AHX voice conversion

Use `sa2-assets ahx INPUT.ahx .local/voice.wav` for extracted, unencrypted mono voices. The converter reconstructs Layer II frames, keeps decoder state across frames and trims the float WAV to the source sample count. The conversion manifest records the source hash.

A local comparison decoded every installed event-bank entry separately with the Rust release binary and vgmstream-cli (`-W 4`). Every sample count matched the AHX header and the reference WAV. Waveform comparison used corresponding samples without alignment shifts or normalization.

| Bank | Clips | Mono samples | Clips with reference peaks outside ±1 | Samples outside ±1 | Largest absolute error against original float reference |
| --- | ---: | ---: | ---: | ---: | ---: |
| event_adx.afs | 2,727 | 208,701,648 | 1,629 | 87,762 | 0.000001565 |
| event_adx_e.afs | 2,727 | 198,635,282 | 1,184 | 47,736 | 0.000001193 |

Float output preserves peaks outside the unit range. This comparison covers offline voice conversion only; playback, cue selection and mixing require separate verification.
