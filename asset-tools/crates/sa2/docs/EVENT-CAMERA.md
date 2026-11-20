# BATTLE event camera frames

The Rust camera sampler reads a raw eNNNNmotion.bin for the same event as the scene directory. Pass an explicit motion index from the scene's camera_motions list and a frame in that motion. Some scenes reference more than one camera. The sampler returns the requested camera individually; selecting or combining multiple tracks requires the event scheduler.

For each one-part linear camera (motion header 0x01c10004), the four animated channels are position, target, roll and field of view. Roll and FOV use signed 32-bit BAMS values, with 65,536 units per turn. Vector interpolation follows the order of operations in SAModel's GetPosition and GetTarget. GetRoll and GetAngle interpolate their integer values and round half away from zero. Exact keyframes preserve the source values. The final half frame wraps toward frame zero according to the reference evaluator. The clip interval is zero inclusive to frame_count exclusive. Unsupported types, missing channels, invalid indices, nonfinite frame times, bad pointers and unordered key times are rejected.

The report separately retains the NJS_CAMERA attributes: static position, Z direction, roll, FOV, signed near/far clipping depths and the local X/Y axes. These fields come from the camera-data reference following the motion header. A separate field preserves the associated animation reference. Source layouts are [SA3D camera data](https://github.com/X-Hax/SA3D.SA2Event/blob/e68828b9e7dd3aa8b4ea790cc93461a68b71ab76/src/SA3D.SA2Event/Model/AnimationData/CameraData.cs), [SA3D camera animation](https://github.com/X-Hax/SA3D.SA2Event/blob/e68828b9e7dd3aa8b4ea790cc93461a68b71ab76/src/SA3D.SA2Event/Model/AnimationData/CameraAnimation.cs), and [SA Tools interpolation](https://github.com/X-Hax/sa_tools/blob/cc05c849b1b7017fb0b519932863e206fa34077a/Libraries/SAModel/Animation.cs#L3106-L3366).

## Independent comparison

SA Tools 786 splitEvent.exe exported 13 scene camera motions from the installed E0000, E0100 and E0200 files. All four animated channels matched SAModel.dll's evaluator at every integer and half frame, including the last half frame, for 7,780 samples. For all 13 tracks, the separate camera attribute INIs matched all the static fields listed above. This validates bank decoding and sampling against the independent tools. No retail playback capture, view/projection matrix or rendered shot is part of this test.

From the sa2 crate directory:

```powershell
$env:SA2_EVENT_DIR = 'C:\path\to\resource\gd_PC\event'
$cameraOutput = 'C:\path\to\private\camera-verification'
./tests/event_camera_reference.ps1 -ToolsRoot 'C:\path\to\sa-tools-786' -EventDirectory $env:SA2_EVENT_DIR -PrivateOutput $cameraOutput
$env:SA2_CAMERA_ORACLE = Join-Path $cameraOutput 'camera-frames.bin'
cargo test --locked --test event_camera -- --include-ignored
```

Choose a new private output directory for the native motion exports and oracle. The tools ran on Windows with DOTNET_ROLL_FORWARD set to Major for the installed Windows Desktop runtime.

For character MTN cameras use `character-camera MTN.prs OUTPUT.json SLOT FRAME`. The explicit slot and fractional clip frame select position, target, roll and field of view. This command does not read event attributes from a character archive. The four installed tracks are Super Sonic 106/108 and Super Shadow 111/115. Their durations are 90/64 frames respectively for each character. All 616 integer/half-frame samples match SAModel's evaluator bit for bit. The tests cover the final fractional interval, reject the end boundary, and do not establish when the game activates a track.

To regenerate the comparison, supply `tests/character_camera_reference.ps1` with the SA Tools 786 root and the private directory containing the splitMTN exports (ssonicmtn and sshadowmtn). Set SA2_SET_DIR to gd_PC and run `cargo test --locked --test character_camera_local -- --include-ignored`.
