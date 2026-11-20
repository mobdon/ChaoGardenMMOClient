# Expo phone app scaffold (moved from the server repo)

Moved unchanged from mobdon/ChaoGardenMMOServer `apps/mobile` and its
dependency `packages/contracts` (server main de640b8). It came from the Nov 18
Vibe batch (server PRs #4 and #13). Phone and Android presentation belongs in
the client repo.

Status: **unaccepted**. The server's `docs/coordination/VIBE-REVIEW-2026-11-18.md`
records why. VIBE-18: it can't launch as delivered, because there's no `index.js`
entry, `styles` is declared twice, and an import is missing. VIBE-19: its contract
doesn't match the Rust server's real session and WebSocket protocol. It also uses
placeholder art. Don't build game mechanics on it. Game rules stay in the Rust
server.

It isn't part of the Vite build or `npm test` at the root.
