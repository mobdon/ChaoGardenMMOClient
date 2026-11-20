// Present the server's native Chao behaviour (`watch.behaviour`). The server
// runs the ChaoMain behaviour port and main.dol MotionControl, and publishes
// the native motion state number (0..621), its frame and speed, any running
// link (blend) and the native face numbers. This module only plays that state's
// clip from the local `chao-behaviour.glb` and extrapolates between snapshots;
// it never chooses a behaviour, moves the Chao or grants anything.
//
// The GLB's `asset.extras.chaoBehaviour.clips` rows are keyed by native state
// number and carry the state's table row (StartFrame, EndFrame, PlaySpeed,
// LoopProperty) from the local manifest; see `scripts/build-chao-behaviour.js`.
import { cloneNativeMotion } from './chao-locomotion.js';

/** Native logic rate: one frame per NTSC field. */
export const FRAMES_PER_SECOND = 60000 / 1001;
export const MOTION_STATES = 622;
// main.dol MotionControl modes.
const LOOP = 0, BACK = 7;
// Extrapolate at most this far past a snapshot; the next one corrects it.
const MAX_EXTRAPOLATE_FRAMES = 30;

let faceApi = null;
// The renderer's face module registers itself here (setChaoFace/resetChaoFace),
// so behaviour presentation does not depend on its load order.
export function registerChaoFaceApi(api) {
  faceApi = api && typeof api.setChaoFace === 'function' ? api : null;
}

const state = value => Number.isInteger(value) && value >= 0 && value < MOTION_STATES;
const face = value => Number.isInteger(value) && value >= 0 ? value : null;

/** Validate the published native behaviour; null for anything malformed or legacy. */
export function readBehaviour(watch, receivedAt = 0) {
  const b = watch?.behaviour;
  if (!b || typeof b !== 'object' || typeof b.fn !== 'string') return null;
  const m = b.motion;
  if (!m || !state(m.num) || !Number.isFinite(m.frame) || !Number.isFinite(m.spd)) return null;
  let link = null;
  if (m.link != null) {
    const l = m.link;
    if (!state(l.target) || !Number.isFinite(l.ratio) || !Number.isFinite(l.spd)) return null;
    link = { target: l.target, ratio: l.ratio, spd: l.spd, selfLoop: l.selfLoop === true };
  }
  return { fn: b.fn, motion: { num: m.num, frame: m.frame, spd: m.spd, link },
    face: { eye: face(b.face?.eye), mouth: face(b.face?.mouth) }, receivedAt };
}

/** Read the state catalog from a loaded behaviour GLB container, keyed by native state. */
export function readCatalog(source) {
  const rows = source?.gltfAssetExtras?.chaoBehaviour?.clips ?? source?.chaoBehaviour?.clips ?? [];
  const catalog = new Map();
  for (const row of rows) {
    if (state(row?.nativeState) && typeof row.nativeName === 'string') catalog.set(row.nativeState, row);
  }
  return catalog;
}

function table(row) {
  const t = row.tableProperties || {};
  const n = (value, fallback) => Number.isFinite(value) ? value : fallback;
  return { start: n(t.StartFrame, 0), end: n(t.EndFrame, 0), mode: n(t.LoopProperty, 0) };
}

/** Advance a published frame by `frames` native frames within the state's range. */
export function advanceFrame(row, frame, spd, frames) {
  const { start, end, mode } = table(row);
  const lo = Math.min(start, end), hi = Math.max(start, end), span = hi - lo;
  const next = frame + spd * frames;
  if (span <= 0) return lo;
  if (mode === LOOP) return lo + ((next - lo) % span + span) % span;
  if (mode === BACK) {
    const t = ((next - lo) % (2 * span) + 2 * span) % (2 * span);
    return lo + (t <= span ? t : 2 * span - t);
  }
  return Math.min(hi, Math.max(lo, next));
}

/**
 * Decide what to show for one frame. Returns null when locomotion should play
 * instead (no native behaviour, mounted, swimming, stale, or no clip for the
 * state). `now` and `behaviour.receivedAt` are local milliseconds.
 */
export function selectBehaviourMotion(catalog, behaviour, { now, fresh = true, mounted = false, swimming = false } = {}) {
  if (!behaviour || mounted || swimming || !fresh) return null;
  const { num, frame, spd, link } = behaviour.motion;
  const row = catalog.get(num);
  if (!row) return null;
  const elapsed = Number.isFinite(now) ? Math.max(0, now - behaviour.receivedAt) : 0;
  const frames = Math.min(MAX_EXTRAPOLATE_FRAMES, elapsed / 1000 * FRAMES_PER_SECOND);
  const out = { key: `${num}:${row.nativeName}`, nativeState: num, row, fn: behaviour.fn, face: behaviour.face };
  if (link) {
    const target = catalog.get(link.target);
    const weight = Math.min(1, Math.max(0, link.ratio + link.spd * frames));
    // A plain link freezes both poses: the source at its frame, the target at
    // its start. A self loop (LKLP) blends the clip's end into its start.
    if (!target) return { ...out, frame, blend: null };
    return { ...out, frame, blend: { nativeState: link.target, row: target, frame: table(target).start, weight } };
  }
  return { ...out, frame: advanceFrame(row, frame, spd, frames), blend: null };
}

/** Clone every catalogued native clip onto this Chao's own rig. */
export function attachNativeBehaviours(imported, source, targetPose) {
  const catalog = readCatalog(source);
  imported.behaviourClips = new Map();
  for (const row of catalog.values()) {
    if (imported.behaviourClips.has(row.nativeName)) continue;
    const group = cloneNativeMotion(imported, source, targetPose, row.nativeName, `Behaviour${row.nativeState}`);
    if (group) imported.behaviourClips.set(row.nativeName, group);
  }
  for (const [num, row] of catalog) if (!imported.behaviourClips.has(row.nativeName)) catalog.delete(num);
  imported.behaviourCatalog = catalog;
  return imported;
}

function show(group, frame, weight) {
  if (!group.isPlaying) { group.start(true, 1); group.pause(); }
  group.setWeightForAllAnimatables(weight);
  group.goToFrame(frame);
}

function applyFace(root, f) {
  if (!faceApi) return;
  if (f && (f.eye !== null || f.mouth !== null)) faceApi.setChaoFace(root, f);
  else faceApi.resetChaoFace?.(root);
}

/**
 * Apply the native pose for this frame. Returns the selection when a native
 * clip took over, or null so the caller runs locomotion.
 */
export function updateNativeBehaviour(imported, root, behaviour, context) {
  const motion = selectBehaviourMotion(imported.behaviourCatalog || new Map(), behaviour, context);
  const clips = imported.behaviourClips;
  const group = motion && clips?.get(motion.row.nativeName);
  const blendGroup = motion?.blend && clips?.get(motion.blend.row.nativeName);
  if (!group) {
    if (imported.behaviourActive) {
      for (const g of imported.behaviourGroups || []) g.stop();
      imported.behaviourGroups = [];
      imported.behaviourActive = null;
      imported.behaviourFace = null;
      if (faceApi?.resetChaoFace) faceApi.resetChaoFace(root);
    }
    return null;
  }
  const active = blendGroup && blendGroup !== group ? [group, blendGroup] : [group];
  if (imported.current && !active.includes(imported.current)) imported.current.stop();
  for (const g of imported.behaviourGroups || []) if (!active.includes(g)) g.stop();
  imported.current = group;
  imported.nativeMotionMode = 'Behaviour';
  imported.behaviourGroups = active;
  if (blendGroup && blendGroup !== group) {
    show(group, motion.frame, 1 - motion.blend.weight);
    show(blendGroup, motion.blend.frame, motion.blend.weight);
  } else {
    // A self-loop link blends the same clip toward its start; show the source frame.
    show(group, motion.frame, 1);
  }
  imported.behaviourActive = motion.key;
  const faceKey = `${motion.face.eye}:${motion.face.mouth}`;
  if (imported.behaviourFace !== faceKey) {
    imported.behaviourFace = faceKey;
    applyFace(root, motion.face);
  }
  return motion;
}
