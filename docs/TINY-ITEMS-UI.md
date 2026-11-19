# Tiny Garden item presentation

The client Items screen projects the canonical self profile's `ownedCosmetics`,
`equipped`, and `gear.footwear` / `gear.hands`. It reports omitted or malformed
data as **not reported**. A server-provided empty cosmetic list or explicit null
equipment slot is shown as empty. Disconnect retains the last received profile
and labels it last synced; this does not establish a save acknowledgement.

The published Rust `Player::public` has no consumable inventory. Its shop catalog
describes offers; buying food/drives applies effects immediately and requires
desktop proximity. Tiny Garden does not treat catalog entries as owned stock,
display invented quantities, or call those desktop feed/buy/train actions.
The existing Watch controller provides the supported server-validated pet,
rest, and wake care controls.

The separate authoritative-care branch adds owner-only `snapshot.care` using
`rulesVersion:"adapted-care-v1"`. The Items controller can show exact inventory
quantities and source catalog names from this snapshot. It never treats shop
stock as ownership or applies native raw rows as locally calculated gains.

`createTinyItems({root,send}).update(profile, {connected,care,careMutations,
accountsV2,serverTime})` enables consume/pet only when the caller has explicitly
confirmed the compatible care transport (`careMutations:true`), current
eligibility permits care, the canonical cooldown is ready and account v2 is
absent. Account-v2/care revision integration remains a backend dependency, so
these actions remain disabled there. The current published service advertises
no care snapshot; its Items view stays read-only. The entry point recognizes
the exact owner snapshot's `adapted-care-v1` version as the compatible local
care transport. It never infers support from a catalog name or from inventory
cached on this device. Account-v2 mutations remain disabled until the two
backend contracts are integrated.

The strict care envelope uses the exact published `requestId`,
`expectedRevision`, and `intent:{kind,itemId?}` shape. An unacknowledged online
operation may be retried with its identical payload and identity. Disconnect
does not queue new mutations. `handleEvent(event)` accepts only the matching
durable `care-result`; replayed receipts are shown as already saved, never
reapplied. Quantity/needs/stat changes come only from a refreshed canonical
snapshot, and newer operations wait for the acknowledged revision.
