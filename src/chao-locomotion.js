import { AnimationGroup, Quaternion } from '@babylonjs/core';

export const RUN_ENTER_SPEED = 1.05;
export const RUN_EXIT_SPEED = 0.95;
export const GAIT_INPUT_SCALE = { walk: 0.4, run: 0.62, auto: 1 };
export const NATIVE_RUN_PLAY_SPEED = 0.21;
const indexOf = node => node.name.match(/(?:^|-)(\d{3})_object_/)?.[1];

export function captureChaoPose(imported) {
  return new Map(imported.visual.getDescendants().flatMap(node => {
    const index = indexOf(node);
    return index && node.rotationQuaternion ? [[index, { node, position: node.position.clone(), quaternion: node.rotationQuaternion.clone() }]] : [];
  }));
}

function cloneNativeMotion(imported, source, targetPose, name, state) {
  const native = source.animationGroups.find(group => group.name === name || group.name.endsWith(`-${name}`));
  if (!native || imported.visual.isDisposed()) return null;
  const group = new AnimationGroup(`${imported.visual.name}-Native${state}`, imported.visual.getScene());
  for (const entry of native.targetedAnimations) {
    const reference = targetPose.get(indexOf(entry.target));
    if (!reference || entry.animation.targetProperty === 'scaling') continue;
    const animation = entry.animation.clone(true);
    if (animation.targetProperty === 'position') {
      // Keep each form's original bind position. Only the common native Chao
      // motion's displacement is transferred, without copying a child rig.
      const offset = reference.position.subtract(entry.target.position);
      animation.setKeys(animation.getKeys().map(key => ({ ...key, value: key.value.add(offset) })), true);
    } else if (animation.targetProperty === 'rotationQuaternion') {
      const sourceRest = entry.target.rotationQuaternion || Quaternion.Identity();
      const offset = reference.quaternion.multiply(sourceRest.conjugate()).normalize();
      animation.setKeys(animation.getKeys().map(key => ({ ...key,
        value: offset.multiply(key.value).normalize(),
        ...(key.inTangent ? { inTangent: offset.multiply(key.inTangent) } : {}),
        ...(key.outTangent ? { outTangent: offset.multiply(key.outTangent) } : {}),
      })), true);
    } else continue;
    group.addTargetedAnimation(animation, reference.node);
  }
  if (!group.targetedAnimations.length) { group.dispose(); return null; }
  group.enableBlending = true; group.blendingSpeed = 0.12;
  imported.entries.animationGroups.push(group);
  return group;
}

export function attachNativeRun(imported, source, targetPose) {
  return imported.run = cloneNativeMotion(imported, source, targetPose, 'alm_run', 'Run119');
}

export function attachNativeLocomotion(imported, source, targetPose) {
  attachNativeRun(imported, source, targetPose);
  imported.jumpUp = cloneNativeMotion(imported, source, targetPose, 'alm_jump_up', 'Jump132');
  imported.jumpDown = cloneNativeMotion(imported, source, targetPose, 'alm_jump_down', 'Fall135');
  return imported;
}

export function createNativeMotionState() {
  return { gait: 'Idle', mode: 'Idle', elapsed: 0, wasGrounded: null, jumpOffset: null };
}

// State132 ascends through frames0–6 at .2;133 holds6–7. State135
// descends through0–4 at .2;136 holds4–5. Landing reuses the genuine
// state137 reverse4–0 recovery. Server jump timing drives selection: the
// trigger/hold mapping is an MMO presentation adaptation, not retail physics.
export function selectNativeMotion(state, dt, { speed = 0, grounded = false,
  jumpOffset, verticalVelocity, fresh = false, mounted = false, swimming = false } = {}) {
  dt = Math.max(0, Math.min(.1, Number.isFinite(dt) ? dt : 0));
  const offset = Number.isFinite(jumpOffset) && jumpOffset >= 0 ? jumpOffset : null;
  const blocked = !fresh || mounted || swimming || offset === null;
  let mode;
  if (blocked) {
    mode = 'Idle'; state.wasGrounded = null; state.jumpOffset = null;
  } else {
    const delta = offset !== null && state.jumpOffset !== null ? offset - state.jumpOffset : 0;
    const descending = Number.isFinite(verticalVelocity) ? verticalVelocity < -.03 : delta < -.0001;
    if (!grounded) mode = descending || state.mode === 'Fall' ? 'Fall' : 'Jump';
    else if (state.wasGrounded === false || state.mode === 'Land' && state.elapsed < 4 / (60 * .2)) mode = 'Land';
    else mode = chooseNativeGait(speed, state.gait);
    state.wasGrounded = !!grounded; state.jumpOffset = offset;
  }
  state.elapsed = mode === state.mode ? state.elapsed + dt : 0;
  state.mode = mode;
  if (mode === 'Idle' || mode === 'Walk' || mode === 'Run') state.gait = mode;
  if (mode === 'Jump') return { mode, key: 'jumpUp', frame: Math.min(6, state.elapsed * 60 * .2), rate: .2, nativeState: state.elapsed < .5 ? 132 : 133 };
  if (mode === 'Fall') return { mode, key: 'jumpDown', frame: Math.min(4, state.elapsed * 60 * .2), rate: .2, nativeState: state.elapsed < 1 / 3 ? 135 : 136 };
  if (mode === 'Land') return { mode, key: 'jumpDown', frame: Math.max(0, 4 - state.elapsed * 60 * .2), rate: -.2, nativeState: 137 };
  return { mode, key: mode.toLowerCase(), rate: nativeGaitRate(mode, speed), nativeState: mode === 'Run' ? 119 : null };
}

export function updateNativeLocomotion(imported, state, dt, snapshot) {
  const motion = selectNativeMotion(state, dt, snapshot);
  const animation = imported[motion.key] || (motion.mode === 'Run' ? imported.walk : null) || imported.idle;
  const sampled = animation === imported[motion.key] && Number.isFinite(motion.frame);
  const changed = imported.current !== animation || imported.nativeMotionMode !== motion.mode;
  if (animation && changed) {
    imported.current?.stop();
    animation.start(true, sampled ? 1 : motion.rate);
    if (sampled) animation.pause();
    imported.current = animation;
  }
  if (animation) {
    if (sampled) animation.goToFrame(motion.frame);
    else animation.speedRatio = nativeGaitRate(animation === imported.run ? 'Run' : animation === imported.walk ? 'Walk' : 'Idle', snapshot.speed);
  }
  imported.nativeMotionMode = motion.mode;
  return { ...motion, available: animation === imported[motion.key] };
}

export function chooseNativeGait(speed, previous = 'Idle') {
  if (!Number.isFinite(speed) || speed < 0.08) return 'Idle';
  return speed >= (previous === 'Run' ? RUN_EXIT_SPEED : RUN_ENTER_SPEED) ? 'Run' : 'Walk';
}

export function nativeGaitRate(gait, speed) {
  if (gait === 'Run') return NATIVE_RUN_PLAY_SPEED * Math.max(.4, Math.min(2, speed / 1.2));
  if (gait === 'Walk') return Math.max(.4, Math.min(2, speed / .8));
  return 1;
}

export function stepInputPace(current, mode, moving, dt, mounted = false) {
  if (!moving) return 0;
  if (mounted) return 1;
  const desired = GAIT_INPUT_SCALE[mode] || GAIT_INPUT_SCALE.auto;
  return Math.min(desired, Math.max(0, Number.isFinite(current) ? current : 0)
    + Math.max(0, Math.min(.2, Number.isFinite(dt) ? dt : 0)) * 1.8);
}

export function createGaitControls(canvas) {
  let mode = 'auto';
  const panel = document.createElement('div'); panel.className = 'gait-controls';
  panel.setAttribute('role', 'group'); panel.setAttribute('aria-label', 'Movement pace');
  const label = document.createElement('span'); label.textContent = 'PACE'; panel.append(label);
  const buttons = Object.keys(GAIT_INPUT_SCALE).map(value => {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = value[0].toUpperCase() + value.slice(1);
    button.dataset.pace = value; button.setAttribute('aria-pressed', String(value === mode));
    button.addEventListener('click', () => { mode = value; for (const row of buttons) row.setAttribute('aria-pressed', String(row === button)); canvas.focus({ preventScroll: true }); });
    panel.append(button); return button;
  });
  const hint = document.createElement('small'); hint.textContent = 'Shift: walk · Alt: run · Auto: full pace'; panel.append(hint);
  canvas.parentElement.append(panel);
  return { get mode() { return mode; }, setPaused(value) { panel.hidden = value; }, dispose() { panel.remove(); } };
}
