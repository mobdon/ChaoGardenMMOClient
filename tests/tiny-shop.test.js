import test from 'node:test';
import assert from 'node:assert/strict';
import { createTinyShop, tinyShopView } from '../src/tiny-shop.js';

function state() { return { connected: true, serverTime: 5000, care: { rulesVersion: 'adapted-care-v1', revision: 4,
  inventory: { 'garden-fruit': 2 }, eligibility: { canCare: true, effectReadyAt: 4500 },
  tinyShop: { rulesVersion: 'adapted-tiny-shop-v1', enabled: true, eligibility: { canPurchase: true, reason: null },
    items: [{ itemId: 'garden-fruit', name: 'Garden Fruit', price: 3, enabled: true, maxQuantity: 99 }] } } }; }
test('shop shows only reported catalog and disables stale, unsupported, unaffordable and full inventory actions', () => {
  const profile = { id: 'same', rings: 148 };
  assert.equal(tinyShopView(profile, {}).supported, false);
  const current = state(); const view = tinyShopView(profile, current);
  assert.deepEqual([view.rings, view.items[0].quantity, view.items[0].canBuy, view.items[0].canFeed], [148, 2, true, true]);
  assert.equal(tinyShopView(profile, { ...current, connected: false }).items[0].canBuy, false);
  assert.equal(tinyShopView(profile, { ...current, accountsV2: true }).items[0].canFeed, false);
  assert.equal(tinyShopView({ ...profile, rings: 2 }, current).items[0].canBuy, false);
  current.care.inventory['garden-fruit'] = 99;
  assert.equal(tinyShopView(profile, current).items[0].canBuy, false);
  current.care.eligibility.effectReadyAt = 6000;
  assert.equal(tinyShopView(profile, current).items[0].canFeed, false);
  current.care.tinyShop.items[0].price = -10;
  assert.equal(tinyShopView(profile, current).items.length, 0);
});

class Node {
  constructor(tag, document) { this.tagName = tag; this.ownerDocument = document; this.children = []; this.handlers = {}; this.textContent = ''; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(key, value) { this[key] = value; }
  addEventListener(key, action) { this.handlers[key] = action; }
  click() { if (!this.disabled) this.handlers.click?.(); }
  find(text) { return this.children.flatMap(child => [child, ...child.descendants()]).find(child => child.textContent === text); }
  descendants() { return this.children.flatMap(child => [child, ...child.descendants()]); }
}
test('pending shop requests never edit wallet/inventory and uncertain retry uses the same durable UUID', () => {
  const document = { createElement: tag => new Node(tag, document) }, root = new Node('section', document);
  const sent = [], profile = { id: 'same', rings: 148 }, current = state();
  const shop = createTinyShop({ root, send: request => { sent.push(request); return true; }, requestId: () => 'durable-one' });
  shop.update(profile, current); root.find('Buy one · 3 rings').click();
  assert.deepEqual(sent[0], { type: 'care', requestId: 'durable-one', expectedRevision: 4, intent: { kind: 'tiny_purchase', itemId: 'garden-fruit' } });
  assert.equal(profile.rings, 148); assert.equal(current.care.inventory['garden-fruit'], 2);
  root.find('Check the same request again').click(); assert.equal(sent[1], sent[0]);
  assert.equal(shop.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'unrelated', accepted: true, revision: 5 }), false);
  assert.equal(shop.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'durable-one', accepted: true, revision: 5 }), true);
  assert.equal(root.find('Buy one · 3 rings').disabled, true, 'receipt waits for reconciled inventory snapshot');
  shop.update({ ...profile, rings: 145 }, { ...current, care: { ...current.care, revision: 5, inventory: { 'garden-fruit': 3 } } });
  assert.equal(root.find('Buy one · 3 rings').disabled, false);
  assert.ok(root.descendants().some(node => node.textContent === '◎ 145'));
});
test('changing canonical Chao discards another profile’s pending request without applying its receipt', () => {
  const document = { createElement: tag => new Node(tag, document) }, root = new Node('section', document);
  const shop = createTinyShop({ root, send: () => true, requestId: () => 'old-chao-request' });
  shop.update({ id: 'first', rings: 148 }, state()); root.find('Feed one').click();
  shop.update({ id: 'second', rings: 20 }, state());
  assert.equal(shop.handleEvent({ type: 'event', kind: 'care-result', rulesVersion: 'adapted-care-v1', requestId: 'old-chao-request', accepted: true, revision: 5 }), false);
});

import { shelfStock } from '../src/tiny-stock-shelf.js';
test('native slots contain server offers only, including the free normal egg, without moving the toy when the egg is held', () => {
  const care=state().care;
  care.tinyShop.items.push({itemId:'tiny-fruit:6',name:'Red Fruit',price:70});
  care.tinyEggShop={enabled:true,offers:[{eggId:0,price:0}]};
  care.tinyToys={items:[{id:0,name:'Trumpet',offered:true,owned:false,price:1000}]};
  assert.deepEqual(shelfStock(care).map(r=>[r.slot,r.art,r.price]),[[6,'fruit-6',70],[7,'egg-0',0],[8,'toy-0',1000]]);
  care.tinyCollection={pendingEgg:{nativeEggId:0}};
  assert.deepEqual(shelfStock(care).map(r=>r.slot),[6,8]);
  care.tinyToys.items[0].offered=false;
  assert.deepEqual(shelfStock(care).map(r=>r.slot),[6]);
});
