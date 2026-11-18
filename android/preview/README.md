# Original garden browser preview

This remains a browser preview of the widget presentation. Native packaging,
pairing and quick care are implemented separately under `android/app`.

Use a worktree **outside `.local`**. Vite correctly blocks private files under
that directory, including absolute module URLs generated from worktrees there.
Keep the private boundary intact. This preview config extends the existing
config and serves exactly one size-limited, hash-pinned PNG; it exposes no
directory or arbitrary native input.

Generate the background using [the Rust exporter](../docs/TINY-GARDEN-ART.md),
then run from the client worktree:

```powershell
npm ci
$env:TINY_GARDEN_ART_DIR = (Resolve-Path .local/imports).Path
$env:FRONTEND_PORT = '5177'
# Proxy the current local Rust service without restarting it.
$env:GAME_PORT = '3001'
npx vite --config android/preview/vite.config.mjs
# Open http://127.0.0.1:5177/widget.html
```

For a local production bundle use
`npx vite build --config android/preview/vite.config.mjs`. If configured, the
verified image is emitted at `__widget-art/tiny-garden-a2ne.png`. Generated
bundles containing native art remain local and ignored. Without the image the
page labels the unavailable original art, preserving the saved Chao's data.

Browser save credentials are origin-specific: a new port does not automatically
inherit a saved session from another port or browser. The preview resumes only
an existing local slot; it never silently creates a Chao. The integrated live
preview remains on its own port. This branch does not migrate its saves or
restart it. The observer discovers `/api/status`: legacy uses the existing local
resume contract; advertised accounts v2 uses bearer account reads and a queryless
WebSocket with first-frame authentication. Failed discovery pauses observation,
and an observed v2 service cannot silently downgrade. Observation takes no lease.

Run the production observer against an actual Rust accounts-v2 executable:

```powershell
node android/preview/verify-observer-roundtrip.mjs --server C:/path/to/chao-game-server.exe
```

This creates a fresh **synthetic** Chao in a new ignored `.local/verification/`
directory, checks the real HTTP and authenticated WebSocket snapshot, verifies
all seven stats/needs and no acquired lease, then restarts that isolated process
and resumes the same identity. It leaves the live service and user saves alone.
The sanitized receipt contains executable/source hashes and booleans, never
credentials or profile data. Use `--receipt path.json` to choose its output path.
This is service/adapter acceptance; it does not prove browser interaction or
an installed Android widget.

The checked [observer receipt](../verification/OBSERVER-ROUNDTRIP-2026-11-18.json)
uses the same exact executable hash as the native gateway's
[Rust round-trip evidence](../verification/RUST-ROUNDTRIP.md). The command does
not build or infer the provenance of an arbitrary supplied executable; inspect
its pinned build/source evidence separately.

The original background is 176×160, exported through the A2NE Rust port with
an exact ROM/payload fingerprint. The Chao overlay is the current canonical
desktop-model-derived phone atlas. Native GBA Chao type-to-art rules and full
minigame behavior are not established by this static art export.
