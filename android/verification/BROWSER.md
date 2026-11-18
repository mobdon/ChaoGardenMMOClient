# Browser verification — November 18, 2026

The isolated branch preview runs at `127.0.0.1:5177/widget.html` from a managed
worktree outside `.local`. The original preview and Rust process remain on
their original ports. No private-source deny rule was weakened.

Executed checks:

- `npm test`: 28 passing client tests, including eighteen widget artwork,
  projection, bounded-request and protocol/auth/identity tests. Art tests cover
  preview descriptors without identity/progression, canonical precedence,
  literal owner-role gating, the complete seven-gem group and native foot
  placement with square scaling. The formerly failing private
  source boundary passes from this worktree location.
  A real cross-origin HTTP 307 test confirms that resume-token bodies are not
  forwarded; all observer requests reject redirects.
- `node --test android/preview/preview-boundary.test.mjs`: passes. The explicit
  local-art routes reject wrong garden and Chaos-atlas hashes without serving their contents, and
  encoded private paths stay blocked.
- `vite build --config android/preview/vite.config.mjs`: passes with the local
  art configured. All twelve emitted originals (garden, three Chaos atlases,
  Dark flame, seven transparent geometry renders) match their pinned SHA-256.
  The three local atlases and flame also returned their exact hashes while the
  Rust artwork proxy was unavailable. Existing main-bundle size/runtime-asset
  warnings remain.
- After the final deterministic emerald manifests were loaded by the isolated
  preview, all seven live preview gem routes returned 200 with exact final
  hashes. Manifest changes require restarting that preview configuration.
- Actual in-app browser: compact, wide and large controls switch correctly;
  the original garden and Dark, Neutral and Hero Chaos art render while
  disconnected. The Dark flame is visible; feet land on the grass. Art preview
  labels stay visible, needs/levels/rings remain unknown and AFK explicitly
  unavailable. At a 390×844 viewport, the
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

The isolated preview defaults to a labeled Dark Chaos art descriptor containing
only form/alignment. Neutral/Hero selection is artwork only; canonical saved
profiles always take precedence. Canonical owner ornament rendering is covered
by helper tests; this browser check did not use the live owner's credentials or
claim a connected owner screenshot. The emerald sprites are transparent renders
of unchanged original geometry/materials, not opaque environment textures or
recovered GBA sprites. Provenance is in `android/docs/WIDGET-EMERALDS.json`.

The isolated preview has a different origin and therefore no automatic access
to another port's browser save credentials. It asks for an existing save and
does not create a replacement. No token was copied out of browser storage.
Fixtures and production builds containing native art remain local/ignored.

The Android APK, real JVM-to-Rust service round trip and unexecuted native
instrumentation are recorded separately in [STATUS.md](STATUS.md). Browser
screenshots and JavaScript tests do not prove launcher widget installation,
Keystore execution, public HTTPS pairing or durable AFK service integration.
