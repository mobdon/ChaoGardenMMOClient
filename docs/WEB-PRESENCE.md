# Website, phone address and branding

Updated November 18, 2026. These are proposed addresses; no domain has been purchased or site publicly deployed.

| Address | Intended role |
| --- | --- |
| `chaogardenmmo.com` | Public project home, screenshots, setup/ownership instructions, news and play links. |
| `play.chaogardenmmo.com` | Desktop browser game, title screen, garden, stages and activities. |
| `tiny.chaogardenmmo.com` | Phone Tiny Garden PWA: stats, care, minigames and server-timed training. |
| `api.chaogardenmmo.com` | Rust HTTP/WebSocket service shared by both clients. |

The Verisign `.com` registry RDAP endpoint returned HTTP 404 for `chaogardenmmo.com` at **2026-11-18 09:32:50 UTC**. That means no registration record was returned by this lookup; registrar purchase availability, reserved/premium status and pricing are unverified and can change. Recheck with the chosen registrar when registration is actually requested.

The public page can ship independently of the game service. Game deployment needs HTTPS/WSS, production accounts/device pairing, a durable store/backups, a documented owned-asset workflow and measured instance limits. The current localhost preview is development infrastructure. Keep the client and server in their existing independent repositories. Original ROMs and extracted game content remain local rather than bundled into a public website/source release.

The phone and desktop sites use shared account/device authority through the API; subdomains do not share localStorage automatically. Do not copy bearer tokens through URLs as a shortcut. See [TINY-GARDEN.md](TINY-GARDEN.md) for pairing, leases, handoff and offline behavior.

## Native branding outputs

- `.local/imports/discord-icon.png`: 1024 × 1024, actual Dark Chaos portrait with original blue flame, seven genuine cinematic emeralds and native owner gear. Main elements fit a round Discord crop.
- `.local/imports/login-chao.png`: 1024 × 1280 portrait for the title screen.
- `.local/imports/brand-render-report.json`: source hashes and render provenance.
- `scripts/render-brand-assets.py`: reproducible local native-asset renderer.

These outputs are rendered from the owner's local game assets, not substitutes from the unavailable image-generation service. Keep them ignored with other converted game content. The local backend serves only their exact allowlisted PNG routes for browser presentation. A generated standalone original mascot can be evaluated separately if wanted later.

The title screen continues a chosen server Chao, remembers browser-local save-slot credentials and can create another Chao without discarding the old slot. Save & title waits for the server's durable `saved` acknowledgement. This is a local play flow; production Steam sign-in/ownership verification and cross-device accounts remain separate tasks.
