# Android widget integration with Rust accounts v2

Updated November 18, 2026. Runtime source: the independent server workstream `codex/accounts-pairing-ownership`, `src/accounts.rs`, `src/identity.rs` and `docs/ACCOUNTS-V2.md`. The published server baseline `a8188a3` lacks these endpoints. Local source inspection is not deployment or installed-device evidence. This client introduces no new authoritative gameplay endpoint.

## Authentication and pairing

All requests use one explicit HTTPS origin. Reject userinfo, non-origin paths, queries, fragments, HTTP and redirects. Only known account endpoints and allowlisted PNG paths are requested. Credentials travel exclusively in the authorization header; never URLs, logs, bundles or screenshots. A reverse proxy must redact authorization and pairing bodies. Device responses must use `Cache-Control: no-store`.

Desktop creates an approved invitation with bearer authentication:

```json
POST /api/pairing
{"expectedRevision":40}
```

Response: `{pairingId,pairingSecret,expiresAt,revision}`. Invitations expire after 120 seconds and are single use. The Android claim has no desktop credential:

```json
POST /api/pairing/claim
{"pairingId":"invitation-id","pairingSecret":"short-lived-secret"}
```

Response: `{token,deviceId,identity}`. Identity has `schemaVersion:2`, `accountId`, `chaoId`, `deviceId`, `revision`, `devices` and nullable `lease`. Top-level/nested device IDs must match. Android strictly validates revision/schema numeric types and stores the pairing baseline so later snapshots cannot roll back below it. Keystore encryption covers credentials, private snapshots and pending care. Backup is disabled. A lost successful claim response requires a new desktop invitation; there is no blind claim retry.

Revocation belongs to authenticated desktop `POST /api/devices/revoke` with `{deviceId,expectedRevision}`. Android Forget is local deletion, with that limitation shown in the app. Tokens are separately revocable; identity v2 does not yet define token expiry, restricted widget scopes or password recovery. Android does not exercise administration, movement, minigame or training control through this credential.

## Read-only canonical snapshot

`GET /api/account` with `Authorization: Bearer <device token>` returns:

```json
{
  "identity": {"schemaVersion":2,"accountId":"account","chaoId":"chao","deviceId":"android","revision":42,"lease":null,"devices":[]},
  "snapshot": {"type":"snapshot","protocolVersion":2,"selfId":"chao","serverTime":1234.5,"identity":{},"players":[]},
  "capabilities": {"version":2,"httpIntents":true,"activityLeases":true,"intentSequence":true,"durableTraining":false}
}
```

This is a structural illustration, not a valid fixture: real nested identity must match the outer identity and `players` must contain exactly one matching canonical self. Never fall back to the first player, create a Chao or reinterpret a missing player as a fresh profile. `AccountAdapter` rejects incomplete/private identity mismatch and malformed/out-of-range needs or seven-skill data. Internal display identifier `tinygarden.widget.v1` is only a native projection tag, not a wire protocol or extra server endpoint.

`player.hunger` means fullness. Preserve all seven skills: Swim/Fly/Run/Power/Stamina grades 0–5 map E–S; Luck/Intelligence grades remain 0–255 numeric. Levels are 0–99, points 0–9999. Show authoritative rings, form, garden zone and Watch activity. Snapshot simulation `serverTime` is not Unix UTC and is never displayed as save/real-world time.

The current server has no truthful last-saved UTC or durable training job. Internal `serverTimeMs` and `lastSavedAtMs` remain explicit null, and AFK status is unavailable. The Read timestamp is the Android receipt time and makes no durability claim. Even a future `durableTraining:true` does not imply a known job schema; add/validate that contract before presenting phases, costs or end times. Local clocks never advance a phase or grant points.

Server draft PR #15 at `56af22d` includes both private `snapshot.self` and exactly one matching canonical player even when offline without a resident. The obsolete include-self patch is removed. This adapter requires the canonical player and fails closed on older incomplete snapshots; it never selects a substitute.

## One-tap Watch care

Only explicit user taps may take a lease. Background reads never acquire, renew, start Watch or interrupt an activity. Current Rust Watch supports `pet`, `rest`, `wake`, current Watch ID and its cooldown; it has no feed/inventory transaction. Compact offers Pet; wide/large offer Pet and Rest/Wake. Minigames and training controls open the full app.

Care requires an existing current Watch session, current identity revision, supported HTTP/lease/sequence capabilities, the same recent native snapshot/process, and a recent user gesture. A fresh account read that changed revision/Watch causes refresh and a new user decision rather than rewriting the old tap onto a new state.

If needed, acquire a 30-second watch lease with bearer authentication:

```json
POST /api/lease
{"operation":"acquire","activity":"watch","expectedRevision":42}
```

Response `{identity}` supplies the new revision and lease `{id,deviceId,activity,expiresAt,lastSequence}`. Rust validates exclusivity and activity; Android cannot replace an unexpired conflicting lease. Client wall time can only decide whether to try acquire; Rust time decides whether it succeeds. If Android already owns a valid watch lease it uses that generation. It never switches an existing own non-watch activity to care.

Persist the exact pending envelope encrypted before sending:

```json
POST /api/intent
{
  "leaseId":"watch-lease",
  "expectedRevision":43,
  "sequence":1,
  "message":{"type":"action","action":"watch-care","sessionId":"current-watch-id","kind":"pet"}
}
```

Rust commits sequence/profile state atomically. A Watch acknowledgement is `{type:"event",kind:"watch-saved",sequence,identity,accepted,watch,serverTime}` after persistence. Validate identity, lease and sequence. `accepted:false` means no gameplay change, such as cooldown denial; accepting transport never implies an effect/reward. Android obtains a fresh account after the acknowledgement and displays only that account state. It releases only a lease acquired for this tap using `{operation:"release",leaseId,expectedRevision}`; release failure is left to server expiry/reconciliation.

No automatic mutation retry exists. An uncertain network/proxy/malformed response retains the original pending lease/revision/sequence and disables further care. A fresh authoritative read resolves/fences it when the sequence was committed, revision advanced, or lease generation changed/cleared. A generic HTTP 4xx alone is not proof the mutation never committed. If the state remains ambiguous, keep it pending and use the full app/account recovery path. Old queued work is dropped after 15 seconds, after process change, after redelivery or after the OS stops it. Expiry is checked again during dispatch. It never becomes a delayed offline food/action queue.

## Original artwork and app entry

`ORIGINAL-ASSETS.json` records the actual existing local atlas/model/image hashes without copyrighted contents or private filesystem paths. Native art is fetched from the same HTTPS origin, hash-checked before bounded decoding, and cached against canonical Chao plus appearance. Atlas <=1024px, flame <=512px and individual gems <=256px bound decoded memory. The original flame black matte is converted to coverage. Far gems render behind the opaque body; the fixed chest gem stays separate from all seven orbit gems.

Canonical Chaos form/alignment selects the original source variant; only literal canonical `admin:true` adds the owner's approved chest/aura. This visual flag grants no permissions. Missing/unknown normal/gear sprite coverage is a visible art limitation, never a different Chao or invented stats. No original PNG/model bytes enter Git/APK.

Open launches this HTTPS origin's `/tiny.html` without any credential. The full app must authenticate the same account separately; secure browser handoff is still an integration dependency. Browser-preview WebSocket credentials/slots are not Android device pairing and are never imported as a fallback.

## Release evidence

The accounts owner fixed the Watch acknowledgement owner-online comparison identified during review; current source compares the same connectivity state before/after offline HTTP care. This Android workstream did not edit the server. Include offline resident cooldown/invalid-care acknowledgement in live integration acceptance; source inspection is not executed Android evidence.

Keep the PR a draft until native APK/lint/JUnit/instrumentation and an actual paired launcher test pass. Validate expired/replayed invitation, lost claim response, revocation, wrong canonical identity, stale revision/Watch, concurrent devices/activity, dropped response after commit, no duplicate care, save failure/restart, resident/nonresident self, asset hash failures, process death/Doze/reboot and full-app canonical login. Rust workstream tests do not substitute for Android HTTP/device integration; authored Android fixtures do not prove Rust authority. No live saves or preview processes are changed here.
