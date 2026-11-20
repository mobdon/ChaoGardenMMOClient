import test from 'node:test';
import assert from 'node:assert/strict';
import { careIntent, careItemKind, careItemLabel, createGardenCare, gardenCareView, matchingResult } from '../src/garden-care.js';
import { tinyCareView } from '../src/tiny-items.js';

const care = () => ({
  rulesVersion: 'adapted-care-v1', revision: 7,
  inventory: { 'sa2-fruit:5': 2, 'seed:hero': 1, 'sa2:0': 1, 'sa2:22': 1, 'garden-fruit': 1 },
  catalog: { enabled: true, fruitsEnabled: true, items: [{ id: 0, name: 'Penguin', kind: 'animal' }], fruits: [{ id: 5, name: 'Hero Fruit' }, { id: 0, name: 'Chao Garden Fruit' }] },
  eligibility: { canCare: true, effectReadyAt: null },
  animalParts: { shown: { ears: 0, tail: 0, antlers: 3, wings: 99 } },
  shop: [{ itemId: 'seed:hero', price: 15, enabled: true }, { itemId: 'garden-fruit', price: 3, enabled: true }],
  garden: {
    plots: [
      { id: 'garden-1', zone: 'garden', x: 10, z: 2, tree: null },
      { id: 'garden-2', zone: 'garden', x: 13, z: 2, tree: { family: 'hero', stage: 'mature', fruitId: 'sa2-fruit:5', fruitReady: 2 } },
      { id: 'garden-3', zone: 'garden', x: 10, z: -2, tree: { family: 'round', stage: 'young', fruitReady: 0, waterReadyAt: 5000 } },
    ],
    groves: [{ id: 'garden-grove', zone: 'garden', x: -7, z: -5, fruitId: 'sa2-fruit:0', fruitReady: 3 }],
  },
});

test('item kinds and labels come from exact IDs and the server catalog', () => {
  assert.equal(careItemKind('sa2-fruit:23'), 'fruit');
  assert.equal(careItemKind('sa2-fruit:24'), null);
  assert.equal(careItemKind('seed:square'), 'seed');
  assert.equal(careItemKind('seed:banana'), null);
  assert.equal(careItemKind('sa2:20'), 'animal');
  assert.equal(careItemKind('sa2:21'), 'drive');
  assert.equal(careItemLabel('sa2-fruit:5', care().catalog), 'Hero Fruit');
  assert.equal(careItemLabel('sa2-fruit:9', care().catalog), 'Native fruit 9');
  assert.equal(careItemLabel('seed:triangle'), 'Triangle Seed');
});

test('nearby places offer only the actions the server could accept', () => {
  const at = (x, z) => gardenCareView(care(), { zone: 'garden', x, z, rings: 20 }, { connected: true, serverTime: 1000 });
  assert.deepEqual(at(10, 2).places[0].actions, [{ kind: 'plant', itemId: 'seed:hero', label: 'Plant Hero Seed' }]);
  assert.deepEqual(at(13, 2).places[0].actions.map(a => a.kind), ['harvest']);
  assert.deepEqual(at(10, -2).places[0].actions, [], 'water is on cooldown until 5000');
  assert.deepEqual(gardenCareView(care(), { zone: 'garden', x: 10, z: -2 }, { connected: true, serverTime: 6000 }).places[0].actions.map(a => a.kind), ['water']);
  assert.equal(at(-7, -5).places[0].actions[0].label, 'Pick Chao Garden Fruit (3)');
  assert.equal(at(0, 0).places.length, 0);
  assert.equal(gardenCareView(care(), { zone: 'hero', x: 10, z: 2 }, { connected: true }).places.length, 0);
  assert.deepEqual(gardenCareView(care(), { zone: 'garden', x: 13, z: 2 }, { connected: false }).places[0].actions, []);
});

test('inventory, parts and seed offers are validated and seeds are never eaten', () => {
  const view = gardenCareView(care(), { zone: 'hub', x: -4, z: 8, rings: 20 }, { connected: true });
  assert.equal(view.items.find(i => i.id === 'seed:hero').mayUse, false);
  assert.equal(view.items.find(i => i.id === 'sa2-fruit:5').mayUse, true);
  assert.deepEqual(view.parts.map(p => [p.category, p.label]), [['ears', 'Penguin'], ['tail', 'Penguin']]);
  assert.deepEqual(view.offers.map(o => [o.itemId, o.mayBuy]), [['seed:hero', true]]);
  assert.equal(gardenCareView(care(), { zone: 'hub', x: -4, z: 8, rings: 5 }, { connected: true }).offers[0].mayBuy, false);
  assert.equal(gardenCareView(care(), { zone: 'hub', x: 6, z: 8, rings: 50 }, { connected: true }).atShop, false);
  const off = { ...care(), catalog: { ...care().catalog, fruitsEnabled: false } };
  assert.equal(gardenCareView(off, { zone: 'garden' }, { connected: true }).items.find(i => i.id === 'sa2-fruit:5').mayUse, false);
  assert.equal(gardenCareView({ ...care(), rulesVersion: 'other' }, null).valid, false);
});

test('intents carry only IDs and results must match the pending request', () => {
  assert.deepEqual(careIntent({ id: 'garden-1' }, { kind: 'plant', itemId: 'seed:hero' }), { kind: 'plant', itemId: 'seed:hero', plotId: 'garden-1' });
  assert.deepEqual(careIntent({ id: 'garden-grove' }, { kind: 'harvest' }), { kind: 'harvest', plotId: 'garden-grove' });
  const request = { requestId: 'r1' };
  assert.equal(matchingResult(request, { type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'r2', accepted: true, revision: 1 }), null);
  assert.deepEqual(matchingResult(request, { type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'r1', accepted: false, revision: 1, code: 'no-fruit' }),
    { accepted: false, replayed: false, revision: 1, code: 'no-fruit' });
});

test('Tiny Garden items list fruit and seeds without offering to eat seeds', () => {
  const view = tinyCareView(care(), { connected: true, careMutations: true });
  assert.equal(view.reported, true);
  assert.equal(view.items.find(i => i.id === 'sa2-fruit:5').label, 'Hero Fruit');
  assert.equal(view.items.find(i => i.id === 'sa2-fruit:5').mayConsume, true);
  assert.equal(view.items.find(i => i.id === 'seed:hero').mayConsume, false);
});

class Node {
  constructor(tag, doc) { this.tagName = tag; this.ownerDocument = doc; this.children = []; this.listeners = {}; this.attributes = {}; this.textContent = ''; this.disabled = false; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(k, v) { this.attributes[k] = v; }
  addEventListener(type, fn) { this.listeners[type] = fn; }
  all() { return this.children.flatMap(child => [child, ...child.all()]); }
}

test('panel submits one request, retries it unchanged after reconnect and reports the reason', () => {
  const document = { createElement: tag => new Node(tag, document) };
  const root = new Node('aside', document);
  const sent = [];
  const panel = createGardenCare({ root, send: m => { sent.push(m); return true; }, requestId: () => 'req-1' });
  const player = { zone: 'garden', x: 13, z: 2 };
  panel.update(care(), player, { connected: true, serverTime: 1 });
  const pick = root.all().find(n => n.tagName === 'button' && n.textContent.startsWith('Pick'));
  pick.listeners.click();
  assert.deepEqual(sent, [{ type: 'care', requestId: 'req-1', expectedRevision: 7, intent: { kind: 'harvest', plotId: 'garden-2' } }]);
  pick.listeners.click();
  assert.equal(sent.length, 1, 'no second request while one is pending');
  panel.update(care(), player, { connected: false });
  panel.update(care(), player, { connected: true });
  assert.deepEqual(sent[1], sent[0]);
  assert.equal(panel.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'req-1', accepted: false, revision: 7, code: 'no-fruit' }), true);
  assert.ok(root.all().some(n => n.textContent === 'No fruit is ready yet.'));
});
