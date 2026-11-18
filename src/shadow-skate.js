import { Matrix, Quaternion, Vector3 } from '@babylonjs/core';

// Browser world units/second. Current Rust dry-ground speed is
// 1.4 + runLevel * .006 (maximum1.994). These presentation thresholds never
// alter movement, stats, footwear size, or the original Chao hierarchy.
export const SKATE_ENTER_SPEED = 1.55;
export const SKATE_EXIT_SPEED = 1.30;
export const SKATE_REFERENCE_SPEED = 1.8;
export const SKATE_BLEND_SECONDS = 0.18;
// Angular limits are a Chao retargeting adaptation. Source poses can rotate a
// Shadow limb163° from bind pose; copying that range onto the much shorter
// Chao limbs would fold the shoes/hands through the body.
export const SKATE_ROTATION_LIMITS = {
  body: 30, head: 35, leftUpperArm: 75, rightUpperArm: 75,
  leftHand: 75, rightHand: 75, leftFoot: 50, rightFoot: 50,
};

// Selected source world-space orientations are adapted to existing Chao
// rotation channels. Shadow's limb count and proportions are never copied.
// Source left/right map by the verified aligned model positions, not names.
const TARGETS = [
  ['001', 'body'], ['014', 'head'],
  ['003', 'leftUpperArm'], ['004', 'leftHand'],
  ['010', 'rightUpperArm'], ['011', 'rightHand'],
  ['005', 'leftFoot'], ['012', 'rightFoot'],
];

const frameDelta = dt => Math.max(0, Math.min(0.1, Number.isFinite(dt) ? dt : 0));

let sharedMotion;
async function loadMotion() {
  sharedMotion ??= fetch('/api/assets/shadow-skate-motion.json', { cache: 'no-store' })
    .then(response => response.ok ? response.json() : null)
    .catch(() => null);
  return sharedMotion;
}

const finiteQuaternion = value => Array.isArray(value) && value.length === 4 && value.every(Number.isFinite)
  && Math.abs(Math.hypot(...value) - 1) < .01;
const quaternion = value => Quaternion.FromArray(value).normalize();

function supportedMotion(motion) {
  return motion?.version === 1 && motion.basis === 'right-handed glTF Y-up model space'
    && finiteQuaternion(motion.alignmentQuaternion)
    && Array.isArray(motion.bones) && Array.isArray(motion.clips)
    && TARGETS.every(([, semantic]) => {
      const bones = motion.bones.filter(bone => bone?.semantic === semantic);
      return bones.length === 1 && finiteQuaternion(bones[0].restWorld);
    })
    && [172, 173, 174].every(id => {
      const clips = motion.clips.filter(row => row?.nativeId === id);
      const clip = clips[0];
      if (clips.length !== 1) return false;
      return clip && Number.isFinite(clip.durationSeconds) && clip.durationSeconds > 0
        && Array.isArray(clip.frames) && clip.frames.length > 1
        && clip.frames[0]?.timeSeconds === 0
        && clip.frames.every((frame, index, frames) => Number.isFinite(frame?.timeSeconds)
          && frame.timeSeconds >= 0 && frame.timeSeconds <= clip.durationSeconds
          && (!index || frame.timeSeconds > frames[index - 1].timeSeconds)
          && TARGETS.every(([, semantic]) => finiteQuaternion(frame.rotations?.[semantic])));
    });
}

export function observeSkateMotion(previous, player, serverTime, observedAt) {
  const x = player.x, z = player.z;
  const jumpOffset = Number.isFinite(player.jumpOffset) && player.jumpOffset >= 0 ? player.jumpOffset : null;
  const elapsed = (serverTime - previous?.serverTime) / 1000;
  const distance = Math.hypot(x - previous?.x, z - previous?.z);
  const valid = Number.isFinite(x) && Number.isFinite(z) && Number.isFinite(serverTime)
    && elapsed > 0 && elapsed <= 1 && player.zone === previous?.zone
    && distance <= elapsed * 3 + 0.025;
  return { x, z, zone: player.zone, serverTime, observedAt,
    speed: valid ? distance / elapsed : 0, valid,
    jumpOffset,
    verticalVelocity: valid && jumpOffset !== null && Number.isFinite(previous?.jumpOffset)
      ? (jumpOffset - previous.jumpOffset) / elapsed : 0,
    grounded: jumpOffset !== null && jumpOffset <= 0.001,
    swimming: player.zone === 'garden' && (x + 8) ** 2 + z ** 2 < 25 };
}

export class SkateCadence {
  constructor() {
    this.active = false; this.blend = 0; this.phase = 0;
    this.speed = 0; this.rate = 0; this.state = 'idle';
  }

  update(dt, { speed = 0, grounded = false, footwear = false, mounted = false,
    swimming = false, fresh = false, available = false } = {}) {
    dt = frameDelta(dt);
    this.speed = Number.isFinite(speed) ? Math.max(0, speed) : 0;
    const blocked = !grounded || !footwear || mounted || swimming || !fresh || !available;
    if (blocked) { this.active = false; this.blend = 0; }
    else if (this.active) this.active = this.speed >= SKATE_EXIT_SPEED;
    else this.active = this.speed >= SKATE_ENTER_SPEED;
    const target = this.active ? 1 : 0;
    this.blend += Math.sign(target - this.blend) * Math.min(Math.abs(target - this.blend), dt / SKATE_BLEND_SECONDS);
    this.rate = this.speed > 0.05 ? Math.min(2, Math.max(0.35, this.speed / SKATE_REFERENCE_SPEED)) : 0;
    this.state = mounted ? 'mounted' : !grounded ? 'airborne' : swimming ? 'swimming'
      : !fresh ? 'stale' : !footwear ? 'normal' : !available ? 'unavailable'
        : this.active ? this.blend < 1 ? 'blending-in' : 'skating'
          : this.blend > 0 ? 'blending-out' : this.speed > 0.08 ? 'walking' : 'idle';
    return this;
  }
}

function relativeRotation(node, reference) {
  node.computeWorldMatrix(true); reference.computeWorldMatrix(true);
  const local = node.getWorldMatrix().multiply(Matrix.Invert(reference.getWorldMatrix()));
  const rotation = Quaternion.Identity();
  local.decompose(new Vector3(), rotation, new Vector3());
  return rotation.normalize();
}

function nodeDepth(node) {
  let depth = 0;
  while (node.parent) { depth++; node = node.parent; }
  return depth;
}

function sampleClip(clip, semantic, phase) {
  const seconds = phase * clip.durationSeconds;
  const frames = clip.frames;
  if (seconds >= frames.at(-1).timeSeconds) {
    const last = frames.at(-1), first = frames[0];
    const a = last.rotations?.[semantic], b = first.rotations?.[semantic];
    if (!finiteQuaternion(a) || !finiteQuaternion(b)) return null;
    const width = clip.durationSeconds + first.timeSeconds - last.timeSeconds;
    const mix = width > 0 ? (seconds - last.timeSeconds) / width : 0;
    return Quaternion.Slerp(quaternion(a), quaternion(b), Math.max(0, Math.min(1, mix))).normalize();
  }
  let lower = 0, upper = frames.length - 1;
  while (lower + 1 < upper) {
    const middle = (lower + upper) >> 1;
    if (frames[middle].timeSeconds <= seconds) lower = middle; else upper = middle;
  }
  const a = frames[lower], b = frames[upper];
  const qa = a.rotations?.[semantic], qb = b.rotations?.[semantic];
  if (!finiteQuaternion(qa) || !finiteQuaternion(qb)) return null;
  const width = b.timeSeconds - a.timeSeconds;
  const mix = width > 0 ? Math.max(0, Math.min(1, (seconds - a.timeSeconds) / width)) : 0;
  return Quaternion.Slerp(quaternion(qa), quaternion(qb), mix).normalize();
}

export function createShadowSkate(imported, { motion = null, fetchMotion = loadMotion } = {}) {
  const nodes = imported.visual.getDescendants();
  const find = index => nodes.find(node => new RegExp(`(?:^|-)${index}_object_`).test(node.name));
  const root = find('000')?.parent;
  const cadence = new SkateCadence();
  const targets = root ? TARGETS.map(([index, semantic]) => {
    const node = find(index);
    if (!node?.rotationQuaternion) return null;
    return { node, semantic, restWorld: relativeRotation(node, root), base: null };
  }).filter(Boolean).sort((a, b) => nodeDepth(a.node) - nodeDepth(b.node)) : [];
  let data = supportedMotion(motion) ? motion : null;
  let disposed = false, clipName = '', deltaTime = 0;
  if (!data && targets.length) void fetchMotion().then(value => { if (!disposed && supportedMotion(value)) data = value; });

  function restoreBase() {
    for (const target of targets) if (target.base && !target.node.isDisposed()) {
      target.node.rotationQuaternion.copyFrom(target.base);
      target.base = null;
    }
  }

  function update(dt, state) {
    deltaTime = frameDelta(dt);
    cadence.update(deltaTime, { ...state, available: !!data && targets.length === TARGETS.length });
    if (!cadence.blend) clipName = '';
  }

  function apply() {
    if (!data || !root || root.isDisposed() || !cadence.blend) return;
    const ids = cadence.speed < 1.8 ? [172, 173] : [173, 174];
    const clips = ids.map(id => data.clips.find(clip => clip.nativeId === id));
    const mix = Math.max(0, Math.min(1, cadence.speed < 1.8
      ? (cadence.speed - SKATE_ENTER_SPEED) / (1.8 - SKATE_ENTER_SPEED)
      : (cadence.speed - 1.8) / (1.994 - 1.8)));
    const duration = clips[0].durationSeconds + (clips[1].durationSeconds - clips[0].durationSeconds) * mix;
    cadence.phase = (cadence.phase + deltaTime * cadence.rate / duration) % 1;
    clipName = `${clips[mix < 0.5 ? 0 : 1].nativeId}`;
    const alignment = quaternion(data.alignmentQuaternion), inverseAlignment = alignment.conjugate();
    for (const target of targets) {
      const sourceBone = data.bones.find(bone => bone.semantic === target.semantic);
      if (!finiteQuaternion(sourceBone?.restWorld)) continue;
      const a = sampleClip(clips[0], target.semantic, cadence.phase);
      const b = sampleClip(clips[1], target.semantic, cadence.phase);
      if (!a || !b) continue;
      const source = Quaternion.Slerp(a, b, mix).normalize();
      let delta = alignment.multiply(source).multiply(quaternion(sourceBone.restWorld).conjugate()).multiply(inverseAlignment).normalize();
      const angle = 2 * Math.acos(Math.min(1, Math.abs(delta.w)));
      const limit = (SKATE_ROTATION_LIMITS[target.semantic] || 50) * Math.PI / 180;
      if (angle > limit) delta = Quaternion.Slerp(Quaternion.Identity(), delta, limit / angle).normalize();
      const desiredWorld = delta.multiply(target.restWorld).normalize();
      const parent = relativeRotation(target.node.parent, root);
      const desiredLocal = parent.conjugate().multiply(desiredWorld).normalize();
      target.base = target.node.rotationQuaternion.clone();
      target.node.rotationQuaternion.copyFrom(Quaternion.Slerp(target.base, desiredLocal, cadence.blend).normalize());
      target.node.computeWorldMatrix(true);
    }
  }

  return { update, apply, restoreBase,
    get diagnostics() { return { active: cadence.blend > 0, state: cadence.state,
      speed: cadence.speed, rate: cadence.rate, blend: cadence.blend, clip: clipName }; },
    dispose() { disposed = true; restoreBase(); },
  };
}
