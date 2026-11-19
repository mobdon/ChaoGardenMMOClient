import test from 'node:test';
import assert from 'node:assert/strict';
import { createTinyItems, itemLabel, matchingCareResult, tinyCareView, tinyInventoryView } from '../src/tiny-items.js';

test('missing inventory is unreported, while an explicit empty owned list is empty', () => {
  const unknown = tinyInventoryView({ id: 'same-chao' }, { connected: true });
  assert.equal(unknown.cosmeticsStatus, 'unreported');
  assert.deepEqual(unknown.cosmetics, []);
  assert.ok(unknown.equipment.every(slot => !slot.reported && slot.value === 'Not reported'));
  const empty = tinyInventoryView({ id: 'same-chao', ownedCosmetics: [], equipped: null, gear: { footwear: null, hands: null } });
  assert.equal(empty.cosmeticsStatus, 'empty');
  assert.ok(empty.equipment.every(slot => slot.reported && slot.value === 'None equipped'));
});

test('only canonical owned cosmetics are listed and equipment grants no invented stock', () => {
  const profile = { id: 'same-chao', ownedCosmetics: ['eggshell', 'eggshell', 'future-native-hat'], equipped: 'eggshell', gear: { footwear: 'shadow-air-shoes', hands: 'knuckles-hammer-gloves' }, inventory: { fruit: 999 }, shop: { catalog: [{ id: 'fruit', quantity: 999 }] } };
  const before = structuredClone(profile);
  const view = tinyInventoryView(profile, { connected: true });
  assert.deepEqual(view.cosmetics, [{ id: 'eggshell', label: 'Eggshell Cap', equipped: true }, { id: 'future-native-hat', label: 'Future Native Hat', equipped: false }]);
  assert.equal(view.equipment[1].value, 'Shadow Air Shoes');
  assert.equal(view.equipment[2].value, 'Knuckles Hammer Gloves');
  assert.deepEqual(profile, before);
  assert.equal(Object.hasOwn(view, 'inventory'), false);
});

test('disconnection keeps actual cached ownership without inventing availability or gains', () => {
  const profile = { id: 'same-chao', ownedCosmetics: ['eggshell'], equipped: null };
  const live = tinyInventoryView(profile, { connected: true });
  const cached = tinyInventoryView(profile, { connected: false });
  assert.equal(cached.connected, false);
  assert.deepEqual(cached.cosmetics, live.cosmetics);
  assert.deepEqual(cached.equipment, live.equipment);
  assert.equal(tinyInventoryView(null, { connected: true }).hasProfile, false);
});

test('malformed item data is unavailable and cannot turn into empty inventory or HTML', () => {
  for (const ownedCosmetics of [null, {}, ['eggshell', null], ['<img src=x>'], ['x'.repeat(129)]]) {
    assert.equal(tinyInventoryView({ id: 'chao', ownedCosmetics }).cosmeticsStatus, 'unreported');
  }
  assert.equal(itemLabel('<script>'), 'Not reported');
  assert.equal(tinyInventoryView({ id: 'chao', equipped: '' }).equipment[0].reported, false);
  assert.equal(tinyInventoryView({ id: 'chao', equipped: '<img src=x>' }).equipment[0].value, 'Not reported');
});

const careFixture = () => ({ rulesVersion: 'adapted-care-v1', revision: 4,
  inventory: { 'garden-fruit': 2, 'sa2:0': 1 },
  catalog: { enabled: true, items: [{ id: 0, name: 'Penguin', kind: 'animal' }] },
  eligibility: { canCare: true, effectReadyAt: null },
});

test('care inventory uses exact server counts and native catalog identities', () => {
  const care = careFixture();
  const view = tinyCareView(care, { connected: true, careMutations: true });
  assert.equal(view.reported, true);
  assert.deepEqual(view.items.map(item => [item.id, item.label, item.quantity]), [['garden-fruit', 'Garden Fruit', 2], ['sa2:0', 'Penguin', 1]]);
  assert.ok(view.items.every(item => item.mayConsume));
  assert.equal(tinyCareView({ ...care, inventory: {} }).reported, true);
  for (const inventory of [null, [], { 'sa2:25': 1 }, { 'sa2:01': 1 }, { 'garden-fruit': -1 }, { 'garden-fruit': 100 }, { 'garden-fruit': '2' }]) {
    assert.equal(tinyCareView({ ...care, inventory }).reported, false);
  }
  assert.equal(tinyCareView({ ...care, rulesVersion: 'unknown' }).reported, false);
});

test('care actions require explicit transport support, reconciliation and server eligibility', () => {
  const care = careFixture();
  for (const options of [{}, { connected: true }, { connected: true, careMutations: true, accountsV2: true }, { connected: false, careMutations: true }]) {
    assert.equal(tinyCareView(care, options).mayCare, false);
  }
  const enabled = { connected: true, careMutations: true };
  assert.equal(tinyCareView({ ...care, eligibility: { canCare: false, effectReadyAt: null } }, enabled).mayCare, false);
  assert.equal(tinyCareView({ ...care, eligibility: { canCare: true, effectReadyAt: 5000 } }, { ...enabled, serverTime: 4999 }).mayCare, false);
  assert.equal(tinyCareView({ ...care, eligibility: { canCare: true, effectReadyAt: 5000 } }, enabled).mayCare, false);
  const disabledNative = tinyCareView({ ...care, catalog: { enabled: false, items: [] } }, enabled);
  assert.equal(disabledNative.items.find(item => item.id === 'sa2:0').mayConsume, false);
  assert.equal(disabledNative.items.find(item => item.id === 'garden-fruit').mayConsume, true);
});

test('care receipts require exact request identity and a durable contract result', () => {
  const request = { requestId: 'operation-id' };
  const result = { type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: request.requestId, accepted: true, replayed: true, revision: 5, code: 'ok' };
  assert.deepEqual(matchingCareResult(request, result), { accepted: true, replayed: true, revision: 5, code: 'ok' });
  for (const mutation of [{ requestId: 'old-id' }, { accepted: undefined }, { revision: -1 }, { kind: 'notice' }, { rulesVersion: 'unknown' }]) {
    assert.equal(matchingCareResult(request, { ...result, ...mutation }), null);
  }
  assert.equal(matchingCareResult(null, result), null);
});

test('item controller retries the original operation and waits for canonical inventory after saved receipts', () => {
  class Element {
    constructor(document) { this.ownerDocument = document; this.children = []; this.listeners = {}; }
    append(...nodes) { this.children.push(...nodes); }
    replaceChildren(...nodes) { this.children = nodes; }
    setAttribute() {}
    addEventListener(name, callback) { this.listeners[name] = callback; }
    click() { if (!this.disabled) this.listeners.click?.(); }
  }
  const document = { createElement: () => new Element(document) };
  const root = new Element(document);
  const find = text => {
    function visit(node) { if (node.textContent === text) return node; for (const child of node.children) { const match = visit(child); if (match) return match; } }
    return visit(root);
  };
  const sent = [];
  const controller = createTinyItems({ root, send: request => { sent.push(request); return true; }, requestId: () => '00000000-0000-4000-8000-000000000001' });
  const player = { id: 'same-chao' }, care = careFixture();
  const connection = { connected: true, careMutations: true, care };
  controller.update(player, connection);
  find('Use one').click();
  assert.deepEqual(sent[0], { type: 'care', requestId: '00000000-0000-4000-8000-000000000001', expectedRevision: 4, intent: { kind: 'consume', itemId: 'garden-fruit' } });
  controller.update(player, { ...connection, connected: false });
  assert.equal(find('Retry same care request').disabled, true);
  controller.update(player, { ...connection, care: { ...care, revision: 5 } });
  find('Retry same care request').click();
  assert.deepEqual(sent[1], sent[0], 'retry preserves original revision and identity');
  const result = { type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: sent[0].requestId, accepted: true, replayed: true, revision: 6, code: 'ok' };
  assert.equal(controller.handleEvent(result), true);
  assert.equal(controller.handleEvent(result), false, 'receipt replay is not applied twice');
  assert.equal(find('Use one').disabled, true, 'new mutations wait for the acknowledged canonical revision');
  assert.ok(find('Garden Fruit ×2'), 'receipt does not decrement local inventory');
  controller.update(player, { ...connection, care: { ...care, revision: 6, inventory: { 'garden-fruit': 1 } } });
  assert.ok(find('Garden Fruit ×1'));
  assert.equal(find('Use one').disabled, false);
});
