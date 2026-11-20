# Verification — November 18, 2026

Source baseline: published client `af74a8df70b9781bb1dafe5ce148905ffa788716` (`origin/main`). Isolated branch: `codex/android-tiny-garden-widget`; worktree `.local/worktrees/android-widget`. The live checkout, preview processes, profile data and independent server history were not changed by this workstream.

## Executed

- `npm run build`: passed; all three browser entries built. Existing native asset runtime references and large main bundle warnings remain. This is browser evidence only.
- `node --check src/widget.js`: passed.
- `node --test tests/*.test.js`: 7 passed, 1 failed. The standalone Vite boundary test receives 403 for `/src/main.js` because this worktree is inside `.local/`, which the existing Vite filesystem deny rule excludes. No Vite/test/shared boundary changes were made. The first sandbox attempt could not spawn child processes; the reported 7/8 run used the existing approved Node test escalation.
- All 13 Android XML resources/manifest parsed successfully.
- `git diff --check`: passed.
- Protocol examples parsed by the contract subtask. Read-only source review checked the published Rust baseline and the accounts-v2 workstream's account/pairing/lease/intent implementation. The native adapter now uses that exact API; it is not yet deployed or tested on Android.
- Verified all 11 original PNG hashes/dimensions and all three source GLB hashes against local provenance. Published sanitized metadata only; all original bytes remain ignored/local.

## Native checks not yet executed

No JDK, Gradle, Android SDK, adb or emulator/device was present in targeted toolchain locations or PATH. An isolated JDK download failed in the network sandbox; both escalated official JDK download requests were canceled before download. The user accepted the Android SDK terms. Acceptance of SDK terms is not a successful download/build. No APK, Android lint report, executed JVM/JUnit result, native screenshot, installed-widget result or physical-device evidence exists at this checkpoint. No download or approval remains running.

`verify.ps1` provides the exact native build/lint/JUnit/instrumentation-APK commands once tools are available. Source includes Snapshot/Origin/AccountAdapter/CarePolicy/OriginalCatalog JVM regression tests and five Android instrumentation tests, including three real RemoteViews sizes at font scale 1.3 and package protections. Authored tests are **not** passing-test evidence.

## Cross-repository blockers

- Identity v2 pairing claim/HTTP routes and secure reachable HTTPS deployment are not present on the published Rust baseline.
- Integrate/deploy the Rust accounts-v2 PR and validate actual HTTPS Android HTTP pairing/account/lease/intent calls. Read-only account requests never acquire a lease; care uses Rust's watch ID/revision/sequence fencing and never retries an uncertain mutation automatically. Restricted widget scopes and token expiry remain future server policy.
- The current server public-player filter omits an offline nonresident self. `integration/include-self.patch` passed `git apply --check` against the accounts worktree; it was not applied to the separately owned server. Missing canonical self is rejected rather than replaced.
- Durable AFK service integration is separate; absent state shows unavailable. The widget never starts, cancels or settles training.
- Original assets remain local/runtime-only. Verify actual paired Neutral/Hero/Dark forms, original flame/chest/seven emeralds and provenance on Android after deployment.
- Full app entry opens the same origin's Tiny Garden browser route without a credential. The full app requires its own login to that canonical account; authenticated app handoff is not implemented here.

Keep the PR a draft until native packaging and live canonical-Chao acceptance are completed. Do not label the browser build, XML parse or fixtures as an installed Android widget test.
