import test from 'node:test';
import assert from 'node:assert/strict';
import { BEHAVIOUR_FACES, chooseClip, readBehaviour, readCatalog, registerChaoFaceApi, selectBehaviourMotion, updateNativeBehaviour } from '../src/chao-behaviour.js';

const behaviour = (kind, phase = 'loop', extra = {}) => ({ kind, phase, variant: 0, startedAt: 1000, until: 9000,
  phaseStartedAt: 1000, phaseUntil: 9000, rules: 'placeholder-tuning', ...extra });
const catalog = readCatalog({ chaoBehaviour: { clips: [
  { kind: 'sit', phase: 'enter', nativeName: 'clip_sit_down', nativeState: 1, tableProperties: { StartFrame: 0, EndFrame: 10, PlaySpeed: .5 } },
  { kind: 'sit', phase: 'loop', nativeName: 'clip_sit', nativeState: 2, tableProperties: { StartFrame: 0, EndFrame: 20, PlaySpeed: 1 } },
  { kind: 'play', phase: 'loop', nativeName: 'clip_play_a', nativeState: 3 },
  { kind: 'play', phase: 'loop', nativeName: 'clip_play_b', nativeState: 4 },
  { kind: 'bogus', phase: 'loop', nativeName: 'x' },
] } });

test('malformed or legacy watch payloads are ignored', () => {
  assert.equal(readBehaviour(null), null);
  assert.equal(readBehaviour({ activity: 'idle' }), null, 'servers without behaviour keep legacy presentation');
  assert.equal(readBehaviour({ behaviour: behaviour('dance') }), null);
  assert.equal(readBehaviour({ behaviour: { ...behaviour('sit'), until: 'soon' } }), null);
  assert.equal(readBehaviour({ behaviour: behaviour('sit', 'enter') }).phase, 'enter');
  assert.equal(readBehaviour({ behaviour: { ...behaviour('sit'), variant: -3 } }).variant, 0);
  assert.equal(catalog.length, 4, 'unknown kinds are dropped from the catalog');
});

test('server variant picks the same clip for every observer and phases fall back to loop', () => {
  assert.equal(chooseClip(catalog, behaviour('play', 'loop', { variant: 0 })).nativeName, 'clip_play_a');
  assert.equal(chooseClip(catalog, behaviour('play', 'loop', { variant: 3 })).nativeName, 'clip_play_b');
  assert.equal(chooseClip(catalog, behaviour('sit', 'exit')).nativeName, 'clip_sit', 'missing exit clip falls back to loop');
  assert.equal(chooseClip(catalog, behaviour('wander')), null, 'wander always uses locomotion');
  assert.equal(chooseClip(catalog, behaviour('sleep')), null, 'no native sleep clip means locomotion idle');
});

test('native table timing samples enter once and loops the loop phase', () => {
  const enter = selectBehaviourMotion(catalog, behaviour('sit', 'enter'), { serverNow: 1000 + 1000 / 60 * 8 });
  assert.equal(enter.loop, false); assert.ok(Math.abs(enter.frame - 4) < 1e-9, 'PlaySpeed .5 at 60 reference frames');
  const late = selectBehaviourMotion(catalog, behaviour('sit', 'enter'), { serverNow: 60000 });
  assert.equal(late.frame, 10, 'enter holds its end frame');
  const loop = selectBehaviourMotion(catalog, behaviour('sit'), { serverNow: 1000 + 1000 / 60 * 25 });
  assert.equal(loop.loop, true); assert.ok(Math.abs(loop.frame - 5) < 1e-9);
  assert.equal(loop.face, null, 'no face without a traced native value');
  assert.ok(Object.values(BEHAVIOUR_FACES).every(face => face === null));
});

test('behaviour never overrides movement, mounts, swimming or stale snapshots', () => {
  const sit = behaviour('sit');
  assert.ok(selectBehaviourMotion(catalog, sit, { serverNow: 2000, speed: 0 }));
  assert.equal(selectBehaviourMotion(catalog, sit, { serverNow: 2000, speed: .5 }), null);
  assert.equal(selectBehaviourMotion(catalog, sit, { serverNow: 2000, mounted: true }), null);
  assert.equal(selectBehaviourMotion(catalog, sit, { serverNow: 2000, swimming: true }), null);
  assert.equal(selectBehaviourMotion(catalog, sit, { serverNow: 2000, fresh: false }), null);
});

test('update drives a paused native group and the face API once per change', () => {
  const calls = [];
  registerChaoFaceApi({ setChaoFace: (root, face) => calls.push(['set', face]), resetChaoFace: () => calls.push(['reset']) });
  const group = name => ({ name, frames: [], started: 0, stopped: 0, start() { this.started++; }, pause() {}, stop() { this.stopped++; }, goToFrame(frame) { this.frames.push(frame); } });
  const clips = new Map(catalog.map(row => [row.nativeName, group(row.nativeName)]));
  const walk = group('walk');
  const imported = { behaviourCatalog: catalog, behaviourClips: clips, current: walk };
  const root = {};
  assert.ok(updateNativeBehaviour(imported, root, behaviour('sit'), { serverNow: 1100 }));
  assert.equal(walk.stopped, 1); assert.equal(imported.current, clips.get('clip_sit'));
  updateNativeBehaviour(imported, root, behaviour('sit'), { serverNow: 1200 });
  assert.equal(clips.get('clip_sit').started, 1, 'no restart while the same clip continues');
  assert.equal(calls.length, 1);
  assert.equal(updateNativeBehaviour(imported, root, behaviour('sit'), { serverNow: 1300, speed: 1 }), null);
  assert.deepEqual(calls.at(-1), ['reset']);
  registerChaoFaceApi(null);
});

test('recorded native state map is complete and well formed', async () => {
  const { readFile } = await import('node:fs/promises');
  const map = JSON.parse(await readFile(new URL('../scripts/chao-behaviour-map.json', import.meta.url), 'utf8'));
  const keys = new Set();
  for (const row of map.clips) {
    assert.ok(['idle', 'sit', 'sleep', 'play', 'happy', 'startled'].includes(row.kind));
    assert.ok(['enter', 'loop', 'exit'].includes(row.phase));
    assert.ok(Number.isInteger(row.nativeState) && row.nativeState >= 0 && row.nativeState < 622);
    assert.match(row.nativeName, /^alm_[a-z_]+$/);
    assert.ok(row.evidence, 'every mapping records its evidence');
    const key = `${row.kind}:${row.phase}:${row.variant}`; assert.ok(!keys.has(key), key); keys.add(key);
  }
  for (const kind of ['idle', 'sit', 'sleep', 'play', 'happy', 'startled']) assert.ok(keys.has(`${kind}:loop:0`), `${kind} needs a loop clip`);
  // Server variant counts (src/behaviour.rs Kind::variants) are covered.
  for (const [kind, count] of [['idle', 3], ['play', 4], ['sit', 2]]) assert.ok(keys.has(`${kind}:loop:${count - 1}`));
});
