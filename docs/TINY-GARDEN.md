# Tiny Chao Garden — phone companion and Rust port

Updated November 18, 2026. This records agreed scope and the current local build. Tiny Garden has shared server stats, a custom resident watch/care routine and five initial authoritative Rust minigame adaptations. All five games' controls and a 390×844 mobile viewport were checked without overflow; a complete Memory round earned a saved reward. Persistent residency is implemented/tested and the release is live, with resident UI acceptance starting. Native fidelity, complete touch playthroughs, device pairing, animal collection and AFK training controls with durable service integration remain unfinished. The full supplied GBA games have not been ported.

## Intended experience

Take the **same Chao** from the desktop adventure into a small phone garden. Check its real stats, feed and play with it, start a bounded training session, play short minigames and return to desktop with the results. Planned server-started training jobs will finish while the phone is closed. The bounded Rust TrainingLedger is tested, but its Tiny Garden controls and durable authoritative-service integration are not connected yet. The app should feel like the original Tiny Chao Garden/Chao Transporter relationship.

Keep the responsive browser Tiny mode and planned installable PWA, alongside the Android home-screen widget the user has selected. The widget and its authenticated synchronization are not implemented yet. Use a lightweight 2D garden with locally converted original art, touch controls and reduced audio/effects. **Mobile is strictly Tiny Garden:** care, stats, training, items and its five minigames. Desktop also plays Tiny Garden and keeps the full third person garden, stages, mechs, races and Karate. Both views use one server-owned Chao ID, inventory, progression and save record. The latest user priority is finishing Tiny Garden and its distinct Chaos Chao before extending normal stages.

The user explicitly identified and supplied **Sonic Advance, Sonic Advance 2 and Sonic Pinball Party for GBA**. All three are now inventoried locally. Their availability does not establish that their garden systems are identical or that any whole game is ported.

## Actual local evidence

`scripts/inventory-gba.ps1` reads supplied ZIP/raw GBA files with bounded decoding, identifies their headers and hashes them. It does not execute a ROM. Optional extraction uses generated game-code/hash filenames under ignored `.local/gba/`, never archive paths.

| Supplied source | Header identity | Decoded size | Verified evidence |
| --- | --- | --- | --- |
| Sonic Advance 2 | `SONICADVANC2`, `A2NE`, revision 0 | 16,777,216 bytes | Fixed/header checksum valid. SHA-1 `7bcd6a07af7c894746fa28073fe0c0e34408022d` matches the USA retail target listed by SAT-R/sa2. |
| Sonic Pinball Party | `SONICPINBALL`, `A3VE`, revision 0 | 8,388,608 bytes | Fixed/header checksum valid. SHA-1 `92b5c19cb4dee8fdfc15a8abdc699822d5373042`; version-specific internal offsets still require validation. |
| Sonic Advance | `SONICADVANCE`, `ASOE`, revision 0 | 8,388,608 bytes | Fixed/header checksum valid. SHA-1 `d842afa7dd1e84de08addb94a51506f1bcafd551`; this USA source differs from the Europe target listed by SAT-R, so use version-specific offsets. |

Full archive/ROM SHA-256 hashes, source paths and decoded output paths are in `.local/gba/inventory.json`. Raw ROMs, uploaded attachments, native saves, reference downloads and extracted/converted assets remain excluded from both source repositories and from generic development-file serving.

The primary SAT-R/sa2 research snapshot is commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`. Its `chao_garden` subproject identifies the embedded multiboot garden separately from the main Advance 2 game. Its dedicated `mb_chao_garden.sha1` lists `2c7da8e1c9af85986eecf4effae39197cdac9abf`; the subproject README repeats the main-ROM hash, so use the dedicated hash file when validating an extracted payload. We extracted the supplied Advance 2 payload at ROM offset `0xAED70`: 155,273 compressed bytes decode to 191,340 bytes and exactly match that garden SHA-1. All 256 native checksum-table entries match reflected polynomial `0x8408`; initialization `0xFFFF` and final inversion identify CRC-16/X-25. Local proof: `.local/gba/tcg-payload-proof.json`. A matching main-game build does not imply that every garden system or Pinball Party is documented.

The inspected garden save reader scans six flash sectors beginning at sector 10, checks header marker `0x1965`, validates a checksum and chooses the greatest stored generation. The native structure is 0x858 bytes. Many fields are unnamed in the reference and must remain opaque until verified. Native save checksum, field decoding and upload authority are separate concerns: a valid checksum is **not** proof of legitimate MMO progression.

## Native-system coverage and reuse

| System | Required audit | Intended adaptation |
| --- | --- | --- |
| Chao Transporter | Exact Chao/item/ring payload, handshake, bounds, success/failed-transfer and save ordering. | A phone/desktop mode handoff with explicit saved acknowledgement and one authoritative identity. Hardware JoyBus is reference behavior; it is not our browser networking protocol. |
| Chao save/type | Native type, alignment, appearance, stats, name, needs and any transferred opaque data. | Preserve full desktop identity even where the handheld renderer has fewer visual variants. No conversion to a normal Chao just because a sprite is missing. |
| Care and behavior | Pet/feed/food/needs, movement and learned reactions; distinguish garden versions. | Short touch interactions backed by server-owned items, needs, cooldowns and effects. |
| Fruit/shop/eggs/toys | Actual item IDs, original prices/effects, stock and transfer eligibility. | Shared inventory and Black Market catalog; spending and consumption are atomic. Proposed MMO pricing remains explicitly distinct. |
| Native minigames | Locate each game's complete state/input/scoring/reward rules, assets and audio. | Reimplement in Rust with touch presentation and server-validated results. Do not accept a client score or award rings from a local completion claim. |
| Native presentation | Tilemaps, palettes, sprites, animation scripts, text, UI, music and effects with source offsets/hashes. | Genuine compact phone garden art and original identity; keep full high-resolution desktop rendering separate. |
| Full GBA games | Main stages, physics, pinball tables, multiplayer, audio, menus and save dependencies need independent catalogs. | Preserve potential reusable systems as future scope; a Tiny Garden port is not a claim that either whole game has been ported. |

All five are required: **Chao Memory, Rock–Paper–Scissors, Chao Bounce, Chao Ball and Chao Search**. The supplied three ROMs cover the original sources for the first four; Search belongs to SADX's downloadable garden. Initial Rust adaptations for all five are connected to the authoritative service. All five controls and the 390×844 mobile layout have been checked; a complete browser Memory round earned 88 saved rings. Full playthrough/touch acceptance for the remaining games and native fidelity are separate work. [MINIGAMES.md](MINIGAMES.md) records source coverage, authority, saved rewards and UI/fidelity gates. These adaptations are not full ROM ports, and shared garden code must not erase each game's unique minigame.

## Chaos Chao on the phone

The user reports that native Tiny Garden recognizes Chaos Chao types but uses a shared-looking image. Treat that as a specific hypothesis to verify by tracing the native **type → palette/sprite/animation** lookup, including unhandled values and fallbacks. Record source offsets and representative inputs. Do not announce unused playable content solely from a type byte or a filename.

Regardless of native display limits, our companion should bring **Neutral, Hero and Dark Chaos Chao** to life, with distinct approved appearances, matching head effects and native-compatible movement. These are deliberate visual additions where native phone art is absent. Preserve original Chao proportions; create verified sprite renders from the corresponding native desktop model if needed, with provenance and an atlas manifest. Do not invent or overwrite a native Chaos form code.

`scripts/render-tiny-chao.py` now renders all three genuine desktop Chaos models into separate transparent 768×768 atlases: 96-pixel cells, eight directions, four native Idle and four native Walk frames. Native rigs remain unchanged; each action produces multiple distinct source poses. The sprites are a new phone presentation, not a claim that unused original GBA sprites were recovered. `.local/imports/tiny-chao-atlas.json` records source/output hashes, poses and layout. Browser composition/head effects and live visual acceptance remain separate checks.

The actual SA2 GBA-version Chao Transporter parts are mapped in `PC_SA2/ChaoCommon.ini`: button `E56784`, shutter `E56994`, casing `E57104`, base `E5A23C`, using `AL_OBJECT`. `scripts/extract-transporter.js` extracted all four native components and `al_object.prs`, verifying the executable/archive unchanged. Local manifest: `.local/extracted/transporter/manifest.json`. Assembly/conversion, placement/collision and the desktop handoff interaction remain queued; do not mistake the lobby's decorative exit transport platform for this garden machine.

The owner remains the existing perfect Dark Chaos Chao with the approved blue flame, emeralds, Air Shoes and Hammer Gloves. The user approved the portrait's chest gem, then explicitly requested a **fixed glowing chest emerald plus all seven floating emeralds**, with glow on the orbiting gems too. Use a separate genuine cyan model on an animated torso socket; preserve the complete approved orbit, individual native textures/facets/transparency and exactly five-times mounted orbit size. Keep emission/bloom subtle enough to retain the native colors rather than making white blobs.

## Animals, drives and the shared gameplay loop

The intended loop is **stage objective/pickup → animal or drive inventory → care/training → Chao stat/behavior/appearance progression → races, Karate and later missions**. Stage catalog/scenery availability does not yet mean live stages contain collectible animals.

Keep the original animal and drive systems in the same mechanics catalog as food and growth. For each native animal/drive record: source ID/name, release/pickup model, textures, motions, source stage/object references, verified stat changes, animal-part/behavior effects, removal/replacement rules and evolution/Chaos requirements. Preserve unknowns. Stat deltas, cosmetic traits and learned actions are different effects and need independent evidence.

The server creates each stage pickup, validates reach/eligibility, awards its stable item instance once and records ownership. Giving an animal/drive to a Chao consumes or changes its inventory state according to a chosen, documented rule and applies all effects in one durable transaction. Native reuse versus single-use MMO rules must be chosen explicitly; do not quietly turn a reusable original interaction into unlimited asynchronous stat farming.

Phone access must use the **same** item ownership and effects. Do not add an independent phone ring wallet or animal stock that can be uploaded later. Offline practice must not mint transferable animals, drives, eggs, rings or stats.

## AFK training — a new MMO rule

Native fidelity and the requested idle extension are separate. Timed offline/AFK training is an MMO adaptation requiring published balance. The following lifecycle is planned: its standalone Rust core is tested, while the training UI, persistent service wiring and device/activity lease remain TCG-06/08/16 work.

1. The authenticated player selects a skill and an allowed duration. The server validates eligibility, one-job-per-Chao, resource cost, limits and current activity.
2. The server writes a job ID, rules version, selected skill, start/end timestamps and any reserved costs. Client clocks and requested stat gains are ignored.
3. The job advances from server time even when the app is closed; a background browser timer is unnecessary.
4. Settlement calculates the bounded result on the server and atomically updates the job, skill points and save generation. Repeated settle requests return the same outcome; they never award twice.
5. Cancel/reconnect/restart/desktop return follow explicit policy. Partial rewards or refunds, if allowed, are calculated from server time and committed once.
6. Training must not run alongside another reward-bearing activity that conflicts with its chosen rules. A mounted vehicle, a desktop mission and a second device cannot independently drive the same Chao.

Start with bounded skill training, no offline currency generation and no unlimited stacked jobs. Exact durations, costs, point curves and caps are configurable tuning that require playtests. Store enough job state to settle a task after a service restart. Preserve the original seven skills and their limits; raising points does not silently grant S grades, owner privileges or Chaos form.

## Device handoff and saves

Production pairing requires accounts or a one-time, short-lived desktop-approved pairing flow. The phone receives its own revocable credential; a raw desktop bearer token must not appear in a permanent link, QR image, screenshot, analytics or page source. Current local browser session tokens are a development mechanism, not completed cross-device accounts.

Use a server-owned active-device/activity lease and profile revision. Entering Tiny Garden or returning to desktop freezes/confirms the old interactive state, settles eligible jobs, durably saves and issues the new lease. Old inputs carry an expired lease/revision and cannot keep moving, feeding or claiming rewards. Duplicate/retried requests use operation IDs and return the committed result. Lease expiry permits recovery without cloning the Chao or discarding earned progress.

The current service allows multiple connections to one canonical profile with last-input-wins; it does not yet provide this exclusive-device lease. Implement the handoff before mobile reward-bearing jobs go live. A native GBA `.sav`/`.chao` compatibility importer, if built, is an isolated local/practice path and must not overwrite an authenticated MMO character's stats or inventory.

## PWA and offline behavior

The phone's primary screens are Garden, Stats, Training, Minigames and Items. Use large touch targets, portrait layout, safe-area spacing, reduced-motion and mute controls. Show connectivity, a real last-confirmed save time, training end time and pending/settled status.

Planned offline behavior: cache only the versioned app shell and approved user-local presentation assets. Show cached stats as last-synced data. An offline user may view the garden/stats and play unranked practice; mutations/rewards need the server. Once training service integration is connected, a server-started training job will continue while disconnected. Do not queue arbitrary offline food/animal/spending/results and later trust them. Resume reconciles with the canonical server revision before enabling actions.

A phone cannot access a desktop's `127.0.0.1` service. LAN testing needs an intentional reachable address and device authentication; public use needs the planned HTTPS/WSS deployment and durable accounts/store. Desktop local import and phone asset delivery remain explicit deployment work.

## Ordered delivery and acceptance

| Gate | Concrete completion evidence |
| --- | --- |
| Local source identification | All three supplied archives hashed, exact headers checked, local-only inputs excluded from Git and file serving. |
| Rust native reader | Bounded header/LZ77/save parsing; authentic-source fixture comparisons; corrupt, truncated and oversized inputs rejected; unknown save bytes preserved. |
| Native catalog | Every discovered garden system, asset table and minigame has offsets/hashes and decoded/unknown/unused status per game/version. |
| Rust care and progression | Independently tested items/needs/animal/drive/stat effects with explicit native-versus-adapted tuning and once-only transactions. |
| Phone stats and handoff | Same Chao/profile before and after switching; exclusive lease; dropped/replayed requests, reconnect and server restart tested without duplicate items/rewards. |
| AFK training | Job survives restart; client-clock tampering has no effect; repeated claims/cancel/desktop return settle once; cost and reward change atomically. |
| Five phone minigames | Complete rounds and failure paths for Memory, RPS, Bounce, Ball and Search through server-owned inputs, scoring and once-only saved rewards; browser/touch acceptance for each. Controls/mobile layout and one full Memory saved-reward round are checked. These are initial adaptations, with native physics/art/constants tracked separately; offline practice awards nothing. |
| Chaos presentation | Neutral/Hero/Dark Chaos type mapping audited; distinct atlases verified for correct eyes, head effect, scale and animation; full identity survives desktop return. |
| PWA playtest | Real phone portrait/install/resume, slow network, cache/version update, controls, sound and battery/render performance checked. |
| Full original-games port | Separate subsystem-by-subsystem parity tests and runnable stage/pinball/game/menu/save paths. No status shortcut from an asset inventory or a successful Rust build. |

Track execution in [TASKS.md](../TASKS.md), the source system map in [MECHANICS.md](MECHANICS.md), and user intent in [REQUESTS.md](REQUESTS.md).

## Primary references

- Nintendo-hosted *Sonic Advance 2* manual: https://www.nintendo.co.jp/data/software/manual/manual_pcrj_00.pdf — Tiny Garden and GameCube transfer are original documented features.
- SAT-R/sa2 project: https://github.com/SAT-R/sa2 — main-game status, separate garden subproject and target ROM identities.
- Pinned garden save reference: https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/src/save.c
- Pinned garden save structure: https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/include/save.h
- Pinned JoyBus reference: https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/src/joy_bus.c

References are research inputs. Third-party source/art and supplied ROM contents are not copied into the public source tree. Record applicable attribution/license requirements before incorporating any external implementation.

## Watching and checking in

The same saved Chao can switch between direct play and a server-owned resident garden routine. Tiny Garden presents the server's canonical position, heading, needs and idle/wander/rest activity using the native Chaos atlas. Watch mode and minigames are mutually exclusive; taking control stops AI input before manual actions resume. Stale watch IDs cannot stop or care for a newer session. The Rust routine and persistent residency are implemented/tested and included in the live local release; resident UI acceptance is starting. This routine is an MMO adaptation, not a port of retail Chao AI.

Watch mode must not call reward-bearing feed/train/pickup paths or collect rings passively. Care intents are validated by the server. The user explicitly chose to leave the same Chao visible to other players while away. A durable resident opt-in keeps the canonical Chao in an allowed garden after the final owner disconnect, and a service restart restores one safe resident with a fresh watch session. This is continued simulation while the server is running; it does not fabricate elapsed AI activity while the service was stopped.

The user clarified that away-time competition means AI races and Karate. This is queued as COMPETE-01: deliberate opt-in entries, server-owned competitors and match simulation, eligibility and reward budgets, durable receipts and spectator/replay state. The implemented resident routine does not enter these matches or trigger player race/minigame rewards. AFK skill training belongs in Tiny Garden, but its UI and durable service integration are also queued; only the standalone bounded TrainingLedger core is tested. Client clocks and client-claimed results never produce gains.

