// Account v2 transport adapter. Credentials stay in memory and request headers;
// gameplay state, lease ownership, revisions and counters remain server-owned.
const activities = new Set(['direct', 'tiny', 'watch', 'training']);
const integer = value => Number.isSafeInteger(value) && value >= 0;
const id = value => typeof value === 'string' && value.length > 0 && value.length <= 128;
const errors = {
  unauthorized: 'This saved Chao could not be authenticated. Choose an authorized save to reconnect.',
  device_revoked: 'This device has been revoked. Pair it again from an authorized desktop.',
  stale_revision: 'Your Chao changed on the server. The latest state is being synced; repeat the action only after checking it.',
  stale_lease: 'Control changed on the server. Syncing the current device and activity.',
  lease_expired: 'The control lease expired. Syncing before another action.',
  replayed_intent: 'The server rejected an old input. Syncing the current input sequence.',
  activity_conflict: 'Another device or activity currently controls this Chao. Finish that activity before taking control here.',
  lease_required: 'Syncing your Chao before requesting control.',
  persistence_failed: 'The server could not save this action. Check the synced state before trying again.',
  invalid_protocol: 'This server does not provide the supported account and activity contract.',
  invalid_identity: 'The server returned incomplete account state. Controls remain paused.',
  disconnected: 'Connection interrupted. Your Chao must sync before another action.',
  confirmation_timeout: 'The server has not confirmed this input. Syncing without replaying it.',
  request_timeout: 'The account server did not respond in time. Controls remain paused until your Chao syncs.',
};

export function supportsTinyAccounts(status) {
  const value = status?.accounts;
  return value?.version === 2 && value.activityLeases === true && value.intentSequence === true
    && Array.isArray(value.authentication) && value.authentication.includes('websocket-first-frame');
}

export function normalizeAccountIdentity(value) {
  if (!value || value.schemaVersion !== 2 || !id(value.accountId) || !id(value.chaoId)
    || !id(value.deviceId) || !integer(value.revision) || !Object.hasOwn(value, 'lease')) return null;
  if (!Array.isArray(value.devices) || value.devices.some(device => !device || !id(device.id)
    || !['desktop', 'android'].includes(device.kind) || typeof device.revoked !== 'boolean')) return null;
  let lease = null;
  if (value.lease !== null) {
    const raw = value.lease;
    if (!raw || !id(raw.id) || !id(raw.deviceId) || !activities.has(raw.activity)
      || !integer(raw.expiresAt) || !integer(raw.lastSequence)) return null;
    lease = { id: raw.id, deviceId: raw.deviceId, activity: raw.activity,
      expiresAt: raw.expiresAt, lastSequence: raw.lastSequence };
  }
  return { schemaVersion: 2, accountId: value.accountId, chaoId: value.chaoId,
    deviceId: value.deviceId, revision: value.revision, lease,
    devices: Array.isArray(value.devices) ? value.devices.map(device => ({
      id: device.id, kind: device.kind, revoked: device.revoked === true,
    })) : [] };
}

export function tinyIntentActivity(message) {
  const action = message?.action || '';
  if (message?.type === 'tiny-input' || action.startsWith('tiny-')) return 'tiny';
  if (action.startsWith('watch-')) return 'watch';
  if (action.startsWith('training-')) return 'training';
  return 'direct';
}

class AccountError extends Error {
  constructor(code) { super(errors[code] || 'The server could not confirm this request. Sync your Chao before trying again.'); this.code = code; }
}

export function createTinyAccount({ fetch: requestFetch = globalThis.fetch, transmit, onState = () => {},
  onSnapshot = () => {}, onError = () => {}, now = () => globalThis.performance.now(), acknowledgementMs = 6000,
  requestTimeoutMs = 10000 } = {}) {
  let token = null, identity = null, capabilities = null, snapshot = null;
  let ready = false, preparing = false, pending = null, generation = 0, lastRenewedAt = -Infinity;
  let reconciling = null;
  const idleWaiters = [];
  const notify = () => {
    onState({ ready, pending: preparing || !!pending, identity, capabilities,
      controlAvailable: ready && (!identity?.lease || identity.lease.deviceId === identity.deviceId) });
    if (!ready || (!preparing && !pending)) idleWaiters.splice(0).forEach(resolve => resolve(ready));
  };
  const report = error => onError(error.message, error.code);
  function acceptIdentity(value) {
    const next = normalizeAccountIdentity(value);
    if (!next || (identity && (next.accountId !== identity.accountId || next.chaoId !== identity.chaoId || next.deviceId !== identity.deviceId))) {
      throw new AccountError('invalid_identity');
    }
    if (!identity || next.revision >= identity.revision) identity = next;
    return identity;
  }
  async function request(path, method = 'GET', body) {
    const controller = new AbortController();
    let timeout;
    try {
      return await Promise.race([(async () => {
        const response = await requestFetch(path, { method, cache: 'no-store', credentials: 'omit', signal: controller.signal,
          headers: { Authorization: `Bearer ${token}`, ...(body ? { 'Content-Type': 'application/json' } : {}) },
          ...(body ? { body: JSON.stringify(body) } : {}) });
        let data;
        try { data = await response.json(); } catch { throw new AccountError('invalid_identity'); }
        if (!response.ok) throw new AccountError(typeof data.error === 'string' ? data.error : 'request_failed');
        return data;
      })(), new Promise((_, reject) => {
        timeout = setTimeout(() => { controller.abort(); reject(new AccountError('request_timeout')); }, requestTimeoutMs);
      })]);
    } finally { clearTimeout(timeout); }
  }
  function finishPending(accepted) {
    if (!pending) return;
    const operation = pending; pending = null; clearTimeout(operation.timer); operation.resolve(accepted); notify();
  }
  async function readAccount() {
    const currentGeneration = generation;
    const result = await request('/api/account');
    if (currentGeneration !== generation) throw new AccountError('disconnected');
    const stale = identity && result.identity?.revision < identity.revision;
    acceptIdentity(result.identity);
    if (result.capabilities?.version !== 2 || result.snapshot?.protocolVersion !== 2
      || result.snapshot?.self?.id !== identity.chaoId) throw new AccountError('invalid_identity');
    capabilities = result.capabilities;
    // A concurrent socket acknowledgement may already have a newer durable
    // revision than this HTTP response. Keep its matching gameplay snapshot.
    if (!stale) { snapshot = result.snapshot; onSnapshot(snapshot); }
    return { identity, snapshot, capabilities };
  }
  async function leaseOperation(operation, activity) {
    const currentGeneration = generation;
    const body = { operation, expectedRevision: identity.revision };
    if (operation === 'acquire') body.activity = activity;
    else body.leaseId = identity.lease.id;
    const result = await request('/api/lease', 'POST', body);
    if (currentGeneration !== generation) throw new AccountError('disconnected');
    acceptIdentity(result.identity);
    if (operation !== 'release') lastRenewedAt = now();
    notify(); return identity;
  }
  async function renewOrAcquire(activity) {
    try { await leaseOperation('renew'); }
    catch (error) {
      if (error.code !== 'lease_expired') throw error;
      // No gameplay request was sent. Re-read ownership before requesting a
      // fresh lease generation; never retry the original gameplay mutation.
      await readAccount();
      if (identity.lease && identity.lease.deviceId !== identity.deviceId) throw new AccountError('activity_conflict');
      await leaseOperation('acquire', activity);
    }
  }
  async function ensureActivity(activity) {
    const lease = identity.lease;
    if (lease?.deviceId !== undefined && lease.deviceId !== identity.deviceId) throw new AccountError('activity_conflict');
    if (lease && lease.activity !== activity) await leaseOperation('release');
    if (!identity.lease) await leaseOperation('acquire', activity);
    else if (now() - lastRenewedAt >= 10000) await renewOrAcquire(activity);
  }
  async function reconcile(error) {
    if (reconciling) return reconciling;
    ready = false; finishPending(false); notify();
    if (error) report(error);
    const expectedGeneration = generation;
    reconciling = (async () => {
      try {
        const result = await readAccount();
        if (expectedGeneration !== generation) return null;
        ready = true; notify(); return result;
      } catch (failure) {
        if (expectedGeneration === generation) { ready = false; report(failure instanceof AccountError ? failure : new AccountError('disconnected')); notify(); }
        return null;
      } finally { if (expectedGeneration === generation) reconciling = null; }
    })();
    return reconciling;
  }
  return {
    async connect({ token: nextToken, status }) {
      generation++; finishPending(false); ready = false; preparing = false; identity = null; snapshot = null; reconciling = null; lastRenewedAt = -Infinity;
      if (!supportsTinyAccounts(status)) throw new AccountError('invalid_protocol');
      if (typeof nextToken !== 'string' || !nextToken || nextToken.length > 256 || /\s/.test(nextToken)) throw new AccountError('unauthorized');
      token = nextToken; notify();
      const currentGeneration = generation;
      try {
        await readAccount();
        const lease = identity.lease;
        if (lease?.deviceId === identity.deviceId) await renewOrAcquire(lease.activity);
        if (currentGeneration !== generation) return null;
        ready = true; notify(); return { identity, snapshot, capabilities };
      } catch (error) {
        if (currentGeneration === generation) { ready = false; report(error instanceof AccountError ? error : new AccountError('disconnected')); notify(); }
        throw error instanceof AccountError ? error : new AccountError('disconnected');
      }
    },
    async send(message) {
      if (!ready || !message || typeof transmit !== 'function') return false;
      if (message.type === 'action' && message.action === 'save') return transmit({ type: 'action', action: 'save' }) !== false;
      // Transient movement may be dropped while a durable intent is in flight.
      // No care, purchase, game or movement request is queued for later replay.
      if (preparing || pending) return false;
      preparing = true; notify();
      const currentGeneration = generation;
      try {
        await ensureActivity(tinyIntentActivity(message));
        if (currentGeneration !== generation || !ready) return false;
        const lease = identity.lease;
        if (!lease || lease.deviceId !== identity.deviceId || lease.lastSequence >= Number.MAX_SAFE_INTEGER) throw new AccountError('invalid_identity');
        const sequence = lease.lastSequence + 1;
        const envelope = { ...message, leaseId: lease.id, expectedRevision: identity.revision, sequence };
        return await new Promise(resolve => {
          pending = { sequence, leaseId: lease.id, resolve,
            timer: setTimeout(() => { void reconcile(new AccountError('confirmation_timeout')); }, acknowledgementMs) };
          if (transmit(envelope) === false) { finishPending(false); ready = false; notify(); }
        });
      } catch (error) {
        if (currentGeneration === generation) await reconcile(error instanceof AccountError ? error : new AccountError('disconnected'));
        return false;
      } finally {
        if (currentGeneration === generation) { preparing = false; notify(); }
      }
    },
    async takeControl(activity) {
      if (!ready || preparing || pending || !activities.has(activity)) return false;
      const currentGeneration = generation;
      preparing = true; notify();
      try {
        if (identity.lease?.deviceId !== undefined && identity.lease.deviceId !== identity.deviceId) {
          // This explicit user request asks the authoritative server whether a
          // retained foreign lease has expired. Never release another device's
          // lease or infer ownership from the browser's wall clock.
          await leaseOperation('acquire', activity);
        } else await ensureActivity(activity);
        return currentGeneration === generation && ready;
      } catch (error) {
        if (currentGeneration === generation) await reconcile(error instanceof AccountError ? error : new AccountError('disconnected'));
        return false;
      } finally { if (currentGeneration === generation) { preparing = false; notify(); } }
    },
    consume(message) {
      if (!token) return false;
      if (message?.type === 'error' && message.protocolVersion === 2) {
        void reconcile(new AccountError(message.error || 'request_failed')); return true;
      }
      if (!message?.identity) return false;
      const staleSnapshot = message.type === 'snapshot' && identity && message.identity.revision < identity.revision;
      try { acceptIdentity(message.identity); }
      catch (error) { ready = false; finishPending(false); report(error); notify(); return true; }
      if (staleSnapshot) return false;
      if (message.type === 'snapshot' && message.protocolVersion === 2) snapshot = message;
      if (message.type === 'event' && ['intent-accepted', 'watch-saved'].includes(message.kind)
        && pending && message.sequence === pending.sequence && identity.lease?.id === pending.leaseId
        && identity.lease.lastSequence >= pending.sequence) finishPending(true);
      notify(); return true;
    },
    disconnect() { generation++; ready = false; preparing = false; finishPending(false); notify(); },
    whenIdle() { return !ready || (!preparing && !pending) ? Promise.resolve(ready) : new Promise(resolve => idleWaiters.push(resolve)); },
    get ready() { return ready; },
    get identity() { return identity; },
    get snapshot() { return snapshot; },
    get pending() { return preparing || !!pending; },
    get version() { return 2; },
    reconcile() { return reconcile(); },
  };
}
