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
  shop: [{ itemId: 'garden-fruit', price: 3, enabled: true }],
  market: { rules: 'native-gc-black-market-v2', stock: [{ itemId: 'seed:strong', price: 500, sell: 150 }, { itemId: 'sa2-fruit:5', price: 120, sell: 40 }, { itemId: 'sa2:3', price: 9, sell: 1 }], sellPrices: { 'sa2-fruit:5': 40 } },
  garden: {
    plots: [
      { id: 'garden-1', zone: 'garden', x: 10, z: 2, tree: null },
      { id: 'garden-2', zone: 'garden', x: 13, z: 2, tree: { family: 'hero', stage: 'mature', phase: 'adult', fruitId: 'sa2-fruit:5', fruitReady: 2 } },
      { id: 'garden-3', zone: 'garden', x: 10, z: -2, tree: { family: 'round', stage: 'young', phase: 'young', fruitReady: 0 } },
      { id: 'garden-4', zone: 'garden', x: 10, z: -6, tree: { family: 'round', stage: 'seed', phase: 'sprouting', fruitReady: 0 } },
    ],
    groves: [{ id: 'garden-grove-1', zone: 'garden', x: -7, z: -5, fruitId: 'sa2-fruit:0', tree: { kind: 8, stage: 'mature', phase: 'adult', fruitId: 'sa2-fruit:0', fruitReady: 3 } }],
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
  assert.deepEqual(at(13, 2).places[0].actions.map(a => a.kind), ['harvest', 'water'], 'adults can be shaken and watered');
  assert.deepEqual(at(10, -2).places[0].actions.map(a => a.kind), ['water']);
  assert.deepEqual(at(10, -6).places[0].actions, [], 'a sprouting seed cannot be watered');
  assert.deepEqual(at(-7, -5).places[0].actions.map(a => a.kind), ['harvest', 'water'], 'permanent garden trees');
  assert.equal(at(-7, -5).places[0].actions[0].label, 'Shake for Chao Garden Fruit (3 ripe)');
  assert.equal(at(0, 0).places.length, 0);
  assert.equal(gardenCareView(care(), { zone: 'hero', x: 10, z: 2 }, { connected: true }).places.length, 0);
  assert.deepEqual(gardenCareView(care(), { zone: 'garden', x: 13, z: 2 }, { connected: false }).places[0].actions, []);
});

test('inventory, parts and Black Market offers are validated and seeds are never eaten', () => {
  const view = gardenCareView(care(), { zone: 'hub', x: -4, z: 8, rings: 20 }, { connected: true });
  assert.equal(view.items.find(i => i.id === 'seed:hero').mayUse, false);
  assert.equal(view.items.find(i => i.id === 'sa2-fruit:5').mayUse, true);
  assert.deepEqual(view.parts.map(p => [p.category, p.label]), [['ears', 'Penguin'], ['tail', 'Penguin']]);
  const rich = gardenCareView(care(), { zone: 'hub', x: -4, z: 8, rings: 500 }, { connected: true });
  assert.deepEqual(rich.offers.map(o => [o.itemId, o.price, o.mayBuy]), [['seed:strong', 500, true], ['sa2-fruit:5', 120, true]], 'only native seed and fruit rows');
  assert.deepEqual(view.offers.map(o => o.mayBuy), [false, false], '20 rings buys nothing');
  assert.deepEqual(rich.sales.map(o => [o.itemId, o.price]), [['sa2-fruit:5', 40]], 'only carried items the market prices');
  assert.equal(gardenCareView(care(), { zone: 'hub', x: 6, z: 8, rings: 50 }, { connected: true }).atShop, false);
  const off = { ...care(), catalog: { ...care().catalog, fruitsEnabled: false } };
  assert.equal(gardenCareView(off, { zone: 'garden' }, { connected: true }).items.find(i => i.id === 'sa2-fruit:5').mayUse, false);
  assert.equal(gardenCareView({ ...care(), rulesVersion: 'other' }, null).valid, false);
});

test('intents carry only IDs and results must match the pending request', () => {
  assert.deepEqual(careIntent({ id: 'garden-1' }, { kind: 'plant', itemId: 'seed:hero' }), { kind: 'plant', itemId: 'seed:hero', plotId: 'garden-1' });
  assert.deepEqual(careIntent({ id: 'garden-grove-1' }, { kind: 'harvest' }), { kind: 'harvest', plotId: 'garden-grove-1' });
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
  const pick = root.all().find(n => n.tagName === 'button' && n.textContent.startsWith('Shake'));
  pick.listeners.click();
  assert.deepEqual(sent, [{ type: 'care', requestId: 'req-1', expectedRevision: 7, intent: { kind: 'harvest', plotId: 'garden-2' } }]);
  pick.listeners.click();
  assert.equal(sent.length, 1, 'no second request while one is pending');
  panel.update(care(), player, { connected: false });
  panel.update(care(), player, { connected: true });
  assert.deepEqual(sent[1], sent[0]);
  assert.equal(panel.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'req-1', accepted: false, revision: 7, code: 'no-fruit' }), true);
  assert.ok(root.all().some(n => n.textContent === 'No fruit is ripe yet.'));
});

test('animal parts map to the traced native Chao nodes and exact model names', async () => {
  const { animalPartAssets, CHAO_PART_NODES } = await import('../src/chao-attachments.js');
  assert.deepEqual(animalPartAssets({ ears: 3, tail: 0, antlers: 1, wings: 21 }),
    [{ socket: 'leftEar', node: '024', key: 'animal-part-rabbit-child-leftEar' }, { socket: 'rightEar', node: '026', key: 'animal-part-rabbit-child-rightEar' },
      { socket: 'tail', node: '008', key: 'animal-part-penguin-child-tail' }]);
  assert.equal(CHAO_PART_NODES.leftWing, '037');
  assert.deepEqual(animalPartAssets({ ears: 3 }, 'teen'), []);
});

test('entering the Black Market asks the server to restock once per visit', () => {
  const document = { createElement: tag => new Node(tag, document) };
  const root = new Node('aside', document);
  const sent = [];
  let n = 0;
  const panel = createGardenCare({ root, send: m => { sent.push(m); return true; }, requestId: () => `req-${++n}` });
  panel.update(care(), { zone: 'hub', x: 0, z: 0, rings: 600 }, { connected: true });
  assert.equal(sent.length, 0);
  panel.update(care(), { zone: 'hub', x: -4, z: 8, rings: 600 }, { connected: true });
  assert.deepEqual(sent.map(m => m.intent), [{ kind: 'market' }]);
  panel.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'req-1', accepted: true, revision: 8, effects: { restocked: true, market: {} } });
  panel.update(care(), { zone: 'hub', x: -4, z: 8, rings: 600 }, { connected: true });
  assert.equal(sent.length, 1, 'no second visit while still inside');
  const buy = root.all().find(node => node.tagName === 'button' && node.textContent === 'Buy');
  buy.listeners.click();
  assert.deepEqual(sent[1].intent, { kind: 'purchase', itemId: 'seed:strong' });
});

test('Black Market eggs, hats and themes use native names and the right intents', () => {
  assert.equal(careItemKind('egg:2'), 'egg');
  assert.equal(careItemKind('egg:27'), null);
  assert.equal(careItemLabel('egg:2'), 'White egg');
  assert.equal(careItemLabel('egg:36'), 'Shiny red egg');
  assert.equal(careItemLabel('hat:12'), 'Red wool hat');
  assert.equal(careItemLabel('theme:amy'), 'Amy menu theme');
  const withItems = { ...care(), inventory: { 'egg:2': 1, 'hat:3': 1 }, hat: 9, emblems: { count: 12 },
    market: { stock: [{ itemId: 'theme:omochao', price: 10000, sell: 0 }, { itemId: 'egg:5', price: 600, sell: 0 }], sellPrices: { 'hat:3': 500 } } };
  const garden = gardenCareView(withItems, { zone: 'garden', x: 0, z: 0, rings: 0 }, { connected: true });
  assert.equal(garden.items.find(i => i.id === 'egg:2').mayUse, true, 'eggs go down in a garden');
  assert.equal(gardenCareView(withItems, { zone: 'hub', x: 0, z: 0 }, { connected: true }).items.find(i => i.id === 'egg:2').mayUse, false);
  assert.equal(garden.hat, 'Pan');
  assert.equal(garden.emblems, 12);
  const shop = gardenCareView(withItems, { zone: 'hub', x: -4, z: 8, rings: 20000 }, { connected: true });
  assert.deepEqual(shop.offers.map(o => o.itemId), ['theme:omochao', 'egg:5']);
  assert.deepEqual(shop.sales.map(o => [o.itemId, o.price]), [['hat:3', 500]]);
  const document = { createElement: tag => new Node(tag, document) };
  const root = new Node('aside', document);
  const sent = [];
  const panel = createGardenCare({ root, send: m => { sent.push(m); return true; }, requestId: () => 'req-egg' });
  panel.update(withItems, { zone: 'garden', x: 0, z: 0 }, { connected: true });
  root.all().find(n => n.tagName === 'button' && n.textContent === 'Put down').listeners.click();
  assert.deepEqual(sent[0].intent, { kind: 'place-egg', itemId: 'egg:2' });
});

test('worn hats map to the traced native head node and model keys', async () => {
  const { hatAssets, CHAO_HAT_NODE, NATIVE_HAT_IDS } = await import('../src/chao-attachments.js');
  assert.equal(CHAO_HAT_NODE, '016');
  assert.deepEqual(hatAssets(3), ['hat-3']);
  assert.deepEqual(hatAssets(9), ['hat-9', 'hat-9-eye', 'hat-9-jaw']);
  assert.deepEqual(hatAssets(15), ['hat-15']);
  for (const hat of [null, undefined, 0, 1, 2, 16, 40, '3', 3.5]) assert.deepEqual(hatAssets(hat), [], String(hat));
  assert.deepEqual(NATIVE_HAT_IDS, [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
});
