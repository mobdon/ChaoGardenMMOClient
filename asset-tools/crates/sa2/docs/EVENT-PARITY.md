# Installed PC event scene parity

SA Tools 786 splitEvent and the Rust event reader independently read the four installed PRS events below. Their exports agree on scene order and frame counts, every entity's model / GC model / shadow-model reference, motion and shape-motion index, position, flags and layer, and the ordered camera and particle motion lists. The test resolves each motion ID through SA Tools' own exported motion dictionary. SA Tools writes positions as decimal strings; each component must agree to half a unit of its seventh significant digit, with exact equality for zero.

| Installed event | Source SHA-256 | Scenes including common | Entities | Camera entries | Particle entries |
|---|---|---:|---:|---:|---:|
| e0000.prs | 49b107a9520c13c1ffbbb1ac7f6b3f053b62b71211da03c4be46f5d79c06f4a2 | 5 | 121 | 5 | 6 |
| e0100.prs | c127cd9e8cb194cf6dd30fb17c10d01594398b83e0cdfab2ec496274f398b3bb | 6 | 99 | 5 | 2 |
| e0200.prs | b0a38cbd0f4bb4e916f7cdf24ae59d90b8d95dd4e4338648735754112d252f04 | 4 | 39 | 3 | 12 |
| e0207.prs | 6eb7b71e6da5b80523559b26537a2a7627d7af19728a7eebc7326d0cd92bca84 | 10 | 162 | 9 | 2 |

The installation contains 67 files matching `eNNNN.prs`. Rust read 66 retail BATTLE images containing 741 scenes and 8,132 entities. The remaining file, `e0350.prs` (SHA-256 `6632c08cd5692a50b695734055a110907056db742cdea08a4bb7445f960224ae`), is a different format. SA Tools identifies it as GC/PC Beta and exports 2 scenes with 1 and 49 entities. Its JSON reports BigEndian=true and BattleFormat=false. The retail reader rejects this image. Only the four named events received independent field-by-field comparison; the other retail files passed structural decoding. The checks do not verify rendered scenes, camera interpolation, audio or retail timeline playback.

To reproduce, set `DOTNET_ROLL_FORWARD=Major` and run SA Tools 786 `bin/splitEvent.exe INPUT OUTPUT` for each named PRS file. Use the original installed event directory so splitEvent can read companion motion files. The reference DLL `bin/lib/splitEvent.dll` has SHA-256 `72b0150305c46108991bf4beb0032c7664c9778f6b028055c737e2a1fb8e2003`. For each ID, use an output directory named `<reference-root>/<id>`; splitEvent writes `<id>/<id>.json` inside it. Set `SA2_SET_DIR` to the local `resource/gd_PC` directory and `SA2_EVENT_REFERENCE` to the reference root. Run `cargo test --locked --test event_local -- --ignored --nocapture`. All exported models, animations and JSON stay in ignored local storage.
