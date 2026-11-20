// Present the server-chosen resident behaviour (`watch.behaviour`) with native
// Chao motion clips and faces. This is presentation only: the server owns the
// behaviour kind, variant, phase and timing; this module never moves the Chao,
// never grants anything and never runs while the Chao is actually moving.
//
// Clip selection comes from the local `chao-behaviour.glb` built by
// `scripts/build-chao-behaviour.js`. Its `asset.extras.chaoBehaviour.clips`
// rows carry each clip's native state index, name and original table
// properties (StartFrame, EndFrame, PlaySpeed, loop). Which native state shows
// which behaviour is a recorded mapping in `scripts/chao-behaviour-map.json`.
import { cloneNativeMotion } from './chao-locomotion.js';

export const BEHAVIOUR_KINDS = ['idle', 'sit', 'sleep', 'play', 'happy', 'startled'];
const MOVING_SPEED = 0.08;
const REFERENCE_FPS = 60;

// Provisional faces, from the appearance work's reading of SA2Enums ChaoEyes /
// ChaoMouth. They are unverified against a native behaviour call: a ported
// native controller's own face values replace these. Eyelid is 0..1 closed.
export const BEHAVIOUR_FACES = Object.freeze({
  idle: null,
  sit: { eye: 0, mouth: 3, eyelid: 0.35 },
  sleep: { eye: 2, mouth: 3, eyelid: 1 },
  play: { eye: 3, mouth: 5, eyelid: 0 },
  happy: { eye: 3, mouth: 1, eyelid: 0 },
  startled: { eye: 6, mouth: 11, eyelid: 0 },
});

let faceApi = null;
// The renderer's face module registers itself here (setChaoFace/resetChaoFace),
// so behaviour presentation does not depend on its load order.
export function registerChaoFaceApi(api) {
  faceApi = api && typeof api.setChaoFace === 'function' ? api : null;
}

/** Validate the published behaviour object; returns null for anything malformed. */
export function readBehaviour(watch) {
  const b = watch?.behaviour;
  if (!b || typeof b !== 'object' || !BEHAVIOUR_KINDS.includes(b.kind) && b.kind !== 'wander') return null;
  const numbers = ['startedAt', 'until', 'phaseStartedAt', 'phaseUntil'];
  if (!numbers.every(key => Number.isFinite(b[key]))) return null;
  if (!['enter', 'loop', 'exit'].includes(b.phase)) return null;
  const variant = Number.isInteger(b.variant) && b.variant >= 0 ? b.variant : 0;
  return { kind: b.kind, phase: b.phase, variant, startedAt: b.startedAt, until: b.until,
    phaseStartedAt: b.phaseStartedAt, phaseUntil: b.phaseUntil };
}

/** Read the clip catalog from a loaded behaviour GLB container. */
export function readCatalog(source) {
  const rows = source?.gltfAssetExtras?.chaoBehaviour?.clips ?? source?.chaoBehaviour?.clips ?? [];
  return rows.filter(row => BEHAVIOUR_KINDS.includes(row?.kind) && typeof row.nativeName === 'string'
    && ['enter', 'loop', 'exit'].includes(row.phase));
}

/** Choose the catalog row for a behaviour. Falls back from a missing phase to loop. */
export function chooseClip(catalog, behaviour) {
  if (!behaviour || behaviour.kind === 'wander') return null;
  for (const phase of [behaviour.phase, 'loop']) {
    const rows = catalog.filter(row => row.kind === behaviour.kind && row.phase === phase);
    if (rows.length) return rows[behaviour.variant % rows.length];
  }
  return null;
}

/**
 * Decide what to show for one frame. Returns null when locomotion should
 * play instead (moving, no behaviour, wander, or no matching native clip).
 * `serverNow` is the estimated current server time in milliseconds.
 */
export function selectBehaviourMotion(catalog, behaviour, { serverNow, speed = 0, fresh = true, mounted = false, swimming = false } = {}) {
  if (!behaviour || mounted || swimming || !fresh) return null;
  if (Number.isFinite(speed) && speed >= MOVING_SPEED) return null;
  const row = chooseClip(catalog, behaviour);
  if (!row) return null;
  const props = row.tableProperties || {};
  const start = Number.isFinite(props.StartFrame) ? props.StartFrame : row.frameRange?.[0] ?? 0;
  const end = Number.isFinite(props.EndFrame) ? props.EndFrame : row.frameRange?.[1] ?? start;
  const rate = Number.isFinite(props.PlaySpeed) && props.PlaySpeed > 0 ? props.PlaySpeed : 1;
  const elapsed = Math.max(0, (Number.isFinite(serverNow) ? serverNow : behaviour.phaseStartedAt) - behaviour.phaseStartedAt) / 1000;
  const loop = behaviour.phase === 'loop' && row.loop !== false;
  const span = Math.max(0, end - start);
  const advanced = elapsed * REFERENCE_FPS * rate;
  const frame = span === 0 ? start : loop ? start + advanced % span : Math.min(end, start + advanced);
  return { key: `${row.kind}:${row.phase}:${row.nativeName}`, row, frame, loop, rate,
    nativeState: row.nativeState ?? null, face: BEHAVIOUR_FACES[behaviour.kind] ?? null };
}

/** Clone every catalogued native clip onto this Chao's own rig. */
export function attachNativeBehaviours(imported, source, targetPose) {
  imported.behaviourCatalog = readCatalog(source);
  imported.behaviourClips = new Map();
  for (const row of imported.behaviourCatalog) {
    if (imported.behaviourClips.has(row.nativeName)) continue;
    const group = cloneNativeMotion(imported, source, targetPose, row.nativeName, `Behaviour${row.nativeState ?? ''}`);
    if (group) imported.behaviourClips.set(row.nativeName, group);
  }
  imported.behaviourCatalog = imported.behaviourCatalog.filter(row => imported.behaviourClips.has(row.nativeName));
  return imported;
}

/**
 * Apply the behaviour pose for this frame. Returns the selection when a
 * native behaviour clip took over, or null so the caller runs locomotion.
 */
export function updateNativeBehaviour(imported, root, behaviour, context) {
  const catalog = imported.behaviourCatalog || [];
  const motion = selectBehaviourMotion(catalog, behaviour, context);
  const group = motion && imported.behaviourClips?.get(motion.row.nativeName);
  if (!group) {
    if (imported.behaviourActive) {
      imported.behaviourActive = null;
      if (faceApi?.resetChaoFace) faceApi.resetChaoFace(root);
    }
    return null;
  }
  if (imported.current !== group) {
    imported.current?.stop();
    group.start(true, 1);
    group.pause();
    imported.current = group;
    imported.nativeMotionMode = 'Behaviour';
  }
  group.goToFrame(motion.frame);
  if (imported.behaviourActive !== motion.key) {
    imported.behaviourActive = motion.key;
    if (faceApi) {
      if (motion.face) faceApi.setChaoFace(root, motion.face);
      else faceApi.resetChaoFace?.(root);
    }
  }
  return motion;
}
