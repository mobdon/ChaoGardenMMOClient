# Running a story from the local PC installation

Run the headless Rust dispatcher from the checkout. It reads the authored sequence, loads consecutive opening event images, validates each event timeline at its final frame, and leaves the first unhandled action pending. The JSON and dependency manifest stay under the checkout's ignored .local directory. Use a new output filename on subsequent runs.

    sa2-assets story-run GAME/sonic2app.exe .local/hero.json PC_SA2/ADVERTISE.ini "Hero Story Sequence"

| Sequence | Installed events inspected (frames) | First pending action |
| --- | --- | --- |
| Hero | 0 (1,655) | 1: level 13, City Escape |
| Dark | 100 (1,189) | 1: level 11, Iron Gate |
| Last | 200 (866), 201 (1,169) | 2: event 602; installed Sofdec movie awaits playback |

This is a content run without drawing, elapsed-time playback, or Chao controls. No story clear is acknowledged. Controlling a Chao in City Escape still needs verified movement, collision response, stage objects, mission completion and a presentation adapter. A fan-game growth rule and once-only durable reward belong to the canonical Rust server; the dispatcher neither reads nor writes a save. For event 602 the dispatcher verifies the installed e0602_m.sfd hash and walks the MPEG pack and packet boundaries through the end marker. It reports 14,099 packs, 12,632 video packets and 732 packets in each of two audio streams. The local manifest includes the original movie hash. This does not decode frames, select a language or acknowledge movie completion.

## Paired City Escape poses

    sa2-assets city-rider-pose STAGE .local/rider.json GAME/sonic2app.exe GD_PC/sonicmdl.prs 121 3.25

STAGE accepts the pinned stg13D.rel or Data_DLL.dll. Each has 24 original Sonic motion slots (121–144) paired with 24 board slots (145–168); aliases reduce each bank to 21 distinct headers. The returned model-local poses preserve the original Sonic model 0 hierarchy and streetboard. This original Sonic animation is not a Chao animation or a downhill movement controller. Angles, translations and scale stay in source units. Sampling does not modify textures.

Run tests/board_motion_reference.ps1 with -Rider (with -PcDll for the DLL) to independently digest the 38,366 ordered rider keys per source using SAModel. The installed tests compare the hashes for both formats, the split model's hierarchy and transforms, and all 24 executable action records against the SA Tools split Sonic Animation List. For the latter point SA2_RIDER_REFERENCE at the private reference directory containing split/sonicmdl/0.sa2mdl and actions/figure/sonic/Sonic/Animation List.ini. Generate those with SA Tools split mdl on sonicmdl.prs and split binary using the Sonic Animation List section of PC_SA2/Characters.ini respectively.

The character-animations command reads all 16 native character animation lists (3,154 rows) from the pinned Steam executable. Each array index is an animation state selector; it does not identify the player action that selects it. Rows preserve motion and model slots, mode, next animation, transition speed and playback speed. Table and row addresses and table digests are included. City Escape uses the same reader for its known Sonic board states. Runtime overrides, controller state changes and motion-bank resolution for other characters remain open.

To reproduce the independent check, run `sa2-assets character-animations EXE OUTPUT.json` with a private output path, then `tests/character_animations_reference.ps1 -GameExe EXE -ToolsRoot SA_TOOLS_786 -RustJson OUTPUT.json`. The check resolves Characters.ini addresses with .NET PEReader and compares every row with SplitTools.SA2AnimationInfo.

Character MTN archives: `character-motion-bank MTN.prs OUTPUT.json` indexes each physical slot, including aliases and duplicate IDs. `character-motion MTN.prs OUTPUT.json SLOT` reads native position/rotation (0x0003/2), position/rotation/scale (0x0007/3), vertex/normal morph pointers (0x0030/2), and camera position/target/roll/angle keys (0x01c1/4). It rejects ambiguous slot IDs. Morph pointers are file-relative; array lengths require matching geometry. Camera keys do not select the active camera. Directory offsets refer to the decompressed big-endian file. The output receipt hashes its compressed input.

`sonic-state-pose MTN.prs OUTPUT.json EXE SONICMDL.prs STATE FRAME` samples the pinned Sonic rig from a supplied animation state and frame in either the common or Sonic bank. It checks the selected model and part count, and does not resolve stage overrides or advance the player controller. Installed examples: state 0 / frame 0 in plcommtn.prs and state 87 / frame 0.5 in sonicmtn.prs.

For the local regression, run SA Tools 786 splitMTN on a private copy of sonicmtn.prs, set SA2_MTN_SPLIT to its output directory and SA2_SET_DIR to gd_PC, then run `cargo test --locked --test character_motions_local -- --include-ignored`. All 34 Sonic directory rows (32 unique motions) matched the split offsets and every decoded frame, position and rotation value. SA Tools normalizes some negative zeroes on export; Rust retains the original bits. The directory test also reads the other 16 installed banks and checks the ambiguous Amy slot.

`character-state-pose MTN.prs OUTPUT.json EXE MDL.prs STATE FRAME` selects the native animation table from the verified model archive hash. Supported base rigs: Sonic, Shadow, Tails, Eggman, Knuckles, Rouge, both mechs, Super Sonic and Super Shadow. Supply the corresponding character motion bank or the common bank. A shared numeric slot does not authorize borrowing another character's motion. The animation row still selects the model ID within the archive. Costumes, runtime overrides and player controller transitions are outside this command.

The installed regression samples one state per character at frame 0.5 and compares every node's flags, part binding and local/world transforms to the same evaluator using independent SA Tools splitMDL and splitMTN files. Set SA2_GAME_PATH and SA2_CHARACTER_SPLITS (containing e.g. teriosmdl/65.sa2mdl and teriosmtn/169.saanim), then run `cargo test --locked --test character_poses_local -- --include-ignored`. This checks reader parity; it does not compare against live engine poses.

The installed character_channels_local test reads all 19 directory entries in the three additional layouts. With SA2_SET_DIR and SA2_CHARACTER_SPLITS set, it compares every key of Amy slot 229, Rouge slot 206 and Super Sonic slot 106 against splitMTN exports. For relocated morph pointers it compares nullness and the first referenced vector; it does not assert a full mesh deformation.

For Last Story event 602, Rust also demultiplexes both original ADX tracks (C0 and C1), decodes each to stereo 44,100 Hz signed 16-bit PCM, and reports source and PCM hashes. Each track has 1,311,093 sample frames. Independent local verification: extract each track with FFmpeg using -map 0:a:0 or 0:a:1, -c copy -f data; decode the extracted ADX with vgmstream-cli. The installed regression compares both complete ADX hashes and both WAV data-chunk hashes. Movie video decoding, language selection, and synchronized presentation remain open.

The Rust demultiplexer also returns the original E0 video elementary stream. For event 602 all 25,489,466 bytes matched FFmpeg stream copy. Its sequence header gives 640 by 480; the Rust count of 891 picture headers matches ffprobe -count_frames. This verifies encoded video extraction only. The demux report preserves all 2,356 timestamped PES packets in file order with original packet offsets, elementary stream offsets and 90 kHz PTS plus optional DTS. The installed test compares every timing row with ffprobe -fflags +noparse+nofillin -show_packets, treating absent DTS as PTS only for that comparison. The 892 video timing rows are packet anchors, not a one-to-one frame schedule. A synchronized player still needs decoded pictures and a presentation clock.

For event 602, presentationOrder associates each MPEG picture header with its timed source packet and retains both the coded index and on-screen timestamp order. The final extra video timestamp has no new picture. Independent ffprobe -show_frames comparison matched the presentation timestamp, packet position and picture type for all 891 pictures (52 I, 251 P, 588 B). This schedule does not reconstruct decoded pixels.

Use campaign-frame with "Last Story Sequence" action 2 and a zero-based picture index (0 through 890) to inspect one movie picture in presentation order. The output includes the coded index, source offsets, and 90 kHz timestamp plus a local source manifest. Movie pictures have no event camera slots; this command returns metadata, not pixels.

The story movie reader recognizes all nine hash-pinned PC event SFDs: 401, 409, 411, 420, 429, 430, 524, 532 and 602. The installed stream regression checks every MPEG video and ADX elementary stream against an independent FFmpeg stream copy (nine video, seventeen audio), and each coded picture count against ffprobe. Movie 401 has only C0. For 411, 429, 430, 532 and 602 the picture order also matched ffprobe by timestamp, source packet and picture type. In 401, 409, 420 and 524 multiple pictures share a packet timestamp; campaign-frame can now locate their pictures by display ordinal using the temporal reference within each MPEG group. For these four movies Rust retains a picture PTS when its sequence/group prefix (if any) and picture start both fall in the same timed PES payload and that anchor has not already been consumed. This verifies 310/335, 321/327, 2380/2389 and 1004/1010 picture times respectively against ffprobe frame PTS and packet offsets. The remaining 46 picture times are null; packetPts is still available as a packet anchor. Across all nine films the ordered coded indices and picture types match ffprobe decoding the extracted video elementary streams. No picture decoding or cinematic completion is implied.

Decoded audio verification covers all seventeen movie tracks, totaling 27,984,524 sample frames. For each source stream the installed regression compares the Rust sample count, channel count, sample rate and complete interleaved signed 16-bit little-endian PCM hash against FFmpeg stream copy followed by vgmstream-cli -i decoding. Only reference measurements are tracked. This does not verify language selection or synchronization with video.

To export one original soundtrack locally, use:

    sa2-assets --source-root GAME movie-audio GAME/resource/gd_PC/event/e0602_m.sfd .local/event602-c1.wav 602 c1

Use c0 or c1 as encoded in the movie; event 401 has only c0. The converter verifies the movie against the supplied event ID, writes a native-rate PCM WAV, and records the source and output hashes in its sidecar. Its manifest asset ID includes the event and stream. Track ID does not establish a language.

To export the unchanged video elementary stream from any verified movie, use:

    sa2-assets --source-root GAME movie-video GAME/resource/gd_PC/event/e0602_m.sfd .local/event602.m1v 602

The output is encoded MPEG-1 video only. The sidecar identifies event and E0 stream, keeps the movie and export hashes, and labels the representation opaque. Use the movie inspection report for container timestamps; .m1v does not carry the audio or PES timing.
