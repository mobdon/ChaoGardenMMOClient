import test from 'node:test';
import assert from 'node:assert/strict';
import { createCourtship, heartScale, STATE } from '../src/breeding-scene.js';

// Motion table rows as the manifest has them (info.ini), trimmed to what the scene plays.
const row = (name, t) => ({ nativeName: name, tableProperties: { StartFrame: 0, EndFrame: 0, PlaySpeed: 0, LoopProperty: 0, NextAnimation: -1, ...t } });
const rows = new Map([
  [STATE.STAND, row('alm_osuwari', { StartFrame: 10, EndFrame: 0, PlaySpeed: -0.2, LoopProperty: 3 })],
  [STATE.NADE, row('alm_nade_kimotii', { EndFrame: 7, PlaySpeed: 0.08, LoopProperty: 1 })],
  [STATE.HASYAGU, row('alm_hasyagu', { EndFrame: 5, PlaySpeed: 0.2, LoopProperty: 6, NextAnimation: 469 })],
  [STATE.HASYAGU + 1, row('alm_hasyagu', { StartFrame: 5, EndFrame: 6, PlaySpeed: 0.2, LoopProperty: 7 })],
  [STATE.BANZAI, row('alm_jump_banzai', { EndFrame: 9, PlaySpeed: 0.22, LoopProperty: 3 })],
]);

test('the jingle starts at contact, both sway, both cheer on frame 302, then banzai until done', () => {
  const c = createCourtship(rows, () => 0);
  assert.deepEqual(c.step(), ['jingle']);
  assert.equal(c.answerer.state, STATE.NADE);
  for (let i = 1; i < 301; i++) c.step();
  assert.equal(c.answerer.state, STATE.NADE, 'still swaying before frame 302');
  c.step();
  assert.equal(c.answerer.state, STATE.HASYAGU);
  assert.equal(c.inviter.state, STATE.HASYAGU);
  let frames = 0, egg = null;
  while (!c.done && frames++ < 5000) if (c.step().includes('egg')) egg = frames;
  assert.equal(egg, 20, 'the egg on the cheer\u2019s 21st frame');
  assert.ok(c.done, 'the scene ends');
});

test('the heart grows 0.2 a frame to 2.8 and shrinks 0.25 a frame', () => {
  assert.equal(heartScale(5, 60), 1);
  assert.equal(heartScale(30, 60), 2.8);
  assert.ok(Math.abs(heartScale(64, 60) - 1.8) < 1e-9);
  assert.equal(heartScale(200, 60), 0);
});
