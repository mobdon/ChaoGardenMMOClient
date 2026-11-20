import test from 'node:test';
import assert from 'node:assert/strict';
import { FRAMES_PER_SECOND, advanceFrame, readBehaviour, readCatalog, registerChaoFaceApi, selectBehaviourMotion, updateNativeBehaviour } from '../src/chao-behaviour.js';

// Synthetic catalog for presentation tests only (not game data).
const catalog = readCatalog({ chaoBehaviour: { clips: [
  { nativeState: 0, nativeName: 'clip_a', tableProperties: { StartFrame: 0, EndFrame: 20, PlaySpeed: 1, LoopProperty: 0 } },
  { nativeState: 1, nativeName: 'clip_b', tableProperties: { StartFrame: 0, EndFrame: 10, PlaySpeed: 1, LoopProperty: 3 } },
  { nativeState: 2, nativeName: 'clip_c', tableProperties: { StartFrame: 5, EndFrame: 15, PlaySpeed: 1, LoopProperty: 7 } },
  { nativeState: 900, nativeName: 'out_of_range' },
] } });
const watch = (motion, extra = {}) => ({ behaviour: { fn: 'ALBHV_Think', address: '0x47830', mode: 0,
  motion: { num: 0, frame: 0, spd: 1, link: null, ...motion }, face: { eye: null, mouth: null },
  rules: 'native-gsne8p-chaomain', ...extra } });

test('malformed, legacy or placeholder payloads are ignored', () => {
  assert.equal(readBehaviour(null), null);
  assert.equal(readBehaviour({ activity: 'idle', behaviour: null }), null, 'no native data means locomotion');
  assert.equal(readBehaviour({ behaviour: { kind: 'sit', phase: 'loop' } }), null, 'old placeholder shape is rejected');
  assert.equal(readBehaviour(watch({ num: 622 })), null);
  assert.equal(readBehaviour(watch({ frame: 'x' })), null);
  assert.equal(readBehaviour(watch({ link: { target: 3, ratio: 'half', spd: .1 } })), null);
  const b = readBehaviour(watch({ num: 1, link: { target: 2, ratio: .5, spd: .1, selfLoop: false } }, { face: { eye: 3, mouth: -1 } }), 42);
  assert.deepEqual(b.motion.link, { target: 2, ratio: .5, spd: .1, selfLoop: false });
  assert.deepEqual(b.face, { eye: 3, mouth: null });
  assert.equal(b.receivedAt, 42);
  assert.equal(catalog.size, 3, 'states outside 0..621 are dropped');
});

test('frames extrapolate by the native mode between snapshots', () => {
  assert.ok(Math.abs(FRAMES_PER_SECOND - 59.94) < .01);
  assert.equal(advanceFrame(catalog.get(0), 18, 1, 4), 2, 'LOOP wraps');
  assert.equal(advanceFrame(catalog.get(1), 8, 1, 4), 10, 'STOP holds its end');
  assert.equal(advanceFrame(catalog.get(2), 13, 1, 4), 13, 'BACK ping-pongs');
  const ms = 10 / FRAMES_PER_SECOND * 1000;
  const shown = selectBehaviourMotion(catalog, readBehaviour(watch({ frame: 2 }), 0), { now: ms });
  assert.ok(Math.abs(shown.frame - 12) < 1e-9);
  const late = selectBehaviourMotion(catalog, readBehaviour(watch({ num: 1, frame: 0 }), 0), { now: 1e6 });
  assert.equal(late.frame, 10, 'extrapolation is bounded and clamped');
});

test('a link blends source and target by the native ratio', () => {
  const b = readBehaviour(watch({ num: 0, frame: 7, link: { target: 2, ratio: .25, spd: .05, selfLoop: false } }), 0);
  const m = selectBehaviourMotion(catalog, b, { now: 5 / FRAMES_PER_SECOND * 1000 });
  assert.equal(m.frame, 7, 'plain link freezes the source pose');
  assert.equal(m.blend.frame, 5, 'target shows its start frame');
  assert.ok(Math.abs(m.blend.weight - .5) < 1e-9);
});

test('no override when mounted, swimming, stale or the state has no clip', () => {
  const b = readBehaviour(watch({}), 0);
  assert.ok(selectBehaviourMotion(catalog, b, { now: 0 }));
  assert.equal(selectBehaviourMotion(catalog, b, { now: 0, mounted: true }), null);
  assert.equal(selectBehaviourMotion(catalog, b, { now: 0, swimming: true }), null);
  assert.equal(selectBehaviourMotion(catalog, b, { now: 0, fresh: false }), null);
  assert.equal(selectBehaviourMotion(catalog, readBehaviour(watch({ num: 5 }), 0), { now: 0 }), null);
});

test('update drives paused native groups and the face API once per change', () => {
  const calls = [];
  registerChaoFaceApi({ setChaoFace: (root, face) => calls.push(['set', face]), resetChaoFace: () => calls.push(['reset']) });
  const group = name => ({ name, frames: [], weights: [], isPlaying: false, started: 0, stopped: 0,
    start() { this.started++; this.isPlaying = true; }, pause() {}, stop() { this.stopped++; this.isPlaying = false; },
    setWeightForAllAnimatables(w) { this.weights.push(w); }, goToFrame(frame) { this.frames.push(frame); } });
  const clips = new Map([...catalog.values()].map(row => [row.nativeName, group(row.nativeName)]));
  const walk = group('walk');
  const imported = { behaviourCatalog: catalog, behaviourClips: clips, current: walk };
  const root = {};
  assert.ok(updateNativeBehaviour(imported, root, readBehaviour(watch({}), 0), { now: 0 }));
  assert.equal(walk.stopped, 1); assert.equal(imported.current, clips.get('clip_a'));
  assert.deepEqual(calls, [['reset']], 'no native face numbers means the default face');
  updateNativeBehaviour(imported, root, readBehaviour(watch({}, { face: { eye: 3, mouth: 3 } }), 0), { now: 0 });
  assert.equal(clips.get('clip_a').started, 1, 'no restart while the same clip continues');
  assert.deepEqual(calls.at(-1), ['set', { eye: 3, mouth: 3 }]);
  updateNativeBehaviour(imported, root, readBehaviour(watch({ link: { target: 2, ratio: .5, spd: 0 } }), 0), { now: 0 });
  assert.equal(clips.get('clip_c').weights.at(-1), .5);
  assert.equal(clips.get('clip_a').weights.at(-1), .5);
  assert.equal(updateNativeBehaviour(imported, root, readBehaviour(watch({}), 0), { now: 0, mounted: true }), null);
  assert.deepEqual(calls.at(-1), ['reset']);
  assert.equal(clips.get('clip_c').isPlaying, false);
  registerChaoFaceApi(null);
});
