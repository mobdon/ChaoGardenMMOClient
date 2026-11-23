# A2NE native audio boundary

`native_audio` reads the verified A2NE song table, executes bounded MP2K tracks,
resolves native instrument banks and mixes their source PCM/PSG voices into a
local stereo preview. It covers all fourteen regular-act songs (13â€“26) and
twenty effects/jingles: 102, 112, 113, 115â€“119, 138â€“141, 149, 152, 153, 157,
221, 232, 401 and 407. Pickup routing uses 117 (`se_ring_copy`), with native
gameplay volume/pan supplied by the caller. ID102 contains the same ring score.

The ROM identity remains SHA-256
`f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260`.
The table is at ROM offset `0xAD4F4C`, with 507 eight-byte entries. All addresses
are bounded GBA ROM pointers. No ROM, sequence, PCM or generated WAV is published.

## Reused Rust and source evidence

The sequence/sample reader and preview mixer reuse the existing local Sonic
Advance Rust tooling, adapted to the A2NE verified-ROM boundary:

| Input | SHA-256 before A2NE adaptation |
| --- | --- |
| `sonic-advance/src/music.rs` | `569fcd8204ce008eb8a847fdeb7eee8b27d76e6226cb214fe0166e57912ab34a` |
| `sonic-advance/src/music/mixer.rs` | `ca5cb1717dba7cc7075c4fb9d3eac72482f54f82440db14e5dbcf974c4bc5228` |

The independent source reference is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`. The local raw song-table include
matches Git blob `0146a4b3cdf64f4fa9647fd55bf94ae33f9e2119`. The public JSON
report records exact Git blob SHA-1 and SHA-256 identities for each original
MIDI. `native_audio_evidence.py` independently parses those original MIDI bytes
and compares note tick, key, velocity, duration and voice with actual-ROM Rust
tracks. Native TIE/EOT commands normalize to note durations for this comparison.

All **34,511 notes across 38 cues** agree: complete introductions and the first
native score loop for act music, and complete scores for the one-shots. The
actual ring sample header and all 7,078 signed PCM bytes also receive an
independent offset/header/hash check: header `0xB3B740`, PCM `0xB3B750`,
SHA-256 `ba76332e380eead2131a6b259e471cb0263058bc2f876d7d64fb199cafea9c29`.

The A2NE configuration is `ENGINE_2` (`ENGINE=GAME`), five DirectSound channels,
master volume 15, 10,512Hz with 176 sample pairs per driver update. The mixer
separately allocates one voice for each PSG square, wave and noise channel.
Square1 is supported when native sweep/length parameters are zero. Unknown
commands, encodings, sweep/length parameters, bad pointers, truncated samples,
invalid loop bounds and exhausted instruction/frame budgets return errors.

## Complete cue and loop API

`song` exposes the actual priority, voice bank and track offsets. `trace` emits
bounded score-time commands. `direct_sample` exposes the native PCM header,
data offsets, pitch and optional sample loop without copying source bytes.
`render` renders a requested bounded count of driver frames. `wav` writes an
ordinary stereo signed16 PCM WAV from that preview.

`cue_timing` follows actual GOTO targets and maps score ticks through native
tempo changes to driver frames. Every track of each supported act song agrees
on its score loop. `render_cue` returns `CompleteCue { timing, audio }`:

- Looping music contains the complete introduction and first loop, ending at
  `loop_end_frame`. The caller receives both native loop frame indices.
- One-shots run through all track FINE commands and until the preview's live
  samples/envelopes have finished, within the 10,800-frame bound. FINE denotes
  the sequence boundary; release audio can continue afterward.

| Act song ID | Native loop driver frames, start..end |
| --- | --- |
| 13 | 3..4118 |
| 14 | 75..4243 |
| 15 | 465..3603 |
| 16 | 477..3888 |
| 17 | 575..3775 |
| 18 | 236..4069 |
| 19 | 412..4938 |
| 20 | 0..6624 |
| 21 | 444..4431 |
| 22 | 384..3840 |
| 23 | 0..5424 |
| 24 | 0..5310 |
| 25 | 0..4992 |
| 26 | 0..3960 |

Detailed FINE, rendered duration, polyphony and fingerprint metadata for every
cue is in `catalog/native-audio-evidence.json`. In particular, act clear 401
needs 236 rendered frames (FINE 229), and 1-up 407 needs 241 (FINE 240). A fixed
180-frame effect preview truncates those jingles. Counting 140 needs 5 frames;
count complete 141 needs 121; long brake 232 needs 182.

## Fidelity and host adaptations

Native sequence events and source PCM/PSG data drive this output. No generated
replacement sounds are used. The independent comparison establishes score
decoding and the ring sample bytes. The 45,924,032 mixed PCM bytes and their
hashes are local reproducibility evidence, **not independent retail DAC parity**.

The reused floating-point preview has linear PCM interpolation, driver-frame
envelope approximation, approximate PSG synthesis/LFO/channel arbitration and
fixed output headroom. It does not reproduce GBA PWM/DMA timing, exact mixer
integer rounding, reverb, hardware square sweep or hardware PSG envelopes.
Global song-player interactions, tempo multipliers, native fade/stop commands,
simultaneous music/effect contention and gameplay volume/pan routing are outside
the isolated renderer. A host that independently mixes cue buffers adapts that
native shared-channel behavior. Cross-platform floating-point PCM hashes can
differ; tests assert source score/sample fingerprints and render metadata.

Native score loop points replace arbitrary clip durations. Repeating a buffered
waveform at those points is still a host adaptation: it does not prove
phase-continuous mixing or preserve the continuously running driver's channel
state. `render` executes repeated native GOTOs during a longer requested render;
the current API does not expose a persistent streaming mixer.

## Reproduction

From the combined checkout, set local `CARGO_HOME`, `RUSTUP_HOME` and `A2NE_ROM`
to the existing toolchain and verified ROM. Run:

```powershell
cargo test --locked --manifest-path <worktree>/asset-tools/crates/sonic-advance-2/Cargo.toml --test source_native_audio -- --include-ignored --nocapture
cargo clippy --locked --manifest-path <worktree>/asset-tools/crates/sonic-advance-2/Cargo.toml --lib --test source_native_audio -- -D warnings
python <worktree>/asset-tools/crates/sonic-advance-2/tests/native_audio_evidence.py --references .local/research/a2ne-native-audio --rust-log .local/research/a2ne-native-audio/rust-render-complete.log --rom .local/gba/A2NE-f2b7a3329401.gba --report <worktree>/asset-tools/crates/sonic-advance-2/catalog/native-audio-evidence.json
```

Capture the test's `AUDIO` lines to the specified local log first. Optional
`A2NE_AUDIO_OUTPUT` must point below `.local`; it writes private WAV and decoded
trace JSON files using create-new or identical-existing checks. Original MIDI
inputs must match the pinned Git blobs. The unignored tests reject malformed
tracks, recursion/runaway streams and invalid PCM headers/loops/extents. The
ignored actual-ROM test asserts independently verified score/sample hashes,
complete cue durations, loop metadata, note counts and bounded polyphony.
