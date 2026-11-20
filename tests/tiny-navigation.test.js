import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';
import { durableTinyResult } from '../src/tiny-state.js';

const source = await readFile(new URL('../src/tiny.js', import.meta.url), 'utf8');
// Execute the actual small menu controller without importing the browser entry
// or creating account/game adapters. No browser or synthetic game authority.
const controller = source.slice(source.indexOf('const roundShortcut ='), source.indexOf('function renderConnection()'));

function element(dataset = {}) {
  return { dataset, hidden: false, disabled: false, attributes: {}, listeners: {}, children: [],
    setAttribute(name, value) { this.attributes[name] = value; }, removeAttribute(name) { delete this.attributes[name]; },
    append(...nodes) { this.children.push(...nodes); }, after(node) { this.afterNode = node; },
    addEventListener(kind, callback) { this.listeners[kind] = callback; }, focus() { this.focused = true; },
    click() { if (!this.disabled) this.listeners.click?.(); } };
}

function harness() {
  const screens = ['garden', 'stats', 'training', 'games', 'items'];
  const panels = screens.map(panel => element({ panel }));
  const buttons = screens.map(screen => element({ screen }));
  const nav = element(), stop = element(); let resets = 0, renders = 0, stops = 0, mutations = 0;
  stop.addEventListener('click', () => stops++);
  const location = { hash: '#garden' };
  const context = vm.createContext({ location, selectedScreen: 'garden', tinyGame: null, lastResultId: null,
    games: { memory: { title: 'Chao Memory' } }, durableTinyResult,
    history: { replaceState(_state, _title, hash) { location.hash = hash; } },
    resetMovement() { resets++; }, renderGame() { renders++; }, send() { mutations++; },
    $: () => stop,
    document: { createElement: () => element(),
      querySelector: selector => selector === '.tiny-screen-nav' ? nav : panels.find(panel => selector.includes(`"${panel.dataset.panel}"`)),
      querySelectorAll: selector => selector === '[data-panel]' ? panels : buttons } });
  vm.runInContext(`${controller}\nglobalThis.menu = { tinyScreenFromHash, selectScreen, renderRoundShortcut, roundShortcut, returnToRound, stopFromMenu };`, context);
  return { context, location, panels, buttons, stop, menu: context.menu,
    counts: () => ({ resets, renders, stops, mutations }) };
}

test('only the five literal menu hashes select screens', () => {
  const h = harness();
  assert.equal(h.menu.tinyScreenFromHash(''), 'garden');
  for (const screen of ['garden', 'stats', 'training', 'games', 'items']) {
    assert.equal(h.menu.tinyScreenFromHash(`#${screen}`), screen);
    h.menu.selectScreen(screen, true);
    assert.equal(h.location.hash, `#${screen}`);
    assert.deepEqual(h.panels.filter(panel => !panel.hidden).map(panel => panel.dataset.panel), [screen]);
    assert.equal(h.buttons.find(button => button.dataset.screen === screen).attributes['aria-current'], 'page');
  }
  for (const hash of ['#train-run', '#games?start=memory', '#Games', '#%67ames', '#<script>', 'games', null]) {
    assert.equal(h.menu.tinyScreenFromHash(hash), null);
  }
  assert.equal(h.counts().mutations, 0);
  assert.equal(h.counts().stops, 0);
  assert.equal(h.counts().resets, 5);
  assert.match(source, /window\.addEventListener\('hashchange'/);
  assert.match(source, /selectScreen\(tinyScreenFromHash\(location\.hash\) \|\| 'garden'\);\s*draw\(\)/);
});

test('leaving an active game keeps explicit Return and Stop controls', () => {
  const h = harness();
  h.context.tinyGame = { id: 'round-a', gameId: 'memory', phase: 'playing' };
  h.menu.selectScreen('stats'); h.menu.renderRoundShortcut();
  assert.equal(h.menu.roundShortcut.hidden, false);
  assert.match(h.menu.roundShortcut.children[0].textContent, /continues on the server/);
  assert.equal(h.menu.stopFromMenu.hidden, false);
  assert.equal(h.counts().stops, 0);
  h.menu.stopFromMenu.click(); assert.equal(h.counts().stops, 1);
  h.menu.returnToRound.click(); h.menu.renderRoundShortcut();
  assert.equal(h.location.hash, '#games'); assert.equal(h.menu.roundShortcut.hidden, true);
  assert.equal(h.counts().mutations, 0);
});

test('saving and saved rounds never expose a cancel shortcut', () => {
  const h = harness();
  h.menu.selectScreen('items');
  for (const phase of ['saving', 'finished']) {
    h.context.tinyGame = { id: 'round-a', gameId: 'memory', phase };
    h.menu.renderRoundShortcut();
    assert.equal(h.menu.stopFromMenu.hidden, true);
    assert.match(h.menu.roundShortcut.children[0].textContent, /saving on the server/);
  }
  h.context.tinyGame = { id: 'round-a', gameId: 'memory', phase: 'finished', result: { id: 'round-a', gameId: 'memory', rings: 5, saved: true } };
  h.menu.renderRoundShortcut();
  assert.equal(h.menu.stopFromMenu.hidden, true);
  assert.match(h.menu.roundShortcut.children[0].textContent, /saved round/);
  h.context.tinyGame = null; h.menu.renderRoundShortcut();
  assert.equal(h.menu.roundShortcut.hidden, true);
});
