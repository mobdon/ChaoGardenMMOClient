import test from 'node:test';
import assert from 'node:assert/strict';

const images = new Map();
const media = { matches: false };
const originalImage = globalThis.Image;
const originalWindow = globalThis.window;
globalThis.Image = class {
  complete = true;
  set src(value) {
    this.url = value;
    this.naturalWidth = value.endsWith('chao-flame.png') ? 512 : 768;
    this.naturalHeight = value.endsWith('chao-flame.png') ? 32 : 768;
    images.set(value, this);
  }
  get src() { return this.url; }
};
globalThis.window = { matchMedia: () => media };
const { chaosVariant, chaoFormLabel, nativeDirection, drawTinyAvatar } = await import('../src/tiny-avatar.js');
if (originalImage === undefined) delete globalThis.Image; else globalThis.Image = originalImage;
if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;

function context() {
  const calls = [];
  return { calls, save() { calls.push(['save']); }, restore() { calls.push(['restore']); },
    drawImage(...args) { calls.push(['image', ...args, this.globalCompositeOperation]); },
    beginPath() {}, arc(...args) { calls.push(['arc', ...args]); }, fill() {}, stroke() {}, fillText(...args) { calls.push(['text', ...args]); } };
}

test('explicit Chaos form identity takes precedence over conflicting alignment', () => {
  assert.equal(chaosVariant({ form: 'hero_chaos', alignment: -100 }), 'hero');
  assert.equal(chaosVariant({ form: 'dark_chaos', alignment: 100 }), 'dark');
  assert.equal(chaosVariant({ form: 'neutral_chaos', alignment: 100 }), 'neutral');
  assert.equal(chaoFormLabel({ form: 'dark_chaos', alignment: 100 }), 'Dark Chaos Chao');
  assert.equal(chaoFormLabel({ form: 'neutral_chaos', alignment: -100 }), 'Neutral Chaos Chao');
});

test('generic Chaos alignment and normal Chao remain distinct', () => {
  for (const [alignment, expected] of [[-100, 'dark'], [-60, 'dark'], [-59, 'neutral'], [59, 'neutral'], [60, 'hero'], [100, 'hero']]) {
    assert.equal(chaosVariant({ form: 'chaos', alignment }), expected);
  }
  assert.equal(chaosVariant({ form: 'normal', alignment: -100 }), null);
  assert.equal(chaosVariant(null), null);
  assert.equal(chaoFormLabel({ form: 'normal' }), 'Neutral Chao');
});

test('eight native directions preserve atlas orientation and stationary heading', () => {
  for (const [x, y, direction] of [[0, 1, 0], [-1, 1, 1], [-1, 0, 2], [-1, -1, 3], [0, -1, 4], [1, -1, 5], [1, 0, 6], [1, 1, 7]]) {
    assert.equal(nativeDirection(x, y), direction);
  }
  assert.equal(nativeDirection(0, 0, 6), 6);
});

test('each Chaos form uses its own untouched atlas cells with a flame only for Dark', () => {
  for (const form of ['neutral', 'hero', 'dark']) {
    const ctx = context();
    assert.equal(drawTinyAvatar(ctx, { form: `${form}_chaos` }, 100, 120, 96, { time: 290, walking: true, direction: 7 }), true);
    const drawn = ctx.calls.filter(call => call[0] === 'image');
    assert.equal(drawn[0][1], images.get(`/api/assets/tiny-${form}-chaos.png`));
    assert.deepEqual(drawn[0].slice(2, 10), [6 * 96, 7 * 96, 96, 96, 52, 72, 96, 96]);
    assert.equal(drawn.length, form === 'dark' ? 2 : 1);
    if (form === 'dark') {
      assert.equal(drawn[1][1], images.get('/api/assets/chao-flame.png'));
      assert.equal(drawn[1].at(-1), 'lighter');
    }
    assert.equal(ctx.calls[0][0], 'save');
    assert.equal(ctx.calls.at(-1)[0], 'restore');
  }
});

test('reduced motion freezes native poses and head effect frames', () => {
  media.matches = true;
  try {
    const ctx = context();
    drawTinyAvatar(ctx, { form: 'dark_chaos' }, 0, 0, 100, { time: 99999, walking: true, direction: 2 });
    const drawn = ctx.calls.filter(call => call[0] === 'image');
    assert.equal(drawn[0][2], 4 * 96);
    assert.equal(drawn[1][2], 0);
  } finally { media.matches = false; }
});

test('missing or invalid native art uses an identity-neutral marker', () => {
  const image = images.get('/api/assets/tiny-hero-chaos.png');
  for (const invalid of [{ complete: false }, { naturalWidth: 512 }, { naturalHeight: 0 }]) {
    const before = { complete: image.complete, naturalWidth: image.naturalWidth, naturalHeight: image.naturalHeight };
    Object.assign(image, invalid);
    try {
      const ctx = context();
      assert.equal(drawTinyAvatar(ctx, { form: 'hero_chaos' }, 0, 0, 96), false);
      assert.equal(ctx.calls.filter(call => call[0] === 'image').length, 0);
      assert.equal(ctx.calls.find(call => call[0] === 'text')[1], '◈');
    } finally { Object.assign(image, before); }
  }
  const normal = context();
  assert.equal(drawTinyAvatar(normal, { form: 'normal' }, 0, 0, 96), false);
  assert.equal(normal.calls.filter(call => call[0] === 'image').length, 0);
});
