# Chao Garden Online

A local browser RPG prototype in which you play as a Chao. Sonic Adventure 2 on Steam and its BATTLE DLC are requirements for the intended game. The local bridge imports assets from an installed copy; finding files is separate from checking Steam entitlement.

## Play locally

Use Node.js 22 or newer, then run:

```powershell
npm install
npm run dev
```

For a fresh source checkout, clone the separate server repository inside the client folder as `server/` and install its reference dependencies too:

```powershell
git clone https://github.com/mobdon/ChaoGardenMMOServer.git server
npm --prefix server install
```

The launcher starts separate frontend and backend processes. It uses the Rust release when built at `server/target/release`, with a Node reference fallback when no release is present. `CHAO_BACKEND_EXE` selects another Rust build; `CHAO_BACKEND=node` explicitly selects the reference. See [local development](docs/DEVELOPMENT.md) to build and test Rust.

Open **http://127.0.0.1:5173/**. Keep the terminal running. The first local profile is the owner; its five main grades are S, all seven levels are 99, and hidden Luck/Intelligence grades are 255. The owner can select Dark Chaos form in My Chao. Other browser profiles create regular characters. Session tokens stay in that browser; deleting browser storage loses its character access. Server profiles are saved in ignored `.local/profiles.json`.

The server binds to loopback. Multiplayer currently means multiple local browser profiles connected to this server. Internet hosting, shared accounts, Steam authentication, and production owner provisioning are future work.

## Controls

> The controls and the playable slice below describe the first web hub, which was removed from `main`. Its files are in the git history under `archive/old-hub` (last present at commit 420f1eb: `git show 420f1eb:archive/old-hub/README.md`). The live pages are the login page, Tiny Garden, the widget and phone previews, and the game page.


| Input | Action |
| --- | --- |
| WASD / arrow keys | Move relative to the camera |
| Right mouse drag | Orbit the third person camera |
| Mouse wheel | Zoom |
| Space | Jump |
| F | Attack nearby robots |
| E | Eat fruit at the grove |
| C / J / B | Character / journal / shop |
| R | Recenter the camera behind your Chao |
| Sound button | Enable original music and NPC voices |
| Stage select | Browse the game catalog and enter available areas |
| My Chao → Pilot Dark Chao Walker | Owner mounts/dismounts the native mech |
| F / hold Space while mounted | Fire / use the bounded jetpack |
| Click a portal or NPC | Enter its area or activity |
| Home | Return to the main hub |

Touch direction buttons appear on small screens. Modals stop movement while they are open; side panels do not.

## Current playable slice

- A main hub with access to Neutral, Hero, and Dark garden areas, a mission arena, and a checkpoint race.
- Original local Chao geometry with Idle/Walk animation, including special Chao NPCs and the owner’s Dark Chaos appearance. Procedural art remains for the hub layout, portals, arena, and course.
- Original garden geometry, textures, and a generated terrain heightfield for server collision. These are local converted files and are not tracked in Git.
- Combat and missions that award rings; races that validate checkpoint order and award rewards.
- Server-owned stats, ring pickups, alignment, garden access, purchases, equipment, and Chaos evolution milestones.
- A ring shop with fruit, training drives, and a saved eggshell cap.

The mission arena is a new prototype. Original Sonic stages, original enemy AI, original Chao Karate rules, and a complete evolution/morph system are not ported by extracting models.

## Local game asset pipeline

```powershell
npm run inventory
npm run inventory:all
npm run extract:native
npm run build:terrain
npm run convert:audio
```

`inventory` detects Steam libraries or uses `SA2_GAME_PATH`. The extraction and Blender conversion tools must be present under `.local/tools`; see [the pipeline guide](docs/ASSET-PIPELINE.md) for pinned tools, conversion commands, and material limitations. `scripts/convert-assets.py` runs inside Blender’s Python environment.

Original assets, converted GLBs/PNGs, tools, terrain, and profiles remain in ignored `.local/`. The source installation and original saves are read-only. Do not add this working directory to a deployment archive.

## Verification

```powershell
npm test
npm --prefix server test
npm run build
```

The client test runs the public/private file boundary in a standalone Vite instance. The server suites cover authoritative progression, forged client messages, movement limits and terrain, combat cooldowns/rewards, alignment gates, race validation, shop purchases, persistence, and fixed asset routes. Run `cargo test --locked` from the server repository for native Rust and real HTTP/WebSocket checks. The client sends input and action requests; the server decides their results.

The development dependency audit currently reports an unpatched `picomatch` issue through Vite’s file matching dependencies. The preview is restricted to local use; it is not configured as a public development server.

See [the game design](docs/DESIGN.md), [the task list](TASKS.md), [the request checklist](docs/REQUESTS.md), [the complete inventory](docs/INVENTORY.md), and [local Rust development](docs/DEVELOPMENT.md) for scope, verification, and remaining work.

[The Tiny Garden plan](docs/TINY-GARDEN.md) covers the supplied three GBA sources, Rust native-system port, phone stats/minigames/AFK training, shared animals/drives and safe desktop handoff. These phone systems are in development. [The website plan](docs/WEB-PRESENCE.md) records the proposed desktop/phone/API addresses, native branding outputs and title/save-slot behavior.

The source is split between [the client repository](https://github.com/mobdon/ChaoGardenMMOClient) and [the server repository](https://github.com/mobdon/ChaoGardenMMOServer). In this workstation layout, the separate server checkout lives at `server/` and is excluded from the client repository. See the task list for Rust migration, original movement/karate/race integration, Dark Chao Walker piloting, and the staged 100–500 concurrent-player target.
