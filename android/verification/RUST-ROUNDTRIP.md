# Production Java adapter against the real Rust service

Verified November 18, 2026 in an isolated test store. This is executed transport
and persistence evidence for the widget's Java `Gateway` and `AccountAdapter`.
The live preview process, canonical owner profile and server source were untouched.

The Rust accounts worktree was clean at
`56af22dece8551376f4c8b9a1882acecfff02bf1`. Its real service binary was built
with the original checkout's Rust 1.100 setup into a fresh ignored test directory.
The tested executable SHA-256 was
`76df7aa256296b26468aa25a7bb5c2973d6b6a6fd9fea9413a78a34d89aca913`.
It listened only on `127.0.0.1:4317` with an empty test store and owner bootstrap
disabled. Both service runs shut down cleanly and the listener was removed.

`RustServiceIntegrationTest` passed in two separate Gradle invocations:

- A fixture desktop session approved a pairing invitation. The unchanged
  production `Gateway.claim` redeemed it and `Gateway.account` supplied the real
  response to `AccountAdapter`. The original account/Chao identity, all seven
  stats and rings matched; no duplicate Chao appeared. Reading a disconnected
  nonresident self also passed.
- Reusing the consumed invitation and using an invalid credential failed.
  An acquired Watch lease and bounded `watch-start`/`watch-care:pet` intents
  committed through the actual Rust host. Happiness increased by its server
  rule; stats/rings remained unchanged. The test respects the host's existing
  80ms message throttle and requires the authoritative accepted acknowledgement.
- Stale revision and repeated sequence requests returned HTTP 409 without
  another change. The script stopped the process, restarted that same binary
  on its test store, and used the same Android credential again. Identity,
  seven stats, rings, committed care, resident opt-in and lease sequence survived.
- Sequence replay still failed after restart. Desktop revocation made that
  Android credential return HTTP 401 while the desktop retained the same Chao.

The production adapter requires HTTPS. A separate **test-only** JDK17
`HttpsServer` bridge forwarded bounded requests into the real Rust HTTP
listener. `keytool` generated a one-day localhost SAN certificate inside the
ignored run directory. The test JVM trusted only that certificate, retained
normal hostname verification and restored its default SSL socket factory in
`finally`. No application TLS policy was weakened. The bridge generated no
game responses or progression decisions.

Reproduce using installed local tools:

```powershell
./android/tools/verify-rust-roundtrip.ps1 `
  -ServerRoot C:/path/to/server-accounts-worktree `
  -RustSetup C:/path/to/original/client/scripts/use-rust.ps1 `
  -GradlePath C:/path/to/gradle-8.11.1/bin/gradle.bat `
  -JavaHome C:/path/to/jdk-17 `
  -AndroidSdk C:/path/to/android-sdk
```

The script checks that tracked Rust inputs match the recorded commit, builds a
new isolated binary/store, compiles the JDK bridge separately from Android,
runs the production adapter test, restarts Rust, then runs the persistence and
revocation phase. `TINY_WIDGET_RUST_TEST_CONFIG` contains only the generated
configuration path. It is restored afterward. Without this explicit environment
gate the JUnit test skips; a normal APK build never starts or changes a service.
Private fixture credentials, certificates and profiles remain in ignored
`.local/verification/rust-roundtrip/`. The sanitized receipt for this passing run
is `65c0bb681dbd46e68854ff7d3039961e/roundtrip-evidence.json` there.

This proves the production adapter's real canonical service roundtrip under
the accounts branch. It does not prove installed launcher interaction,
Android Keystore execution, a public HTTPS deployment, authenticated browser
handoff or durable AFK training. Android packaging and device evidence are
recorded separately in `STATUS.md`.
