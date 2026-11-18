# Tiny Garden UI delivery and acceptance

November 18, 2026. Workstream 1, `codex/tiny-garden-mobile`, originally based on
client `af74a8d`. This is browser presentation and bounded protocol adaptation
for the existing Rust authority. It adds no TypeScript/JavaScript game engine,
progression rules, inventory authority, trusted timer or reward calculation.
The generated Expo/NestJS/simulation deliveries remain unaccepted.

## Delivered behavior

- Garden, Stats, Training, Games and Items screens retain one canonical Chao.
  Seven skills show independent server levels, points and grades. Explicit
  Neutral/Hero/Dark Chaos forms take precedence over conflicting alignment;
  original atlases, native poses and the Dark flame remain unchanged.
- All five existing Rust minigames use their public views and bounded intents.
  Touch release/cancellation, navigation, focus loss and reconnect stop held
  input. One-shot inputs are serialized around acknowledged v2 movement.
  A truthy save flag, wrong round ID or changed client clock cannot show an
  earned result as saved. The wallet changes only through canonical snapshots.
- Controls wait for the first canonical snapshot, including after reconnect.
  Save and desktop return wait for an actual save acknowledgement. Last-sync
  and save-confirmation times explicitly identify this device's observation
  time; simulation time is never presented as a wall-clock save date.
- The production-only service worker controls `/tiny.html`. It caches the
  public shell, versioned build modules and an exact presentation-art allowlist.
  It never intercepts account/session/intent/pairing APIs, mutations, query
  credential URLs or third-party responses. Cached public stats are view-only
  and marked last synced. There is no offline mutation queue or reward upload.

## Rust contract boundaries

| Path | Actual support and remaining gate |
| --- | --- |
| Published development Rust service | Resume-only session, canonical snapshots, Watch pet/rest/wake, explicit Save and all five games. The legacy credential URL is restricted to successfully discovered loopback development servers. Discovery failures never downgrade authentication. |
| Isolated Rust accounts workstream | The optional adapter consumes its exact `docs/ACCOUNTS-V2.md` implementation: bearer HTTP, first-auth-frame WebSocket, revisions, leases and sequence acknowledgements. Viewing acquires no new lease; actions request their activity. Foreign-device control needs an explicit request and server approval. This is an unmerged contract dependency, not a claim of production pairing or coordinated desktop/widget rollout. |
| Authoritative-care PR | Exact owner-only `snapshot.care` with `rulesVersion:"adapted-care-v1"` supplies quantities/catalog/eligibility. Compatible local care uses the exact UUID, original revision and intent on manual retry. No local quantity/stat delta is applied. Account-v2 care stays disabled because the two branches have not integrated their revision/envelope semantics. |
| AFK training | The current Rust service and accounts capability do not advertise durable jobs. The Training screen honestly shows unavailable, with no invented cost, duration, cap, countdown or gain. `TINY-TRAINING-UI.md` is a disabled integration proposal. A tested standalone Rust ledger is not live service acceptance. |

The server coordination board owns contract agreement and rollout. Preserve the
native locomotion asset route and later upstream history when integrating the
older accounts/care bases. This UI does not modify server authority, live saves,
credentials, native input files or the running preview.

## Reproducible checks

`npm run build` builds all existing entries; the pre-existing large desktop
bundle warning remains. Tiny Garden does not import Babylon or the desktop
simulation. Ordinary Node tests cover strict receipts, native atlas selection,
inventory/eligibility, account fencing, clock isolation and offline boundaries.

After rebasing onto published client `435f6a3`, the production build passed and
the isolated suite passed 86 tests with zero failures/skips: 62 source checks,
20 touch/browser cases and four production-shell PWA cases. The unchanged
private-source-boundary test separately passed in the primary checkout (one
test). That worktree-path distinction is explained below.

The optional browser suites require an installed Playwright module and browser:

```powershell
$env:TINY_PLAYWRIGHT_MODULE = '/absolute/path/to/playwright/index.mjs'
$env:TINY_BROWSER_CHANNEL = 'chrome'
node --test tests/tiny-browser.test.js tests/tiny-pwa-browser.test.js
```

The fixture suites use isolated ports and synthetic credentials, never the
preview. They verify actual touch events, all five primary/secondary controls,
reconnect reconciliation, strict save acknowledgements, care retry identity,
v2 authentication/ownership, and 320/390/430/844-pixel layouts. Native-form
screenshots use only allowlisted user-local assets and remain ignored.

The real Rust check requires an existing independently tested executable:

```powershell
$env:CHAO_TEST_SERVER = '/absolute/path/to/chao-game-server.exe'
node scripts/verify-tiny-live.mjs
```

It creates a new isolated data directory and ephemeral port, with owner
bootstrap disabled. It starts/stops all five games without cancellation
rewards, solves Memory using only its public peek/shuffle observations, then
checks the durable wallet/receipt through reconnect and a restart of that test
child. It never defaults to a preview address or reads a live profile.

An initial verified run against runtime SHA-256
`9a717fee289a2ad0de05404939d327079b81727a0b1474c398999e901f7f60be`
earned 90 saved rings and retained one Memory receipt after reconnect/restart.
This is evidence for the published adapted game rules, not native ROM parity.

The final runner, which also waits for the real adapter's durable acceptance,
passed again against that legacy executable and the isolated accounts-v2
executable. Each earned 82 rings and retained one receipt through reconnect and
restart. V2 executable SHA-256:
`4317b6d1e887b71274e484ceff3245da86cc86fa74f2d9155ad1692caeef46e1`.
The observed accounts source checkout was clean at
`56af22dece8551376f4c8b9a1882acecfff02bf1`; this remains isolated, unmerged
source, not an accepted rollout. The runner records executable, adapter and
observed-source hashes with its result under ignored `.local/tiny-live-*`.
Set optional `CHAO_TEST_SOURCE` to record the corresponding source checkout.

The existing private-source-boundary test passes in the primary checkout. A
worktree below `.local` intentionally matches Vite's private-file deny glob;
running that unchanged boundary test there returns 403 for its own source.
The isolated browser harness deliberately omits preview proxying and uses a
separate fixture configuration. No production file boundary was weakened.

## Open acceptance

Durable Rust AFK jobs, integrated v2 care, coordinated desktop/widget transport
migration, physical-phone install/resume, complete earned/failure playthroughs
of RPS/Bounce/Ball/Search, and native game fidelity remain open. Browser touch
tests and the Memory durable service result do not establish those gates.
Publication of this source does not authorize runtime rollout or migration.
