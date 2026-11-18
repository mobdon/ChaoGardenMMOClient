# Original Tiny Garden presentation

November 18, 2026. The full Tiny Garden entry reuses the reviewed five-screen
controllers from client PR #5. Garden, Stats, Training, Games and Items remain
views of one canonical Rust-owned Chao. This patch adds no game rules or service
contract, and does not authorize a live rollout.

`/tiny.html#garden`, `#stats`, `#training`, `#games` and `#items` select existing
screens on entry and hash changes. Other hashes cannot start activities. Menu
selection clears held movement and never starts a game, submits care, requests a
lease or takes control. When a round is active outside Games, explicit Return
and Stop controls remain visible. Stop uses the existing fenced handler; saving
and durably saved rounds do not expose that cancel shortcut.

The Garden canvas composes the original A2NE 176×160 map at x0 and the English
72×160 sidebar at x171 inside the original 240×160 screen. The final three
sidebar pixels are clipped by that screen. Both images are exported by the Rust
format-reader tooling and remain private, ignored local files. The allowlisted
presentation routes are `/__widget-art/tiny-garden-a2ne.png` and
`/__widget-art/tiny-garden-a2ne-sidebar-1.png`; they are not invented gameplay or
authentication APIs. Their source and hashes belong to the original-art
manifest. Missing art has an explicit accessible notice.

The inspected English sidebar contains labels, LV marks and a ring multiplier
mark, with empty dynamic fields. Name, rings, Mood (happiness), Belly (fullness)
and the five visible skill levels come only from the actual server profile.
Missing fields display an em dash. These are presentation projections of the
server's adapted MMO rules, not proof of native GBA progression. The existing
Stats screen retains all seven skills, including Luck and Intelligence.

Only a canonical profile with reported x/z coordinates draws a Chao in the
scene. Its existing original Idle/Walk atlas stays intact, with explicit
Hero/Dark/Neutral Chaos forms taking priority over alignment in browser and
Android. Unsupported art retains the identity-neutral marker. The complete
atlas cell spans 32 native screen pixels, with front feet on the original grass
anchor (109.12,108.8); other received positions are projected and bounded to the
small scene. Direction and walking/rest presentation use received server state.
The adapter does not simulate movement or extrapolate disconnected state.
Fresh canonical observations remain marked LIVE and animate even when another
device owns control. That observation status never enables care: the separate
action-permission check still gates every intent and disables the controls.

A literal canonical `admin:true` retains the same original fixed cyan chest gem
and seven orbit sprites as the widget. The existing presentation compositor
draws far gems, actual body/flame, fixed chest gem and near gems in order. The
owner keeps its native front anchor while original poses loop. Every gem must
be loaded at its verified square dimensions; otherwise the complete optional
group is hidden, the actual body remains, and an accessible notice explains the
missing ornaments. This visual flag grants no authority or progression.

Source checks cover the hash allowlist, no implicit mutation, active-round
Return/Stop, saving-result exclusion, original composition bounds, canonical
values and unknowns, native scale/grounding, position-driven rendering, and
unchanged bounded/offline care. Android's catalog regression also checks explicit
form precedence. These checks do not establish an installed Android widget,
physical-phone/PWA acceptance, native game-engine fidelity or durable AFK jobs.
