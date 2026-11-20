# Browser verification — November 18, 2026

The isolated branch preview runs at `127.0.0.1:5177/widget.html` from a managed
worktree outside `.local`. The original preview and Rust process remain on
their original ports. No private-source deny rule was weakened.

Executed checks:

- Current combined source: 95 passing Node tests, including eighteen widget artwork,
  projection, bounded-request and protocol/auth/identity tests. Art tests cover
  preview descriptors without identity/progression, canonical precedence,
  literal owner-role gating, the complete seven-gem group and native foot
  placement with square scaling. The formerly failing private
  source boundary passes from this worktree location.
  A real cross-origin HTTP 307 test confirms that resume-token bodies are not
  forwarded; all observer requests reject redirects. Tiny account/status/resume/
  pairing HTTP requests also reject redirects. A real HTTP 307 negative control
  confirms the Tiny account adapter forwards no credential and enables no controls.
  Entry-point reconnect tests also execute the production resume path and prove
  that once v2 is observed, missing/unsupported capabilities cannot send a
  credential to the legacy session or socket paths. Initial pairing discovery
  uses the same sticky protocol guard; a later confirmed v2 reconnect works.
  The two optional external-browser suites were excluded from this Node run;
  actual UI inspection below used the in-app browser.
- `node --test android/preview/preview-boundary.test.mjs`: passes. The explicit
  local-art routes reject wrong garden, Chaos-atlas and sidebar hashes without serving their contents, and
  encoded private paths stay blocked.
- `vite build --config android/preview/vite.config.mjs`: passes with the local
  art configured. All thirteen emitted originals (garden, English sidebar,
  three Chaos atlases, Dark flame, seven transparent geometry renders) match their pinned SHA-256.
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
- Follow-up scale adjustment: the complete square portrait now uses a 32px
  footprint at the garden's native resolution (20% of its displayed height),
  rather than separate fixed pixel limits. The visible body is about 20 native
  pixels high, a presentation adaptation comparable to a small handheld sprite,
  not a recovered GBA normal sprite metric. Native proportions and the grass
  foot anchor remain unchanged;
  the full frame, including flame/emerald margins, fits inside the garden.
  Compact/wide/large and the 390px layout were visually rechecked; the existing
  placement regression now checks scene-relative scale and all four bounds.
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

- The focused `widget.html?view=phone` keeps the phone mockup centered, hides
  marketing text, increases widget text legibility and retains settings in one
  native disclosure. Its toggle preserves the selected layout/profile. Desktop
  and 390px UI checks passed; the drawer expands/collapses, Art preview stays
  explicit, and `clientWidth=scrollWidth=375` at the narrow viewport.
- The reviewed Tiny app delivery from PR #5 is included as a source dependency.
  All five menu panels and hashes were checked in the actual browser. The Garden
  uses the pixel-verified original English sidebar; a new origin has no Chao,
  needs, levels or rings until authenticated. Training is disabled, Items asks
  for a canonical snapshot and all five minigames remain visible. The 390px
  native scene fits with no horizontal overflow. Temporary viewports were reset.
  A control-handoff regression keeps fresh observed values LIVE and received
  motion active while another device holds the lease, with care disabled. A
  real disconnect returns the same scene to cached status.
- The current production Tiny account adapter passed a fresh real Rust test:
  all five games started/stopped without cancellation gains; Memory earned 82
  durably saved rings and one receipt survived reconnect/restart with the same
  synthetic Chao. Sanitized evidence is in `TINY-MENU-ROUNDTRIP-2026-11-18.json`.
  This does not prove the four other complete reward rounds or native GBA rules.

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
