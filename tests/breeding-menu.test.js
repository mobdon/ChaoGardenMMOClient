import test from 'node:test';
import assert from 'node:assert/strict';
import { breedingView, createBreedingMenu } from '../src/breeding-menu.js';

class Node {
  constructor(tag, doc) { this.tagName = tag; this.ownerDocument = doc; this.children = []; this.listeners = {}; this.attributes = {}; this.textContent = ''; this.style = {}; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(k, v) { this.attributes[k] = v; }
  addEventListener(type, fn) { this.listeners[type] = fn; }
  all() { return this.children.flatMap(child => [child, ...child.all()]); }
}
const snapshot = () => ({
  selfId: 'me', serverTime: 1000,
  players: [
    { id: 'me', name: 'Mine', zone: 'garden', life: { stage: 'adult', ill: false, inTheMood: true, busy: false } },
    { id: 'b', name: 'Bubbles', zone: 'garden', life: { stage: 'adult', ill: true, inTheMood: true, busy: false }, stats: { swim: { level: 3, grade: 2 } } },
    { id: 'c', name: 'Cheese', zone: 'garden', life: { stage: 'adult' } },
    { id: 'kid', name: 'Kid', zone: 'garden', life: { stage: 'child' } },
    { id: 'far', name: 'Far', zone: 'hero', life: { stage: 'adult' } },
  ],
  life: { revision: 4, eligibility: { canBreed: true }, nest: [{ id: 'egg-1', motherName: 'Mine', fatherName: 'Bubbles' }] },
});

test('only grown-up Chao in this garden can be picked, callers first, then nest eggs', () => {
  const view = breedingView(snapshot(), new Map([['c', { expiresAt: 2000 }]]));
  assert.deepEqual(view.rows.map(r => r.id), ['c', 'b', 'egg-1']);
  assert.equal(view.rows[0].note, 'Cheese is calling you');
});

test('pick once to see the Chao, again to invite it; a nest egg is named and hatched', async () => {
  const document = { createElement: tag => new Node(tag, document) };
  const root = new Node('aside', document);
  const sent = [], hatched = [], shown = [];
  const menu = createBreedingMenu({ root, send: m => { sent.push(m); return true; }, requestId: () => 'r1',
    hatch: async (...a) => { hatched.push(a); }, statRows: stats => { shown.push(stats); return new Node('div', document); } });
  menu.update(snapshot());
  const picks = () => root.all().filter(n => n.className === 'garden-care-slot');
  assert.deepEqual(picks().map(n => n.attributes['aria-label']), ['Bubbles', 'Cheese', 'Egg of Mine and Bubbles']);
  assert.equal(shown.at(-1).swim.level, 3, 'the selected partner shows in the stat window');
  const checks = root.all().filter(n => n.className === 'breeding-not').map(n => n.textContent);
  assert.deepEqual(checks, ['\u2716 Healthy'], 'the check-up shows what stops Bubbles');
  picks()[0].listeners.click();
  assert.deepEqual(sent, [{ type: 'life', requestId: 'r1', expectedRevision: 4, intent: { kind: 'breed', partnerId: 'b' } }]);
  menu.handleEvent({ kind: 'life-result', requestId: 'r1', accepted: true, code: 'offer-sent' });
  assert.ok(root.all().some(n => n.textContent === 'Bubbles got your call.'));
  picks()[2].listeners.click(); picks()[2].listeners.click();
  const name = root.all().find(n => n.className === 'garden-care-name');
  name.value = 'Kin'; name.listeners.input(); name.listeners.keydown({ key: 'Enter' });
  assert.deepEqual(hatched, [[{ eggId: 'egg-1' }, 'Kin']]);
});
