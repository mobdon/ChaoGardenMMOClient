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

export function attachNativeRun(imported, source, targetPose) {
  const native = source.animationGroups.find(group => /(?:^|[-_])alm_run$/i.test(group.name));
  if (!native || imported.visual.isDisposed()) return null;
  const group = new AnimationGroup(`${imported.visual.name}-NativeRun119`, imported.visual.getScene());
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
  imported.entries.animationGroups.push(group); imported.run = group;
  return group;
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
