import { readWidgetSession, requestWidgetJson } from './widget-state.js';

// Observation only, using the published Rust ACCOUNTS-V2.md contract. This
// module never acquires/renews a lease or submits a gameplay/save intent.
const id = value => typeof value === 'string' && value.length > 0 && value.length <= 128;
const integer = value => Number.isSafeInteger(value) && value >= 0;
const activities = new Set(['direct', 'tiny', 'watch', 'training', 'competition']);
function invalidAccount() {
  const error = new Error('The server could not confirm the same canonical Chao. Reconnect with an authorized save.');
  error.retry = false; return error;
}
function unsupportedProtocol() {
  const error = new Error('The server account protocol could not be confirmed. The preview is paused.');
  error.retry = false; return error;
}
export function widgetProtocolVersion(status, knownV2 = false) {
  if (status?.ok !== true) throw unsupportedProtocol();
  if (Object.hasOwn(status, 'accounts')) {
    const value = status.accounts;
    if (value?.version !== 2 || !Array.isArray(value.authentication)
      || !value.authentication.includes('bearer-header') || !value.authentication.includes('websocket-first-frame')) throw unsupportedProtocol();
    return 2;
  }
  if (knownV2) throw unsupportedProtocol();
  return 1;
}
export function normalizeWidgetIdentity(value) {
  if (!value || value.schemaVersion !== 2 || !id(value.accountId) || !id(value.chaoId)
    || !id(value.deviceId) || !integer(value.revision) || !Object.hasOwn(value, 'lease')) throw invalidAccount();
  if (!Array.isArray(value.devices) || value.devices.some(device => !device || !id(device.id)
    || !['desktop', 'android'].includes(device.kind) || typeof device.revoked !== 'boolean')) throw invalidAccount();
  let lease = null;
  if (value.lease !== null) {
    const raw = value.lease;
    if (!raw || !id(raw.id) || !id(raw.deviceId) || !activities.has(raw.activity)
      || !integer(raw.expiresAt) || !integer(raw.lastSequence)) throw invalidAccount();
    lease = { id: raw.id, deviceId: raw.deviceId, activity: raw.activity, expiresAt: raw.expiresAt, lastSequence: raw.lastSequence };
  }
  return { schemaVersion: 2, accountId: value.accountId, chaoId: value.chaoId, deviceId: value.deviceId,
    revision: value.revision, lease, devices: value.devices.map(device => ({ id: device.id, kind: device.kind, revoked: device.revoked })) };
}
export function acceptWidgetIdentity(value, previous) {
  const next = normalizeWidgetIdentity(value);
  if (previous && (next.accountId !== previous.accountId || next.chaoId !== previous.chaoId || next.deviceId !== previous.deviceId)) throw invalidAccount();
  return previous && next.revision < previous.revision ? previous : next;
}
export function acceptWidgetAccountSnapshot(snapshot, previous) {
  const next = acceptWidgetIdentity(snapshot?.identity, previous);
  if (snapshot?.type !== 'snapshot' || snapshot.protocolVersion !== 2 || snapshot.self?.id !== next.chaoId
    || snapshot.selfId !== next.chaoId || !Array.isArray(snapshot.players)
    || snapshot.players.filter(player => player?.id === next.chaoId).length !== 1) throw invalidAccount();
  // A matching public-list row never replaces the private canonical `self`.
  // Another device's newer identity/receipt also fences older observations.
  if (previous && snapshot.identity.revision < previous.revision) return null;
  return { identity: next, player: snapshot.self };
}

export async function readWidgetObservation(request, token, { knownV2 = false, identity = null,
  onProtocol = () => {}, ...options } = {}) {
  const status = await requestWidgetJson(request, '/api/status', { method: 'GET' }, options);
  // Discovery failure never falls back to legacy credential URLs.
  if (!status.ok) throw new Error('The garden capabilities could not be read. Retry to reconnect.');
  const version = widgetProtocolVersion(status.data, knownV2);
  onProtocol(version);
  const session = await readWidgetSession(request, token, options);
  if (version === 1) return { version, token: session.token, player: session.player, identity: null };
  const resumedIdentity = acceptWidgetIdentity(session.identity, identity);
  if (session.player.id !== resumedIdentity.chaoId) throw invalidAccount();
  const result = await requestWidgetJson(request, '/api/account', { method: 'GET',
    headers: { Authorization: `Bearer ${session.token}` } }, options);
  if (!result.ok) {
    const error = new Error(result.status === 401 ? 'This device is no longer authorized. Choose an authorized save.'
      : 'The current account state could not be read. Retry to reconnect.');
    error.retry = result.status >= 500; throw error;
  }
  if (result.data?.capabilities?.version !== 2) throw unsupportedProtocol();
  const accountIdentity = acceptWidgetIdentity(result.data.identity, resumedIdentity);
  const observation = acceptWidgetAccountSnapshot(result.data.snapshot, accountIdentity);
  if (!observation || observation.identity.revision !== result.data.identity.revision) throw invalidAccount();
  return { version, token: session.token, ...observation };
}
export function widgetSocketUrl(location, version, token) {
  if (version !== 1 && version !== 2) throw unsupportedProtocol();
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${protocol}//${location.host}/ws${version === 2 ? '' : `?token=${encodeURIComponent(token)}`}`;
}
export function authenticateWidgetSocket(socket, version, token) {
  if (version !== 1 && version !== 2) throw unsupportedProtocol();
  if (version === 2) socket.send(JSON.stringify({ type: 'authenticate', token }));
}
