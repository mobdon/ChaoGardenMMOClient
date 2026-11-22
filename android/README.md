# Tiny Garden Android widget

A native Android application and RemoteViews home-screen widget, separate from the browser preview at `widget.html`. Compact, wide and large layouts show the canonical Chao's needs, stats, activity and training availability. Android setup pairs an independent device credential; the widget opens the full Tiny Garden app for minigames and training controls.

**Delivery status:** the widget now uses the normal account login (the pairing design below this line in older notes no longer applies). The plain-Java classes compile and their 28 JVM tests pass; the APK has **not** been rebuilt or run on a device since this change. Files under `verification/` describe the earlier pairing build.

## Build and test separately from the browser

Prerequisites: JDK 17, Gradle 8.11.1, Android SDK platform 35/build-tools 35.0.0. Plugin: Android Gradle 8.9.2. Minimum Android 8/API 26; target API 35. Install Google's tools and accept its SDK terms through the official installer. Signing keys, tools, APKs, emulator state and original art stay outside Git.

```powershell
.\verify.ps1 -GradlePath C:\tools\gradle-8.11.1\bin\gradle.bat `
  -JavaHome C:\tools\jdk-17 -AndroidSdk C:\tools\android-sdk
# Add -Connected with an authorized adb device/emulator.
```

The script runs `assembleDebug lintDebug testDebugUnitTest assembleDebugAndroidTest`; `-Connected` also runs `connectedDebugAndroidTest`. Debug APK: `app/build/outputs/apk/debug/app-debug.apk`. Reports: `app/build/reports/`. Debug packaging is for verification; distribution needs project-owned release signing.

## Log in and use

1. Install the APK and open Tiny Garden. Enter the HTTPS garden server.
2. Log in with your username or email and password, or with your memory card code. The app calls `POST /api/account/login` (or `GET /api/card`) and stores only the Chao key the server returns, sealed with Android Keystore. The password is never stored.
3. The widget reads `GET /api/tiny-chao` and `POST /api/session`. Until the same account has played once on its PC the server has no Chao model and the widget says so.
4. When the stored model is a PNG picture sheet (8 x 8 cells of 96 px) the widget draws your own Chao from its first cell; otherwise it uses the shared Chaos pictures.
5. Pet and Rest/Wake open Tiny Garden in the browser: care needs a live garden visit, the widget only reads.
6. Log out removes the key and cached pictures from the device.

## Data, art and background work

The native adapter reads the player in the `POST /api/session` reply. It requires the logged-in Chao ID and translates `hunger` to Fullness. It never picks another player or creates a profile. Server simulation time is not a UTC clock. The current contract has no confirmed save UTC or durable AFK job, so save time/training remain unavailable; the widget labels its device receipt time as **Read**, never **Saved**. An internal display schema does not introduce new server routes.

The launcher shows a static snapshot. JobScheduler requests background reads every 30 minutes; Android may delay them under battery/network/quota limits. Manual refresh is available in the app and large widget. No foreground service, exact alarm, continuous simulation, local training settlement or offline reward queue exists. Delayed and redelivered care work is dropped; process changes invalidate freshness. A pending intent is encrypted before sending. Later authoritative revision/lease/sequence changes fence it before another care is allowed.

[docs/ORIGINAL-ASSETS.json](docs/ORIGINAL-ASSETS.json) pins the eleven original/adapted Chaos PNGs. Native atlases preserve the original desktop bodies/poses; these are phone renderings, not recovered GBA sprites. Android composes a static front-idle frame directly from bundled, hash-checked art, including the original Dark flame. Canonical form/alignment selects the variant. Only literal server `admin=true` adds the cyan chest gem and all seven transparent sprites rendered from the unchanged original emerald geometry. Their source textures are opaque environment maps and are never used as square sprites. [WIDGET-EMERALDS.json](docs/WIDGET-EMERALDS.json) records mesh, material, texture and deterministic render provenance. Rendered gems are bundled-only and claim no Rust asset routes; atlases/flame retain the existing same-origin HTTPS fallback.

An unpaired widget without any saved snapshot can show **Dark Chaos · art preview**. This contains no account or Chao ID, rings, needs, stats, activity, training state or enabled care. Its visible status and accessibility descriptions identify the artwork example. A paired normal/unsupported form never becomes that preview. Missing/changed required art gives an explicit marker; normal-Chao and gear-specific art remain open.

The static portrait uses a uniform 0.20 scale: its complete frame occupies 32 original-garden pixels, with the body about 20 pixels high. This is a presentation adaptation of the original desktop Chaos art, not recovered normal/GBA sprite art. Its original front-idle feet stay on verified grass, and the flame/emerald frame stays within the scene. If any optional emerald is absent/corrupt, the actual body/flame stay visible while the entire chest/seven-gem group is hidden with an **ART LIMITED** label and accessibility explanation. Incomplete portraits are not cached, allowing recovery when correct assets are available.

Explicit Hero/Dark/Neutral Chaos forms take priority over conflicting alignment
in both Android and the reviewed full Tiny Garden app. Generic Chaos forms use
the existing alignment thresholds. The full app now reuses its five existing
screens with original garden/sidebar presentation; see
[the presentation boundary](../docs/TINY-ORIGINAL-PRESENTATION.md).

Stage the private originals before building:

```powershell
.\tools\stage-original-art.ps1 -SourceImports C:\path\to\private\.local\imports
```

The script validates every PNG hash and dimension before copying any of the eleven files to ignored `app/src/localArt/assets/`; the separately staged A2NE garden remains there. If needed, `tools/render-widget-emeralds.py` uses the established Blender presentation pipeline on the already converted original GLBs. Original bytes and rendered outputs stay outside Git.

## Integration gates

The exact Rust API and limitations are in [docs/PROTOCOL-v1.md](docs/PROTOCOL-v1.md). Server draft PR #15 (`56af22d`) includes the canonical offline self in authenticated account snapshots. The earlier include-self patch is obsolete and has been removed; this client does not apply changes to the independent server.

Keep this PR a draft until Android build/lint/JVM/instrumentation execution and live pairing/care/resize/reboot/Doze acceptance pass against the integrated Rust service. Prove canonical identity across app entry, invitation expiry/replay, revocation, conflicting devices, stale input, failure after commit, save/restart, actual original art and all training phases once that service exists. Fixture/browser checks are not installed-widget evidence.
