import test from 'node:test';
import assert from 'node:assert/strict';
import { createTinyResident } from '../src/tiny-resident.js';
const profile = { id: 'same-chao' };
const state = { connected: true, capabilities: { autoResident: true }, care: { rulesVersion: 'adapted-care-v1', revision: 2, tinyResident: { enabled: false } } };
test('Tiny opt-in requires advertised authority and submits no client watch or reward state', () => {
  const sent = []; const adapter = createTinyResident({ send: request => sent.push(request), requestId: () => 'one-request' });
  for (const patch of [{ connected: false }, { accountsV2: true }, { capabilities: {} }, { care: null }]) adapter.update(profile, { ...state, ...patch });
  assert.equal(sent.length, 0); adapter.update(profile, state); adapter.update(profile, state);
  assert.deepEqual(sent, [{ type: 'care', requestId: 'one-request', expectedRevision: 2, intent: { kind: 'tiny_enter' } }]);
});
test('acknowledgment cannot invent enabled residency and rejected requests are not retried repeatedly', () => {
  const sent = []; const adapter = createTinyResident({ send: request => sent.push(request), requestId: () => 'one-request' });
  adapter.update(profile, state);
  assert.equal(adapter.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'other', accepted: true }), false);
  adapter.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'one-request', accepted: true });
  adapter.update(profile, state); assert.equal(sent.length, 1);
  adapter.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'one-request', accepted: false });
  adapter.update(profile, state); assert.equal(sent.length, 1);
  adapter.update({ id: 'another-chao' }, state); assert.equal(sent.length, 2);
});
