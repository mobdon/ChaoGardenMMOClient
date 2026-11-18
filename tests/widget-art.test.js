import test from 'node:test';
import assert from 'node:assert/strict';
import { drawWidgetEmeraldLayer, WIDGET_EMERALD_COLORS, widgetArtSubject, widgetCanvasPlacement,
  widgetEmeraldImagesReady, widgetEmeraldLayout } from '../src/widget-art.js';
import { widgetSnapshotView } from '../src/widget-state.js';

test('Chaos art preview contains no saved identity, owner privilege or fabricated progression', () => {
  assert.deepEqual(widgetArtSubject(null).subject, { form: 'dark-chaos', alignment: -100 });
  for (const variant of ['dark', 'neutral', 'hero', 'unknown', 'toString', '__proto__']) {
    const art = widgetArtSubject(null, variant);
    assert.equal(art.preview, true);
    assert.deepEqual(Object.keys(art.subject).sort(), ['alignment', 'form']);
    assert.equal(Object.isFrozen(art.subject), true);
    assert.equal(widgetSnapshotView(null).rings, null);
    assert.ok(widgetSnapshotView(null).stats.every(stat => stat.level === null));
  }
});

test('canonical Chao always overrides art selection, including unsupported normal forms and stale profiles', () => {
  const canonical = Object.freeze({ id: 'actual-chao', form: 'normal', alignment: 0, admin: false, rings: 13 });
  for (const variant of ['dark', 'neutral', 'hero']) {
    const art = widgetArtSubject(canonical, variant);
    assert.equal(art.subject, canonical);
    assert.equal(art.preview, false);
    assert.equal(art.subject.form, 'normal');
    assert.equal(art.subject.rings, 13);
  }
});

test('only literal canonical admin gets the fixed chest and all seven orbit emeralds', () => {
  const options = { canonical: true, x: 180, y: 196, size: 292, time: 0 };
  for (const admin of [false, undefined, 'true', 1]) assert.deepEqual(widgetEmeraldLayout({ admin }, options), []);
  assert.deepEqual(widgetEmeraldLayout({ admin: true }, { ...options, canonical: false }), []);
  assert.deepEqual(widgetEmeraldLayout(widgetArtSubject(null).subject, options), []);
  const layout = widgetEmeraldLayout({ admin: true }, options);
  assert.equal(layout.length, 8);
  assert.equal(layout.filter(prop => prop.layer === 'chest').length, 1);
  assert.equal(layout.find(prop => prop.layer === 'chest').color, 'cyan');
  assert.deepEqual(layout.filter(prop => prop.layer !== 'chest').map(prop => prop.color), [...WIDGET_EMERALD_COLORS]);
  const later = widgetEmeraldLayout({ admin: true }, { ...options, time: 4000 });
  assert.deepEqual(later.find(prop => prop.layer === 'chest'), layout.find(prop => prop.layer === 'chest'));
  assert.notDeepEqual(later.filter(prop => prop.layer !== 'chest'), layout.filter(prop => prop.layer !== 'chest'));
});

test('ornaments require the complete seven original square cells and retain uniform geometry', () => {
  const images = new Map(WIDGET_EMERALD_COLORS.map(color => [color, { color, complete: true, naturalWidth: 64, naturalHeight: 64 }]));
  assert.equal(widgetEmeraldImagesReady(images), true);
  const missing = new Map(images); missing.delete('white');
  assert.equal(widgetEmeraldImagesReady(missing), false);
  const stretched = new Map(images); stretched.set('white', { complete: true, naturalWidth: 64, naturalHeight: 32 });
  assert.equal(widgetEmeraldImagesReady(stretched), false);
  const painted = [];
  const context = { save() {}, restore() {}, drawImage(...args) { painted.push(args); } };
  const layout = widgetEmeraldLayout({ admin: true }, { canonical: true, x: 180, y: 196, size: 292 });
  for (const layer of ['far', 'chest', 'near']) drawWidgetEmeraldLayer(context, images, layout, layer);
  assert.equal(painted.length, 8);
  assert.ok(painted.every(([, , , width, height]) => width === height));
  assert.equal(context.globalCompositeOperation, 'source-over');
});

test('native front Idle feet land at the garden grass point with square scaling', () => {
  for (const variant of ['dark', 'hero', 'neutral']) {
    for (const size of ['compact', 'wide', 'large']) {
      const placement = widgetCanvasPlacement(176, 160, variant, size);
      assert.ok(Math.abs(placement.left - 109.12) < 0.0001);
      const alphaBottom = variant === 'neutral' ? 83 / 96 : 84 / 96;
      const actualFeet = placement.top + placement.size * (50 + 292 * alphaBottom) / 360;
      assert.ok(Math.abs(actualFeet - 108.8) < 0.0001);
      assert.ok(placement.top >= 0 && placement.size > 0);
    }
  }
  const inset = widgetCanvasPlacement(320, 164, 'dark');
  assert.ok(Math.abs(inset.left - 181.648) < 0.0001);
  assert.ok(Math.abs(inset.groundY - 111.52) < 0.0001);
});
