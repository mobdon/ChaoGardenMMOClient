# Android verification — November 18, 2026

Follow-up branch `codex/tiny-widget-native-data`, based on published widget commit
`550c9c5c229c86b8c74f389e300d335b521b905b`. Native checks use the isolated managed
worktree. The live preview, canonical owner/profile and independent server
checkout are not writable test fixtures.

## Actual packaging

`android/verify.ps1` successfully ran the real Gradle tasks `assembleDebug
lintDebug testDebugUnitTest assembleDebugAndroidTest`, including the locally
staged original A2NE garden background and all eleven original/adapted Chaos
art assets. Application and instrumentation APKs
were produced independently of the browser build. Debug signing, original
pixels and generated APKs remain local/ignored.

All 44 regular JVM tests passed: AccountAdapter 9, CarePolicy 11,
OriginalCatalog 8, PortraitPlacement 2, ServerOrigin 3, Snapshot 11. The default report contains 45
tests, zero failures/errors and one deliberately skipped real-service test.
The separately gated test passed in both phases of its dedicated runner below.

Initial lint reported zero errors and 27 warnings, including small widget text,
setup string translation, baseline alignment and unused resources. The Android
12 backup/transfer warning was addressed with explicit `dataExtractionRules`
exclusions, in addition to disabled backup. The final verification passes with
zero lint errors and 26 warnings. Warnings are not suppressed.

Final local artifacts from the configured four-task verification:

| Artifact | SHA-256 |
| --- | --- |
| `app/build/outputs/apk/debug/app-debug.apk` | `15adadc4aba82d63e4d35a7a058d0ea904c1bd0568a35616f09291ace626a493` |
| `app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk` | `1faa1517d2d654efa33536000faba816940a6d84e72bb79d6425f106d6506f1d` |

Google's `apksigner verify --verbose --print-certs` verifies the application APK
using APK Signature Scheme v2, one Android Debug signer. Its certificate SHA-256
is `38c2fe3853558f8a1f525feee8ae8fdbfae82fa31c5002cda4c25aabc5155755`.
This is local debug signing, not production distribution signing.

`aapt dump badging` confirms package `garden.tiny.widget`, minimum API 26,
target/compile API 35, a launchable PairingActivity and app-widget component,
with only INTERNET and ACCESS_NETWORK_STATE permissions. Compiled manifest
inspection confirms disabled backup/cleartext, an unexported care receiver,
the JobScheduler service's BIND_JOB_SERVICE permission and the extraction rules.
The APK's 14,636-byte `assets/tiny-garden-a2ne.png` entry has the exact expected
SHA-256 `75eb4d2839418fc730773377a1135776a0c361cbb1b6967af8134d8d4b5469ae`.
All eleven atlas/flame/transparent-gem APK entries also match their exact
`ORIGINAL-ASSETS.json` hashes. The original opaque emerald environment textures
are absent from the APK. The final application signature verifies using v2.
The local final build log is
`verification/local/build-tiny-native-form-priority-2026-11-18.log`.

The final follow-up preserves the reviewed Tiny app's explicit form precedence:
Hero/Dark/Neutral Chaos form names win over conflicting alignment; generic
Chaos forms use the existing alignment thresholds. The expanded catalog
regression, all four Gradle tasks, 44 regular JVM cases and lint passed again.
The resulting application APK's v2 signature verified; the instrumentation APK
is unchanged. This does not add Android device execution evidence.

The new catalog tests distinguish art-only unpaired preview descriptors from
canonical players, block preview whenever any credential/snapshot exists,
reject filename/hash mismatches and forbid server routes on bundled-only gem
renders. All seven canonical admin gems use the final deterministic hashes in
`WIDGET-EMERALDS.json`; only Blender Date/RenderTime metadata was omitted, with
independent repeated renders preserving exact PNG bytes and decoded RGBA.
The staging script verified all eleven actual hashes/dimensions. A one-byte
modified atlas was rejected before any staging; the prior staged atlas hash
remained unchanged. Original bytes remain ignored. Native Canvas composition
and visible/accessibility preview labels still require Android runtime QA.

The two grounding regressions preserve uniform 0.20 portrait scale (32 original
garden pixels for the complete frame, approximately 20 pixels for the body), retain the
complete original portrait/flame/emerald frame within the scene and map all
three original front-idle foot bounds to source garden grass point
`(109.12,108.8)`, rather than the pond under the previous centered pose.
Native optional-ornament loading preserves the actual base body/flame and
suppresses the complete chest/seven-gem group if any member fails validation.
It reports ART LIMITED/accessibility details and never caches an incomplete
portrait, so corrected local assets can recover. These are source/JVM layout
boundary checks; Android Canvas failure/recovery still needs device execution.

## Real Rust service round trip

`tools/verify-rust-roundtrip.ps1` passed the production Java Gateway and
AccountAdapter against an actual newly built Rust accounts-v2 service, source
commit `56af22dece8551376f4c8b9a1882acecfff02bf1`, binary SHA-256
`76df7aa256296b26468aa25a7bb5c2973d6b6a6fd9fea9413a78a34d89aca913`.
The isolated service uses loopback port 4317 and a fresh private test data
directory. A test-only localhost HTTPS proxy forwards to that Rust service;
default hostname verification stays enabled. Production Android trust settings
are unchanged. This creates a test account rather than modifying the live owner.

The exercise phase pairs a device, reads the same canonical Chao and seven
stats/rings, starts a server watch session and commits Pet care. Pair replay,
stale/duplicate intents and unauthorized access are rejected. After a clean
Rust shutdown/restart, the same credential and committed care survive, the
same canonical Chao/stats remain, sequence replay is rejected, and revocation
removes access. The gated JUnit test executed once successfully in each phase.
The sanitized source/binary receipt is recorded in
`RUST-ROUNDTRIP-2026-11-18.json`; private test fixtures remain ignored.

This proves the real service transport and canonical persistence boundary.
It does not prove Android OS networking/Keystore, the user's existing account
migration, public HTTPS deployment or a running Android launcher widget.

## Toolchain

Tools live under the original checkout's ignored `.local/tools/`. The user's
existing SDK-license acceptance was used by Google's official installer. No
global Java/Gradle installation or Windows hypervisor configuration was changed.

| Tool | Version / package | Download SHA-256 |
| --- | --- | --- |
| Microsoft OpenJDK | 17.0.21.1+2, Windows x64 | `a07e270f86aec056d9c2acf981eb64a38e30814dacfadbc5f80f31c55d5a4427` |
| Gradle | 8.11.1 binary | `f397b287023acdba1e9f6fc5ea72d22dd63669d59ed4a289a29b1a76eee151c6` |
| Google Android command-line tools | 22.0, build 15859902 | `90ae805d20434428bffcb699c290860f19bb5f66a67e6b330067e3de801fb04a` |
| Android SDK | platform 35, build-tools 35.0.0, platform-tools | Installed by Google's SDK manager |
| Android Gradle Plugin | 8.9.2 | Declared official Gradle repositories |

The three tool archives were checked against official publisher SHA-256 values
before extraction. SDK packages use Google's installer. The older Android
plugin prints an SDK XML v3/v4 warning with current command-line tools; the
configured build tasks completed.

## Device/emulator attempts

No physical adb device is attached. The isolated AVD uses Google's API 35
default x86_64 system image, revision 2. CPU hardware virtualization is enabled;
`emulator -accel-check` reports `6` and `Android Emulator hypervisor driver is
not installed on this machine`.

Software-only attempts without driver installation or Windows changes:

- Emulator 37.2.13: `-accel off -gpu swiftshader_indirect -no-window -no-audio
  -no-boot-anim -no-snapshot -memory 2048 -cores 4 -port 5556`; observed about
  96 seconds. Adb stayed offline; no usable guest appeared.
- Same version: `-accel off -gpu off -feature -Vulkan -no-window -no-audio
  -no-boot-anim -no-snapshot -memory 2048 -cores 1 -port 5556 -show-kernel`;
  observed about 189 seconds. Adb stayed offline; no kernel output appeared.
  Both incomplete attempts were stopped by their own process IDs.
- Concrete fallback: official stable Emulator 35.4.9, archive build 13025442,
  SHA-256 `8f0ead88560cb1841ddb38681669b33bbf71c39081b1cc0ada6c56d6aad2cb6f`.
  Flags: `-accel off -gpu swiftshader_indirect -feature -Vulkan -no-window
  -no-audio -no-boot-anim -no-snapshot -memory 2048 -cores 1 -port 5556
  -show-kernel`. Startup exited unsuccessfully within seven seconds. A repeat
  with `-verbose` captured native process exit `-1073741819` (`0xC0000005`,
  access violation) before guest kernel output. The crash reporter printed
  `NtQuerySystemInformation SystemExtendedHandleInformation ... (0xc0000004)`;
  that is a crash-reporting error, not a proven emulator root cause. The full
  verbose log remains local at `.local/tools/android-downloads/emulator35-startup.log`
  in the original checkout.

The missing hypervisor driver is an observed gap, not the proven cause of these
software startup outcomes. No emulator remains running. Final `adb devices -l`
returns an empty device list.

The five Android instrumentation methods are packaged but unexecuted. They
apply three actual RemoteViews sizes, check 48dp controls and increased font
size, preserve seven accessible stats and inspect package protections. No
passing AppWidgetHost binding, launcher placement, native screenshot, reboot or
Doze result exists. Compilation is not installed-widget acceptance.

## Remaining acceptance

- Run `verify.ps1 -Connected` on an authorized usable Android device/emulator;
  verify an actual AppWidgetHost binding, launcher placement, all three sizes,
  accessibility, reboot and background behavior.
- Exercise Android Keystore and real HTTPS pairing, canonical reads, revocation,
  conflicting/stale care, unknown commit outcomes and restart. Separately
  recorded JVM transport execution does not prove Android runtime behavior.
- Verify canonical native forms, A2NE background, flame/chest/seven emerald
  identity and full Tiny Garden entry on Android.
- Durable AFK status remains unavailable until supplied by the real service.
  The widget cannot start, cancel or settle training. Authenticated browser
  handoff, normal/gear-specific art and production release signing remain open.

Keep the PR a draft until missing runtime/paired-device acceptance is complete.
Browser screenshots, actual APK packaging and JVM service calls prove their
respective boundaries.
