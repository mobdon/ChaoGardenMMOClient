// The Rust service persists residency and owns all mode transitions. This
// adapter only opts the selected Chao into the advertised Tiny Garden mode.
export function createTinyResident({ send, requestId = () => crypto.randomUUID() }) {
  let profileId = null, pending = null, rejected = false;
  return {
    update(profile, { connected, care, capabilities, accountsV2 = false } = {}) {
      if (profile?.id !== profileId) { profileId = profile?.id; pending = null; rejected = false; }
      if (care?.tinyResident?.enabled === true) { pending = null; rejected = false; return; }
      if (!profileId || !connected || accountsV2 || pending || rejected || capabilities?.autoResident !== true
        || care?.rulesVersion !== 'adapted-care-v1' || !Number.isSafeInteger(care.revision) || care.revision < 0) return;
      const request = { type: 'care', requestId: requestId(), expectedRevision: care.revision, intent: { kind: 'tiny_enter' } };
      // Mark before sending: a synchronous snapshot cannot enqueue twice.
      pending = request;
      if (send(request) === false) pending = null;
    },
    handleEvent(event) {
      if (!pending || event?.type !== 'event' || event.kind !== 'care-result' || event.rulesVersion !== 'adapted-care-v1'
        || event.requestId !== pending.requestId || typeof event.accepted !== 'boolean') return false;
      if (event.accepted !== true) { rejected = true; pending = null; }
      // Accepted waits for the authoritative enabled snapshot; no local opt-in.
      return true;
    },
  };
}
