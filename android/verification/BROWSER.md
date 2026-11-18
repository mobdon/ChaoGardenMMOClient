# Browser verification — November 18, 2026

The isolated branch preview runs at `127.0.0.1:5177/widget.html` from a managed
worktree outside `.local`. The original preview and Rust process remain on
their original ports. No private-source deny rule was weakened.

Executed checks:

- `npm test`: 23 passing client tests, including thirteen widget projection,
  bounded-request and protocol/auth/identity tests. The formerly failing private
  source boundary passes from this worktree location.
  A real cross-origin HTTP 307 test confirms that resume-token bodies are not
  forwarded; all observer requests reject redirects.
- `node --test android/preview/preview-boundary.test.mjs`: passes. The explicit
  local-art route rejects a wrong hash without serving its contents, and
  encoded private paths stay blocked.
- `vite build --config android/preview/vite.config.mjs`: passes with the local
  art configured. The emitted PNG has the same pinned SHA-256 as the native
  APK. Existing main-bundle size/runtime-asset warnings remain.
- Actual in-app browser: compact, wide and large controls switch correctly;
  the original garden image renders while disconnected; needs/levels/rings
  remain unknown and AFK explicitly unavailable. At a 390×844 viewport, the
  full large widget remains visible by scrolling without horizontal overflow
  (`clientWidth=scrollWidth=375`, accounting for browser scrollbar). Restored
  the default viewport after validation.
- Opened the existing live widget at port 5173 and followed its Tiny Garden
  link. Both displayed the same saved Chao and canonical values from the actual
  legacy Rust service. This confirms that live handoff was working during
  inspection; it is not evidence of a v2 deployment or the new overlay using
  the live owner's credentials.
- The production JavaScript observer also passed real HTTP and authenticated
  WebSocket reads against the isolated Rust accounts-v2 executable before and
  after restart, retaining the canonical synthetic test Chao without taking
  an activity lease. Sanitized source/executable evidence is in
  `OBSERVER-ROUNDTRIP-2026-11-18.json`; the runnable test is documented in
  [the preview README](../preview/README.md).

The isolated preview has a different origin and therefore no automatic access
to another port's browser save credentials. It asks for an existing save and
does not create a replacement. No token was copied out of browser storage.
Fixtures and production builds containing native art remain local/ignored.

The Android APK, real JVM-to-Rust service round trip and unexecuted native
instrumentation are recorded separately in [STATUS.md](STATUS.md). Browser
screenshots and JavaScript tests do not prove launcher widget installation,
Keystore execution, public HTTPS pairing or durable AFK service integration.
