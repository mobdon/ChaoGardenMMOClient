# Audio, PC image and Transporter readers

`audio.rs` indexes every AFS slot with its original index, byte offset and length. It preserves empty and aliased entries, validates source spans and refuses entries overlapping the index. Optional filename metadata is not used to construct output paths. AFS extraction and AHX decoding are separate operations.

The ADX reader accepts standard type 3, 18-byte, 4-bit frames with version 3/4/5 headers. It preserves sample rate, channels, exact sample count, high-pass cutoff, v4 history and known loop points. The decoder emits channel-interleaved signed 16-bit PCM exactly once, without repeating loops. WAV export uses that original rate and sample count. Caller-provided limits bound aggregate PCM samples across every channel. Unsupported AHX, encrypted ADX, type 2/4 ADX and CSB remain opaque; a shared-looking header does not prove codec compatibility.

Format references inspected November 18, 2026: upstream vgmstream `src/meta/adx.c` and `src/coding/adx_decoder.c` at `https://github.com/vgmstream/vgmstream`. Decoder math is independently implemented. The four locally verified streams use version 3. Version 4/5 parsing is implemented but not covered by installed-source parity in this delivery.

`pe.rs` reads PE32/i386 headers, sections, named exports, ordinals and forwarders as bytes. It never loads or executes the DLL. RVA and VA mapping requires complete file-backed spans; virtual-only tails, ambiguous RVA mappings, truncated tables and invalid ordinals are rejected. Multiple names may legitimately point to the same address. Forwarders are recorded, never resolved. Non-ASCII/oversized export names and PE32+ are unsupported. The format reference is Microsoft's PE/COFF specification: `https://learn.microsoft.com/en-us/windows/win32/debug/pe-format`.

The seven PC Chao model candidates come from the `AL_RootObject` indices already documented in the repository and SA Tools 786 `GameConfig/PC_SA2/Chao_DLL.ini`. A valid model pointer proves neither a decoded mesh nor BATTLE entitlement. Transporter mapping reads all four named GBA-machine sections from a supplied local `PC_SA2/ChaoCommon.ini`, requiring `chunkmodel` and texture list `AL_OBJECT`; mapping addresses retain their original address space and are not mislabeled as file offsets. Button, shutter, casing and base have separate identities. Machine assembly, collision behavior, menu actions, save ordering and device handoff remain unimplemented here.

Six Chao UI archive names are discovery hints from `scripts/extract-chao-ui.js`: common, Chao, English entrance, English Transporter menu, English cave menu and common texture archives. Actual sprite roles require decoded-entry evidence. The prior selected `level00`, `ad_22ascii` and `add_parameter_race_en` images are original atlases; the race-entry parameter image is not a garden stat-window implementation.

## Local parity recorded November 18, 2026

The installed originals were read only. Every compared source was read again and unchanged after the operation. Native inputs, generated WAV data, detailed reports and prior converter outputs stay ignored/local.

The four Rust PCM buffers exactly equal the `data` chunks of the existing vgmstream WAV conversions under `.local/audio-work`, with zero differing samples. WAV container bytes may differ because metadata chunks are not PCM content. MP3 is lossy and was not used as a PCM oracle.

| Source | PCM bytes | PCM SHA-256 |
| --- | ---: | --- |
| `chao_hall.adx` | 8,575,040 | `b708cfe8ab553a50c6c05c9eb4243947ea7b8b0490116df05acbe73095f6c1bd` |
| `chao_g_bgm_n.adx` | 13,756,840 | `494027c813c3c5597bef4f9ee2446d89c252b05b2ea667f75c21201c01757fce` |
| `chao_g_bgm_h.adx` | 14,936,940 | `0b0dae25e1cea2e0ca12c5de01b3125daa40e9a10775199fe3e4bdde345e8718` |
| `chao_g_bgm_d.adx` | 13,164,728 | `17afaa353ac7982bbbad8a2adacccad8733d22238a9a6a2725eb715f0d8b83ba` |

The English AFS bank (`SHA-256 3af71d52544d990c4e3e1428c7a4a8a1e5a11342cd5281f18c9a296c1df34287`) contains 2,727 indexed entries. The Rust reader reproduces all three previously extracted voice payloads byte for byte:

| Index | Source byte offset | Length | Prior extraction |
| ---: | ---: | ---: | --- |
| 2654 | 74,166,272 | 6,752 | `voice-knuckles.ahx` |
| 2662 | 74,272,768 | 6,615 | `voice-tails.ahx` |
| 2673 | 74,432,512 | 12,833 | `voice-amy.ahx` |

The local `Data_DLL.dll` retains SHA-256 `6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59`. Rust reads 488 named exports and validates all seven Chao root pointers at file offsets 85,428; 2,196,852; 2,216,388; 2,236,540; 2,256,988; 2,275,324; and 2,294,596. `AL_RootObject`, `al_motion_table` and original garden landtable exports are present. The four Transporter mapping addresses match the earlier extraction report: `E56784`, `E56994`, `E57104`, `E5A23C`.

Run the optional source-backed regression with `SA2_INSTALL` set to the owner-local installation and `SA2_REFERENCE_ROOT` set to the original checkout's `.local` folder, then `cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml local_audio_parity -- --ignored`. Missing reference inputs fail rather than silently skip. Normal tests use only synthetic headers and encoded samples, including truncation, invalid ordinals, aliased exports, unsupported codec/encryption and allocation limits. This is an asset-reader/conversion milestone, not a native audio engine or complete game port.
