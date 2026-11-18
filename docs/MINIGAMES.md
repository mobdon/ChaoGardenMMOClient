# Tiny Garden — all five minigames

Updated November 18, 2026. The user explicitly requested **all five**, on desktop and in the Tiny-only phone client. The first Rust implementations reproduce the documented game loops with clearly identified adaptations; exact original layouts, physics, timing, art and reward fidelity remain separate validation work.

The reusable logic is in the separate server repository at `server/crates/tiny-garden-games`. The crate README records exact public view/input types, evidence and unresolved rules. It contains no original ROM or assets.

| Game | Original source coverage | Required complete loop |
| --- | --- | --- |
| Chao Memory | Present across the three supplied GBA sources. | Memorize seven pairs, track visible card pushes, optional one-time trip, match cards, position-based rings, three-mistake limit and clear bonus. |
| Rock–Paper–Scissors / Janken | Supplied Sonic Advance. | Select and shoot hand cards at moving targets; server resolves the shot and gesture, replacements, timer, wave clears and rings. |
| Chao Bounce | Supplied Sonic Advance 2. | Move the spring, catch/bounce the Chao, time stronger jumps, collect rings, handle balloons and complete levels before losing three lives. |
| Chao Ball | Supplied Sonic Pinball Party. | Fire red/green balls, flip trapped colors, clear connected groups, catch released rings and keep the descending board out of the loss area. |
| Chao Search | Original SADX downloadable garden; no corresponding original payload supplied yet. | Follow hints, find/pick up the requested Chao, return it to the entrance, gain rewards/time or incur the wrong-return penalty, finish the requested set before timeout. |

Do not interpret five entries in a menu, five source files or a core unit test as five finished phone games. Acceptance requires visible renderers, usable touch controls, complete input-to-result play and saved server rewards for each entry.

## Authority contract

The Rust host generates the round ID and private seed, binds the round to the authenticated canonical profile and advances it from host simulation time. Clients submit bounded controls only. Hidden boards, requested-target identities, future order and seeds are not serialized into public views. Native initial peeks and revealed clues remain intentionally visible gameplay.

- Start: `{type:'action', action:'tiny-start', gameId}`.
- Control: `{type:'tiny-input', sessionId, input:{kind,...}}`.
- Stop: `{type:'action', action:'tiny-stop', sessionId}`.
- Own snapshot: `tinyGame:{id,gameId,phase,startedAt,view,result?}`; the game-tagged `view` is the sanitized core view.
- Saved result: `tiny-game-result`, emitted only after durable reward/receipt persistence succeeds.

Game IDs: `memory`, `rock_paper_scissors`, `bounce`, `ball`, `search`. Exact schemas belong to the crate and server documentation. Unknown/surplus fields, score/time/result claims, wrong-profile IDs and stale round IDs are rejected. Main-game movement and incompatible reward activities are blocked during a round. Shared sockets use one profile/round simulation rather than each producing a reward.

A completed round enters a saving state. Its wallet change and unique result receipt are projected into the same atomic persisted profile record; only a successful flush applies the spendable live credit and emits the saved result. Save failure retains the pending result, prevents replacing it with a new round and permits a safe retry. Last disconnect cancels unfinished play but retains a pending completed result. Do not present a local animation as a saved result.

The first live economy caps wallet credit at **100 rings per completed round**, with the existing wallet cap also enforced. The core-earned value remains separate from credited rings. This cap is our MMO balance rule. The Search guide's increasing seven-reward description conflicts with its stated 105-ring maximum; the initial core uses the stated increasing schedule behind a configurable table, while native fidelity remains unresolved.

Simulation currently steps at a fixed host tick and can stretch game time when an overloaded server skips ticks. This prevents a client from accelerating it but still requires performance/elapsed-time policy validation before competitive play. It is not a measured 500-player capacity claim.

## Evidence and current gates

- Five complete initial Rust loops; **22 crate tests** and strict Clippy pass. An external public-view-only Memory solve demonstrates real legal play and a once-only positive result.
- Rust host integration and durable-save tests pass, including a forced save failure/retry. Independent review added two passing checks for cross-profile view/control isolation, replaced IDs and stale movement.
- Phone renderers/touch controls and real browser playability are the current acceptance gate. Original GBA graphics/audio extraction and accurate native constants remain follow-up work.
- The mobile client is strictly Tiny Garden. Full stages, hub exploration, mechs, races and Karate stay in the desktop client. Desktop may open the same Tiny Garden games.

## References supplied by the user

- [Chao Island — all five](https://chao-island.com/guides/spin-off-games/tiny-chao-garden/mini-games/), with separate first-hand mechanics pages for each game.
- [Tiny Chao Garden DX by domobot](https://domobot.itch.io/tiny-chao-garden-dx), a GameMaker recreation for Windows/Android, useful for interface/inventory/transfer comparison. Its creator page lists Match and Janken; the October 7, 2026 devlog still describes two minigames. Do not assume its download supplies all five or original SADX Search code.
- [DX 0.5.0 devlog](https://domobot.itch.io/tiny-chao-garden-dx/devlog/1064721/tiny-chao-garden-dx-050-is-here), October 2, 2025, describes Android and shared-network transfer.
- [DX project status](https://domobot.itch.io/tiny-chao-garden-dx/devlog/1694858/where-chao), October 7, 2026.

No reusable source/license was linked on the inspected DX creator pages. Use its published behavior as a reference; copying a GameMaker download into Rust is not a source port. External code incorporation requires its applicable license/attribution. No files were uploaded to or messages sent to the creator.
