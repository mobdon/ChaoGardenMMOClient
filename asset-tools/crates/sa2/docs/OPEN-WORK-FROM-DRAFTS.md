# Open work carried over from closed draft PRs (November 21, 2026)

mob asked (November 20, 2026, 8:16 PM MT) to close the remaining GPT/Codex
draft PRs and keep anything still undone in the plan instead. Nothing here was
merged. The branches still exist, so the code can be read from them. This file
is part of [PORT-PLAN.md](PORT-PLAN.md).

| Closed PR | What it held | Status | Where it continues |
| --- | --- | --- | --- |
| Client #476, Server #38 | Review indexes listing every GPT/Codex branch | Done: every PR they listed is merged, closed or owned by a thread | Nothing left |
| Client #466, Server #32 | Snapshot of the native Chao Race/Karate branch `claude/project-thread-htbtav` | **Not in main.** Karate runs natively end to end. Race end to end, course paths, cheering, ring-out, prizes and the loss path are still to do. Paused by mob | Phase 8, thread "Port Chao Race and Karate natively". Push its own branch when resumed |
| Client #467 | Snapshot of the characters branch `claude/project-thread-ytb9xh` (Knuckles/Rouge hunter exec 0x8009F8A0 and others) | **Not in main** | Workstream C / Phase 4, thread "Port the other playable characters" |
| Server #15 | Rust accounts, Android pairing, activity leases and save migration (`codex/accounts-pairing-ownership`, 56af22d) | **Not in main.** Its own notes say to keep it unmerged until the desktop, Tiny Garden and widget auth adapters pass together | MMO extras (not native). Rebase onto the current server main, add the client adapters, then review |

Most of the game logic now comes from the static recompiler (`sa2-recomp`,
merged in #502), so hand-port snapshots like #466 and #467 matter mainly for
the server side (race/karate) and as reference.
