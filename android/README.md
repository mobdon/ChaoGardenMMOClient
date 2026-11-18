# Tiny Garden Android widget

A native Android application and RemoteViews home-screen widget, separate from the browser preview at `widget.html`. Compact, wide and large layouts show the canonical Chao's needs, stats, activity and training availability. Android setup pairs an independent device credential; the widget opens the full Tiny Garden app for minigames and training controls.

**Delivery status:** the actual debug APK and instrumentation APK build successfully; Android lint and all 44 regular JVM tests pass. The production Java adapter also passes a real isolated Rust pairing/care/restart round trip. The native package includes the verified local A2NE garden and original Neutral/Hero/Dark Chaos artwork when staged. Device execution remains unverified: no physical device was attached and the isolated software emulator attempts did not reach a usable Android guest. Production deployment and paired Android acceptance remain open. See [verification/STATUS.md](verification/STATUS.md) for exact evidence and limits.

## Build and test separately from the browser

Prerequisites: JDK 17, Gradle 8.11.1, Android SDK platform 35/build-tools 35.0.0. Plugin: Android Gradle 8.9.2. Minimum Android 8/API 26; target API 35. Install Google's tools and accept its SDK terms through the official installer. Signing keys, tools, APKs, emulator state and original art stay outside Git.

```powershell
.\verify.ps1 -GradlePath C:\tools\gradle-8.11.1\bin\gradle.bat `
  -JavaHome C:\tools\jdk-17 -AndroidSdk C:\tools\android-sdk
# Add -Connected with an authorized adb device/emulator.
```

The script runs `assembleDebug lintDebug testDebugUnitTest assembleDebugAndroidTest`; `-Connected` also runs `connectedDebugAndroidTest`. Debug APK: `app/build/outputs/apk/debug/app-debug.apk`. Reports: `app/build/reports/`. Debug packaging is for verification; distribution needs project-owned release signing.

## Pair and use

1. Install the APK and open Tiny Garden. Enter a deliberately reachable HTTPS garden origin. A desktop loopback address is not reachable from a phone.
2. Approve an Android invitation from the desktop account and copy its two-minute pairing ID/secret. Android submits the invitation once and stores its independent device credential using Android Keystore AES-GCM encryption; backup is disabled. Lost claim responses require a new invitation.
3. Refresh and add the widget through the app or launcher picker. Responsive breakpoints are 160×180, 320×180 and 320×360dp; launcher grid counts vary. Compact shows one skill plus six in the app; wide shows seven levels; large shows all seven levels/grades. Accessibility retains points and hidden numeric grades.
4. Pet and Rest/Wake send Rust Watch-care intents only for a current resident, recent snapshot and fresh tap. Android obtains a short watch lease if needed, submits its current revision/sequence and reads the canonical account after acknowledgement. It releases only the lease it acquired for that tap. Conflicts require refresh; uncertain mutations are never automatically replayed. Feed is not exposed because the existing Watch backend does not implement its inventory transaction.
5. Open launches the same origin's `/tiny.html` in the full browser app. Sign in to the same account there; authenticated browser handoff remains separate. No credential enters a URL, WebView bridge or browser storage.
6. Forget removes local credentials/cache. Revoke the device from the desktop account to end server access. Local removal does not claim remote revocation; confirmed authorization failures clear local credentials.

## Data, art and background work

The native adapter reads authenticated `GET /api/account`. It requires the exact canonical Chao ID and private identity/revision and translates `hunger` to Fullness. It never picks another player or creates a profile. Server simulation time is not a UTC clock. The current contract has no confirmed save UTC or durable AFK job, so save time/training remain unavailable; the widget labels its device receipt time as **Read**, never **Saved**. An internal display schema does not introduce new server routes.

The launcher shows a static snapshot. JobScheduler requests background reads every 30 minutes; Android may delay them under battery/network/quota limits. Manual refresh is available in the app and large widget. No foreground service, exact alarm, continuous simulation, local training settlement or offline reward queue exists. Delayed and redelivered care work is dropped; process changes invalidate freshness. A pending intent is encrypted before sending. Later authoritative revision/lease/sequence changes fence it before another care is allowed.

[docs/ORIGINAL-ASSETS.json](docs/ORIGINAL-ASSETS.json) pins the eleven original/adapted Chaos PNGs. Native atlases preserve the original desktop bodies/poses; these are phone renderings, not recovered GBA sprites. Android composes a static front-idle frame directly from bundled, hash-checked art, including the original Dark flame. Canonical form/alignment selects the variant. Only literal server `admin=true` adds the cyan chest gem and all seven transparent sprites rendered from the unchanged original emerald geometry. Their source textures are opaque environment maps and are never used as square sprites. [WIDGET-EMERALDS.json](docs/WIDGET-EMERALDS.json) records mesh, material, texture and deterministic render provenance. Rendered gems are bundled-only and claim no Rust asset routes; atlases/flame retain the existing same-origin HTTPS fallback.

An unpaired widget without any saved snapshot can show **Dark Chaos · art preview**. This contains no account or Chao ID, rings, needs, stats, activity, training state or enabled care. Its visible status and accessibility descriptions identify the artwork example. A paired normal/unsupported form never becomes that preview. Missing/changed required art gives an explicit marker; normal-Chao and gear-specific art remain open.

The static portrait uses a uniform scale and places the original front-idle feet on verified grass in the original garden image. If any optional emerald is absent/corrupt, the actual body/flame stay visible while the entire chest/seven-gem group is hidden with an **ART LIMITED** label and accessibility explanation. Incomplete portraits are not cached, allowing recovery when correct assets are available.

Stage the private originals before building:

```powershell
.\tools\stage-original-art.ps1 -SourceImports C:\path\to\private\.local\imports
```

The script validates every PNG hash and dimension before copying any of the eleven files to ignored `app/src/localArt/assets/`; the separately staged A2NE garden remains there. If needed, `tools/render-widget-emeralds.py` uses the established Blender presentation pipeline on the already converted original GLBs. Original bytes and rendered outputs stay outside Git.

## Integration gates

The exact Rust API and limitations are in [docs/PROTOCOL-v1.md](docs/PROTOCOL-v1.md). Server draft PR #15 (`56af22d`) includes the canonical offline self in authenticated account snapshots. The earlier include-self patch is obsolete and has been removed; this client does not apply changes to the independent server.

Keep this PR a draft until Android build/lint/JVM/instrumentation execution and live pairing/care/resize/reboot/Doze acceptance pass against the integrated Rust service. Prove canonical identity across app entry, invitation expiry/replay, revocation, conflicting devices, stale input, failure after commit, save/restart, actual original art and all training phases once that service exists. Fixture/browser checks are not installed-widget evidence.
