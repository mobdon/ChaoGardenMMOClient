# Tiny Garden training UI integration proposal

November 18, 2026. This is a client integration proposal, pending agreement with
the AFK training and device-pairing workstreams. It is not an implemented server
contract. The published service does not advertise this capability, so the app
shows an unavailable state with no invented duration, price, cap or reward.

`src/tiny-training.js` exposes `createTinyTraining({root, send})`. The caller passes
the canonical profile to `update(player, {connected, reconciled, busy,
capabilities, serverTime})`, and routes server events to `handleEvent(message)`.
`reconciled` must become true only after receiving a fresh authenticated snapshot,
not at socket-open. `busy` reflects conflicting care/minigame/device activity.
The controller exposes `supported`, `busy` and `blocksActivity` getters.

The proposed advertised value is
`tinyGarden.training: {protocolVersion: 1}`. Pass `status.tinyGarden` as the
controller's `capabilities`. An unsupported/missing version disables controls.
The proposed canonical `player.training` shape is:

```json
{
  "revision": 14,
  "rulesVersion": "server-configured-rules-id",
  "options": [
    {"id":"server-option-id","stat":"run","durationMs":60000,"costRings":3,"maxPoints":1}
  ],
  "job": {
    "id":"server-job-id","stat":"run","phase":"active",
    "startedAt":1800000000000,"endsAt":1800000060000,"costRings":3
  }
}
```

Those example values are schema illustrations, not balance defaults. The client
uses only the server's listed options. The seven recognized stat IDs are `swim`,
`fly`, `run`, `power`, `stamina`, `luck` and `intelligence`; the existing standalone
Rust core currently supports five. Unsupported stats are not added by the UI.
An idle state must explicitly contain `job:null`; an absent/malformed job never
becomes an assumed idle session.

Job phases are `active`, `ready`, `saving`, `settled` and `canceled`. The displayed
remaining time is `endsAt - serverTime` from the most recent confirmed snapshot,
labeled “at last sync.” There is no browser wall-clock countdown or elapsed-time
award calculation. Even when the displayed remainder reaches zero, only a server
`ready` phase enables Claim. An absent/backward timestamp displays no countdown.
Disconnected or unreconciled state disables every mutation.

The proposed intent fields are:

| Action | Fields beyond `type:"action"` and `action` |
| --- | --- |
| `training-start` | `operationId`, `optionId`, `revision` |
| `training-cancel` | `operationId`, `jobId`, `revision` |
| `training-claim` | `operationId`, `jobId`, `revision` |

Operation IDs use `crypto.randomUUID()`; the host transport is responsible for
the pairing workstream's current credential/lease envelope. Client messages
contain no time, score, points, price or gain. Pending requests are never queued
offline or silently replayed on reconnect. The server must enforce idempotency,
authorization, activity exclusion and atomic durable mutation independently.

The proposed durable acknowledgement is an event with `kind:"training-saved"`,
`saved:true`, matching `operationId`, `profileId`, `jobId` (for cancel/claim), and
an advanced `revision`. The UI remains pending until a canonical training
snapshot reaches that revision. `training-error` with the matching operation ID
ends the pending presentation without changing stats. A fresh canonical snapshot
after reconnect reconciles uncertain delivery without resubmitting the request.

A settled result is displayed only with
`result:{id,points,savedAt,saved:true}` on the canonical job. `saving` and unsaved
settlements block conflicting activities and another start. The profile stats
themselves always come from the host snapshot; the panel never patches them.

Validation: `node --test tests/tiny-training.test.js` exercises capability/version
gates, malformed state, all seven stat identifiers, strict intent contents,
offline/reconciliation exclusion, server time only, no inferred readiness,
phase-bound cancellation/claim, unsaved results, and stale/cross-profile/job
acknowledgements. Real AFK lifecycle acceptance remains dependent on the server
workstream and cross-device lease integration.
