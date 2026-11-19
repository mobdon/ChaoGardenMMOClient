# Offline ASOE soundtrack preview

`cargo run --example mix -- PRIVATE_ROM 12 300 NEW.wav` creates a stereo,
16-bit WAV without overwriting an existing file. Cue 12 is Neo Green Hill Act
1, 13 is Act 2, and 102 is the ring sound. The Rust API returns buffered
stereo samples; no emulator, audio device, network, or battery file is needed.
There is no in-game audio device integration here. Output is derived game
audio: keep it local. Duration is 1–3600 driver frames (176 sample pairs each,
10512 Hz). The render starts the cue in isolation and stops at the requested
frame; it does not jump randomly into a running song.

## Independent retail evidence

ASOE SHA-256 afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020.
The comparison used the verified mGBA libretro core and boot input sequence
from `tools/retail-replay.py`. The emulator was used for measurement only.
At zero-based boot frame 1546, the stage music player selected header
0x0879b204. At 1547 the retail mixer at EWRAM 0x02001400 reported reverb 0,
five Direct Sound slots, master volume 15, frequency option 3, 176 samples
per update, and sample rate 10512. Two active PCM slots pointed to
0x0872b784 (key 55, gate 6) and 0x086e03f8 (key 67, gate 6). PSG square 2
played key 36 (gate 7) and noise played key 84 (gate 12). These match the
Rust score opening after resolving the drum bank, including the two specific
sample pointers. The retail PCM envelopes already read 255 at this update.

For an output comparison, capture the libretro interleaved i16 audio callback
for boot frames 1540 through 1869 under those same inputs. In the first 220
Rust updates of cue 12, stereo RMS per update correlated **0.807** with the
retail RMS at an eight-update offset. The captured callback averages 1097.25
stereo samples per update. Sampling its stereo mean at 176 equally spaced
positions per update and comparing the first 38,720 Rust samples gave a
best positive waveform correlation **0.303** at offset 1429 (searched the
first 30 updates). Those are approximate agreement in note timing and
energy, and clear disagreement at waveform level. They are not a parity
claim. The audio capture and WAV are intentionally not distributed.

The native reference is SAT-R/sa2 revision
2dd3f17d30cd3c156e508aac0017620ae485b9ba, specifically
`src/lib/m4a/m4a.c`, `asm/m4a0.s` (driver assembly),
`src/lib/m4a/m4a_tables.c`, and
`src/platform/shared/audio/m4a_sound_mixer.c`. Native score and instrument
data come from the verified local ROM, not the reference repository.

## Scope and limits

The renderer follows per-track control events, waits, ties, tempo accumulation,
voice priority, Direct Sound envelopes, PCM loops, fixed-rate samples, bank
splits and drum key/pan overrides. The stage PSG voices use their original
square duty, 32-nibble wave RAM or noise polynomial. Synthesis approximates
PSG envelope and phase, pitch table rounding, pan/routing, gain, DAC and
resampling. PCM mixing uses floating point, fixed preview headroom and a
bounded accumulator; it does not emulate FIFO/DMA timing, the PSG high-pass
response, external player fades, another playing cue, note echo, or reverb.
Unsupported cue IDs, tone types/lengths, encodings and extended mixer
commands return an error. It is not a general MP2K engine.

The private-ROM test renders 3600 updates for each act and 120 for the ring.
Act 1 starts 653 PCM, 169 square-2, 11 wave and 118 noise notes; Act 2 starts
809, 165, 32 and 317 respectively. Both peak at seven active voices. The
ring starts one PCM note and becomes silent. These counts are Rust renderer
observations, not independent retail note-count or Act 2/ring waveform parity.
Run `cargo test --test mixer -- --include-ignored --nocapture` with
`SONIC_ADVANCE_ASOE_ROM` set to the verified private ROM.
