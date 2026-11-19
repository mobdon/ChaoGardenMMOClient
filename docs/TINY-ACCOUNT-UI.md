# Tiny Garden account-v2 adapter

November 18, 2026. The implemented client adapter follows the pairing
workstream's `src/accounts.rs` and `docs/ACCOUNTS-V2.md` contract. It is enabled
only when `/api/status.accounts` advertises version 2, activity leases, intent
sequences and `websocket-first-frame` authentication. Version 2 currently
advertises `durableTraining:false`; it does not enable the separate tentative
training UI proposal.

`src/tiny-account.js` exports `createTinyAccount` and `supportsTinyAccounts`.
The app creates the adapter with:

```js
createTinyAccount({
  fetch,
  transmit: message => sendOnCurrentSocket(message),
  onState: state => renderConnectionState(state),
  onSnapshot: snapshot => reconcileCanonicalPlayer(snapshot),
  onError: (text, code) => showAccountNotice(text, code),
});
```

`onSnapshot` runs for successful HTTP account reads, including reconciliation.
It does not run for `consume()` and cannot recursively re-emit socket snapshots.
The root app must fence callbacks from old sockets/connections by generation.
Callbacks and returned state contain no credential. Never export debug captures
of bearer headers or the authentication frame.

| Method/property | Behavior |
| --- | --- |
| `connect({token,status})` | Fetch bearer-authenticated `/api/account`, validate canonical identity/snapshot, renew an owned lease; reacquire an expired owned generation after another canonical read. Returns `{identity,snapshot,capabilities}`. |
| `send(message)` | Acquire/renew the correct lease, send one fenced WebSocket intent and await a matching durable acknowledgement. Concurrent gameplay inputs are dropped, never queued. |
| `consume(message)` | Accept current identity updates, acknowledge pending input, or reconcile protocol errors. |
| `takeControl(activity)` | Explicit user control request. The server decides whether an existing foreign lease is expired; the client never releases another device's lease. No gameplay intent is sent. |
| `disconnect()` | Disable actions and resolve uncertain pending delivery as unconfirmed. |
| `whenIdle()` | Wait for the current transport operation to finish; returns false if disconnected. Sends nothing. Useful for an explicit Stop gesture after clearing held movement and pausing new movement input. |
| `reconcile()` | Fetch the current account snapshot; never replay the interrupted mutation. |
| `ready`, `identity`, `snapshot`, `pending`, `version` | Read current presentation state. |

Browser sockets open `/ws` with **no query string**, then send exactly
`{type:"authenticate",token}` as their first frame. The first authenticated
snapshot acknowledges authentication; there is no separate auth acknowledgement.
The app routes each incoming message through the adapter before rendering.

Every gameplay intent carries the current `leaseId`, `expectedRevision` and
`identity.lease.lastSequence + 1`. The counter never resets on reconnect or
renewal. A new generation starts at the server's returned counter. Switching
owned activities releases the old lease before acquiring the next one. A foreign
lease makes the adapter read-only until an explicit `takeControl` succeeds.
The app must stop/finish the old runtime activity before switching; the backend
independently rejects incompatible runtime activity.

Only matching `intent-accepted` or `watch-saved` events resolve pending delivery.
`watch-saved.accepted:false` still confirms the persisted sequence, while care
effects and rewards remain separately server-authoritative. A plain snapshot or
stale acknowledgement cannot release a pending send. Save is lease-exempt and
transmits only `{type:"action",action:"save"}`. No profile state is uploaded.

Input confirmation expires after six seconds and triggers a fresh account read.
HTTP fetch and JSON parsing have a ten-second deadline and abort signal. No
timeout, conflict, revocation, disconnect or retry claims success or replays a
mutation. Client time schedules renewal attempts only; server responses decide
lease validity and never infer reward/game time from the browser clock.

Validation: `node --test tests/tiny-account.test.js` covers protocol/identity
validation, bearer-only credentials, first acquisition, durable serialized
sequences, stale acknowledgements, Watch no-op confirmations, activity changes,
reconnect/expired lease generations, foreign devices, explicit take-control,
fencing reconciliation without replay, exempt saves, disconnect, old-response
generation fencing and HTTP timeout. Real service/browser pairing acceptance is
run by the parent UI workstream against an isolated server fixture.
