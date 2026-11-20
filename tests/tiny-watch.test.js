import test from 'node:test';
import assert from 'node:assert/strict';

const originalImage = globalThis.Image, originalWindow = globalThis.window;
globalThis.Image = class { complete = true; naturalWidth = 768; naturalHeight = 768;
  set src(value) { this.url = value; if (value.endsWith('chao-flame.png')) { this.naturalWidth = 512; this.naturalHeight = 32; } }
  get src() { return this.url; }
};
globalThis.window = { matchMedia: () => ({ matches: false }) };
const { createTinyWatch } = await import('../src/tiny-watch.js');
if (originalImage === undefined) delete globalThis.Image; else globalThis.Image = originalImage;
if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;

function context() {
  const calls = [];
  return { calls, save() {}, restore() {}, scale(...args) { calls.push(['scale', ...args]); },
    drawImage(...args) { calls.push(['image', ...args]); }, clearRect() {}, fillRect() {}, beginPath() {}, ellipse() {}, fill() {}, arc() {}, stroke() {},
    fillText(...args) { calls.push(['text', ...args]); } };
}
function element(tag = 'div') {
  return { tag, style: {}, dataset: {}, children: [], attributes: {}, listeners: {}, textContent: '', hidden: false,
    context: tag === 'canvas' ? context() : null,
    setAttribute(name, value) { this.attributes[name] = value; }, getContext() { return this.context; },
    append(...nodes) { this.children.push(...nodes); }, after(node) { this.afterNode = node; },
    querySelectorAll() { return this.children; }, addEventListener(kind, callback) { this.listeners[kind] = callback; } };
}
function harness() {
  const ids = ['tiny-watch-canvas', 'tiny-watch-status', 'tiny-watch-copy', 'tiny-watch-toggle', 'tiny-watch-care', 'tiny-watch-needs'];
  const nodes = new Map(ids.map(id => [id, element(id.endsWith('canvas') ? 'canvas' : 'div')]));
  for (const kind of ['pet', 'rest', 'wake']) nodes.get('tiny-watch-care').append({ ...element('button'), dataset: { care: kind } });
  const images = new Map(), sent = [], created = [];
  const oldDocument = globalThis.document, oldImage = globalThis.Image;
  globalThis.document = { querySelector: selector => nodes.get(selector.slice(1)), createElement(tag) { const node = element(tag); created.push(node); return node; } };
  globalThis.Image = class { complete = false; naturalWidth = 0; naturalHeight = 0;
    set src(value) { this.url = value; images.set(value, this); } get src() { return this.url; }
  };
  let watch;
  try { watch = createTinyWatch({ send: message => { sent.push(message); return true; } }); }
  finally {
    if (oldDocument === undefined) delete globalThis.document; else globalThis.document = oldDocument;
    if (oldImage === undefined) delete globalThis.Image; else globalThis.Image = oldImage;
  }
  return { watch, nodes, images, sent, created,
    canvas: nodes.get('tiny-watch-canvas'),
    loadArt() {
      for (const [path, image] of images) {
        const gem = path.includes('widget-emerald');
        Object.assign(image, { complete: true, naturalWidth: gem ? 64 : path.includes('sidebar') ? 72 : 176, naturalHeight: gem ? 64 : 160 }); image.onload();
      }
    } };
}
const profile = { id: 'canonical-chao', name: 'Actual', form: 'dark-chaos', alignment: 100,
  x: 10, z: 20, happiness: 71, hunger: 63, energy: 82, rings: 0,
  stats: { swim: { level: 12 }, run: { level: 4 } }, watch: { id: 'watch-a', activity: 'idle' } };
const atlasCalls = h => h.created.flatMap(node => node.context?.calls || [])
  .filter(call => call[0] === 'image' && call[1]?.src?.includes('/api/assets/tiny-'));

test('native screen contains original garden and cropped sidebar with explicit missing-art state', () => {
  const h = harness();
  assert.equal(h.canvas.width / h.canvas.height, 240 / 160);
  assert.equal(h.canvas.style.aspectRatio, '3 / 2');
  assert.match(h.canvas.afterNode.textContent, /Original garden art unavailable/);
  h.watch.draw(0); assert.equal(atlasCalls(h).length, 0);
  h.loadArt();
  const background = h.created.find(node => node.tag === 'canvas' && node.width === 240);
  assert.equal(background.width, 240); assert.equal(background.height, 160);
  const draws = background.context.calls.filter(call => call[0] === 'image');
  assert.ok(draws.some(call => call[1].src.endsWith('tiny-garden-a2ne.png') && call[2] === 0 && call[3] === 0));
  assert.ok(draws.some(call => call[1].src.endsWith('tiny-garden-a2ne-sidebar-1.png') && call[2] === 171 && call[3] === 0));
  assert.match(h.canvas.afterNode.textContent, /Connect to view your actual Chao/);
  h.watch.draw(0);
  assert.equal(h.canvas.context.calls.filter(call => call[0] === 'text' && call[1] === '0').length, 0);
});

test('same canonical Chao stays small, grounded and moves only from received positions', () => {
  const h = harness(); h.loadArt(); h.watch.update(profile, { connected: true, busy: false }); h.watch.draw(0);
  const drawn = atlasCalls(h).at(-1);
  assert.ok(drawn[1].src.endsWith('tiny-dark-chaos.png'));
  assert.equal(drawn[8], 32); assert.equal(drawn[9], 32);
  assert.ok(Math.abs(drawn[6] + 16 - 109.12) < 0.0001);
  assert.ok(Math.abs(drawn[7] + 32 * 84 / 96 - 108.8) < 0.0001);
  h.watch.draw(10000); assert.equal(atlasCalls(h).at(-1)[6], drawn[6]);
  h.watch.update({ ...profile, x: 11, watch: { ...profile.watch, activity: 'wander' } }, { connected: true, busy: false });
  h.watch.draw(performance.now());
  assert.ok(Math.abs(atlasCalls(h).at(-1)[6] - drawn[6] - 8.8) < 0.0001);
  assert.equal(h.sent.length, 0);
});

test('sidebar shows canonical values and unknown fields stay unknown', () => {
  const h = harness(); h.loadArt(); h.watch.update(profile, { connected: true, busy: false }); h.watch.draw(0);
  const values = h.canvas.context.calls.filter(call => call[0] === 'text');
  assert.ok(values.some(call => call[1] === '71' && call[3] === 39));
  assert.ok(values.some(call => call[1] === '63' && call[3] === 55));
  assert.ok(values.some(call => call[1] === '12' && call[3] === 71));
  assert.ok(values.some(call => call[1] === '—' && call[3] === 87));
  assert.ok(values.some(call => call[1] === '0' && call[3] === 158));
  for (const node of h.created) if (node.context) node.context.calls.length = 0;
  h.watch.update({ ...profile, x: undefined }, { connected: false, busy: false }); h.watch.draw(1000);
  assert.equal(atlasCalls(h).length, 0);
  assert.match(h.canvas.afterNode.textContent, /Chao position unavailable/);
});

test('original scene keeps bounded care intents and offline care disabled', () => {
  const h = harness(); h.loadArt(); h.watch.update(profile, { connected: true, busy: false });
  const care = h.nodes.get('tiny-watch-care'), pet = care.children[0];
  care.listeners.click({ target: { closest: () => pet } });
  assert.deepEqual(h.sent, [{ type: 'action', action: 'watch-care', sessionId: 'watch-a', kind: 'pet' }]);
  h.watch.update({ ...profile, watch: { ...profile.watch, lastCare: { kind: 'pet', at: 100 } } }, { connected: true, busy: false });
  h.watch.update(profile, { connected: false, busy: false });
  care.listeners.click({ target: { closest: () => pet } });
  assert.equal(h.sent.length, 1); assert.equal(pet.disabled, true);
});

test('fresh observation stays live when another device controls care', () => {
  const h = harness(); h.loadArt(); h.watch.update(profile, { connected: true, reconciled: true, busy: false });
  h.watch.draw(0); const firstX = atlasCalls(h).at(-1)[6];
  h.watch.update({ ...profile, x: profile.x + 1, watch: { ...profile.watch, activity: 'rest' } },
    { connected: false, reconciled: true, busy: false });
  h.canvas.context.calls.length = 0; h.watch.draw(performance.now());
  assert.match(h.canvas.afterNode.textContent, /Live values/);
  assert.equal(h.nodes.get('tiny-watch-status').textContent, 'Resting');
  assert.equal(h.nodes.get('tiny-watch-toggle').disabled, true);
  assert.match(h.nodes.get('tiny-watch-toggle').textContent, /another device/);
  assert.ok(h.canvas.context.calls.some(call => call[0] === 'text' && call[1] === 'LIVE'));
  assert.ok(h.canvas.context.calls.some(call => call[0] === 'text' && call[1] === 'z z'));
  assert.ok(Math.abs(atlasCalls(h).at(-1)[6] - firstX - 8.8) < 0.0001);
  const care = h.nodes.get('tiny-watch-care'), pet = care.children[0];
  assert.equal(care.hidden, false); assert.equal(pet.disabled, true);
  care.listeners.click({ target: { closest: () => pet } });
  assert.equal(h.sent.length, 0);
  h.watch.update(profile, { connected: false, reconciled: false, busy: false });
  h.canvas.context.calls.length = 0; h.watch.draw(performance.now());
  assert.match(h.canvas.afterNode.textContent, /Last synced/);
  assert.ok(h.canvas.context.calls.some(call => call[0] === 'text' && call[1] === 'CACHED'));
  assert.equal(h.nodes.get('tiny-watch-status').textContent, 'Visit paused');
});

test('canonical owner retains fixed chest and complete seven-gem group; partial art keeps the actual body', () => {
  const h = harness(); h.loadArt(); h.watch.update({ ...profile, admin: true }, { connected: true, busy: false }); h.watch.draw(0);
  const gems = () => h.canvas.context.calls.filter(call => call[0] === 'image' && call[1]?.src?.includes('/widget-emerald-'));
  assert.equal(gems().length, 8);
  const body = h.canvas.context.calls.findIndex(call => call[0] === 'image' && call[1]?.width === 720);
  const far = h.canvas.context.calls.findIndex(call => call[0] === 'image' && call[1]?.src?.includes('/widget-emerald-'));
  assert.ok(far < body);
  const chest = gems().find(call => call[1].src.endsWith('widget-emerald-cyan.png') && call[4] === call[5] && call[4] < 3.5);
  assert.ok(chest);
  const chestPosition = chest.slice(2);
  h.canvas.context.calls.length = 0; h.watch.draw(3000);
  assert.deepEqual(gems().find(call => call[1].src.endsWith('widget-emerald-cyan.png') && call[4] < 3.5).slice(2), chestPosition);
  h.images.get('/__widget-art/widget-emerald-white.png').naturalWidth = 0;
  h.images.get('/__widget-art/widget-emerald-white.png').onerror();
  h.canvas.context.calls.length = 0; h.watch.draw(4000);
  assert.equal(gems().length, 0); assert.ok(atlasCalls(h).length > 0);
  assert.match(h.canvas.afterNode.textContent, /Owner ornaments unavailable/);
  for (const admin of [false, 'true', 1]) {
    h.watch.update({ ...profile, admin }, { connected: true, busy: false }); h.canvas.context.calls.length = 0; h.watch.draw(0);
    assert.equal(gems().length, 0);
  }
});
