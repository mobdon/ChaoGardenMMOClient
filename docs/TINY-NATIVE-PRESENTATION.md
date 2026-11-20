# Original Tiny Chao Garden presentation

This isolated change reuses the existing Rust A2NE resource port from client
commit `53fc910`. It reconstructs the original 240 × 160 garden background and
English stat panel. It does not replace them with hub grass or a newly drawn
garden. All decoded artwork stays in ignored local storage.

`asset-tools/crates/tiny-garden-presentation` uses the verified cartridge and
garden hashes, cataloged LZ77 streams, original palette words, native VRAM
destinations, text-map biases, BG priorities, scrolling and mixed 4bpp/8bpp
layers. Setup is traced from `InitGarden` and its loader in the locally pinned
reference `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. The initial display word
is the deferred `0x1f40`, rather than the disabled setup register. Original
code and assets are not copied into tracked source.

The exporter defaults to the second native panel variant, visibly labeled in
English. Both panel variants are tested. Dynamic native OBJ and number-tile
generation are outside this static background slice.

`src/tiny-native-garden.js` is a reusable presentation adapter. Pass the decoded
background image, the real canonical profile and the existing `drawTinyAvatar`
callback. It retains the original panel labels and displays supplied mood,
fullness, the five native visible stat levels and rings. Missing data stays
unknown. All seven canonical stats, including Luck and Intelligence, remain in
the existing app's stats interface. Pass `panel: false` for a garden-only widget
window. Aspect ratio is preserved, and there is no generated substitute Chao or
local simulation. Existing portrait rendering remains responsible for native
forms, proportions, flame, chest gem and seven-emerald identity.

## Reproduce locally

From this isolated worktree, activate the original checkout's portable Rust
toolchain using its absolute `scripts/use-rust.ps1` path. Use a fresh existing
`.local` output directory; exports reject traversal, symlink/junction ancestors
and existing filenames.

```powershell
$env:A2NE_ROM = 'C:/path/to/local/A2NE-f2b7a3329401.gba'
$nativeCrate = 'asset-tools/crates/tiny-garden-presentation/Cargo.toml'
New-Item -ItemType Directory .local/native-background
cargo run --offline --locked --manifest-path $nativeCrate -- $env:A2NE_ROM .local/native-background
cargo test --offline --locked --manifest-path $nativeCrate -- --include-ignored
cargo clippy --offline --locked --manifest-path $nativeCrate --all-targets -- -D warnings
cargo fmt --manifest-path $nativeCrate -- --check
node scripts/verify-tiny-native-garden.mjs .local/native-background/tiny-native-background.png C:/path/to/local/imports 'C:/Program Files/Google/Chrome/Application/chrome.exe'
```

The visual check executes the actual adapter and existing portrait renderer in
an isolated headless browser profile. It uses the existing local Chaos atlases
and flame. Its Dark Chaos appearance is an artwork fixture with unknown stats,
not a live profile. It requires the browser to finish image decoding and drawing
before declaring success. The page embeds its images and sends no game requests
or commands.

## Integration handoff and remaining limits

The accompanying `integration/tiny-native-watch.patch` is a proposed small patch
for the shared Watch entry point. It has not been applied to another workstream.
It removes the custom grass and calls this adapter using the canonical profile.
Its projected movement is presentation of the existing server routine; native
GBA care, collision and AI are not implemented by that projection.

Before applying that patch, integration must add exactly
`tiny-native-background.png` to the existing Rust service's PNG asset allowlist
and place the private exported PNG in that runtime's local imports directory.
This reuses the real static asset endpoint; no new gameplay API is proposed.
The route is a server-repository patch and is deliberately not applied here.
The patch clearly shows an unavailable-art state if the original is missing.

The widget workstream can use the same decoded image and renderer for its
browser layout. Android must compose the same image with the authenticated
canonical portrait in its native bitmap adapter, preserving its real care,
pairing, cache and activity contracts. This contribution does not build, install
or test an APK, and no existing Android checkout is modified.

Recorded November 18, 2026: three new Rust tests passed, including real-source
rendering of both panels and modified-ROM rejection; strict Clippy and formatting
passed. The English RGBA fingerprint is
`7f4af9cab17d2f44c57208630f46577a9488e599f57eda2673c0840a75d721ce`.
This is a regression fingerprint computed from the recovered composition,
not independent retail screenshot or gameplay parity. The original background
and browser composition were visually inspected.

No canonical service round trip, care transaction, native minigame fidelity,
native AI, audio, installed widget, full game port or production rollout is
established by these presentation checks. Existing Rust server authority,
live preview, credentials and saves were not changed. This is a verified
original-art presentation slice awaiting integration, not a completed app.
