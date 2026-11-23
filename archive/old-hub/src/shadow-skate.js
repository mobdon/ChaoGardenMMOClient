import { Matrix, Quaternion, Vector3 } from '@babylonjs/core';
import { smoothObservedSpeed } from './chao-locomotion.js';

// Browser world units/second. Current Rust dry-ground speed is
// 1.4 + runLevel * .006 (maximum1.994). These presentation thresholds never
// alter movement, stats, footwear size, or the original Chao hierarchy.
export const SKATE_ENTER_SPEED = 1.55;
export const SKATE_EXIT_SPEED = 1.30;
export const SKATE_REFERENCE_SPEED = 1.8;
export const SKATE_BLEND_SECONDS = 0.42;
export const SKATE_BLEND_OUT_SECONDS = 0.28;
export const SKATE_SPEED_BLEND_SECONDS = 0.32;
// Cosmetic carry across short descending air gaps reported by the authority
// on sloping terrain. This never changes ground contact, height or movement.
export const SKATE_COAST_HEIGHT = .015;
export const SKATE_COAST_SECONDS = .4;
// Verified AnimationInfo states8/9/10, not direct resource-state172/173/174.
// Mapping this MMO's observed speed bands onto these native rates is a
// presentation adaptation; the retail movement-dependent timing is untraced.
export const SKATE_SOURCE_RATES = Object.freeze({ 172: .12, 173: .15, 174: .20 });
const frameDelta = dt => Math.max(0, Math.min(.1, Number.isFinite(dt) ? dt : 0));
const smoothstep = value => value * value * (3 - 2 * value);

function sourceBlend(speed) {
  return speed < 1.8
    ? { ids: [172, 173], mix: Math.max(0, Math.min(1, (speed - SKATE_ENTER_SPEED) / (1.8 - SKATE_ENTER_SPEED))) }
    : { ids: [173, 174], mix: Math.max(0, Math.min(1, (speed - 1.8) / (1.994 - 1.8))) };
}

function nativeSourceRates(motion) {
  return Object.fromEntries([172, 173, 174].map((id, index) => {
    const speed = motion?.clips.find(clip => clip.nativeId === id)?.nativeStates
      ?.find(state => state.index === 8 + index)?.animationSpeed;
    return [id, Number.isFinite(speed) && speed > 0 && speed <= .5 ? speed : SKATE_SOURCE_RATES[id]];
  }));
}
// Angular limits are a Chao retargeting adaptation. Source poses can rotate a
// Shadow limb163° from bind pose; copying that range onto the much shorter
// Chao limbs would fold the shoes/hands through the body.
export const SKATE_ROTATION_LIMITS = {
  body: 24, head: 18, leftUpperArm: 75, rightUpperArm: 75,
  leftHand: 75, rightHand: 75, leftFoot: 50, rightFoot: 50,
};
// A Chao's large head exaggerates Shadow's native head sway. Retain35% of
// that original rotation before the angular cap; keep native positions and
// the full arm/foot strokes. This is anatomy adaptation, not a new motion.
export const SKATE_ROTATION_WEIGHTS = Object.freeze({ head: .35 });

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
    && distance <= elapsed * (player.devActor ? 60 : 3) + 0.025;
  return { x, y: player.y, groundY: player.groundY, z, zone: player.zone, serverTime, observedAt,
    speed: valid ? distance / elapsed : 0, valid,
    airGap: Number.isFinite(player.jumpOffset) && player.jumpOffset >= 0 ? player.jumpOffset : Infinity,
    descending: valid && Number.isFinite(player.y) && Number.isFinite(previous?.y) && player.y <= previous.y + .0005,
    descendingGround: valid && Number.isFinite(player.groundY) && Number.isFinite(previous?.groundY) && player.groundY < previous.groundY - .000001,
    jumpOffset,
    verticalVelocity: valid && jumpOffset !== null && Number.isFinite(previous?.jumpOffset)
      ? (jumpOffset - previous.jumpOffset) / elapsed : 0,
    grounded: jumpOffset !== null && jumpOffset <= 0.001,
    swimming: player.zone === 'garden' && (x + 8) ** 2 + z ** 2 < 25 };
}

export class SkateCadence {
  constructor() {
    this.active = false; this.blend = 0; this.phase = 0;
    this.speed = 0; this.visualSpeed = 0; this.rate = 0; this.state = 'idle';
    this.blendProgress = 0;
    this.coastSeconds = 0; this.coastSample = null; this.coasting = false;
  }

  update(dt, { speed = 0, grounded = false, footwear = false, mounted = false,
    swimming = false, fresh = false, available = false, airGap = Infinity, descending = false, descendingGround = false, serverTime = null,
    sourceRates = SKATE_SOURCE_RATES } = {}) {
    dt = frameDelta(dt);
    this.speed = Number.isFinite(speed) ? Math.max(0, Math.min(3, speed)) : 0;
    const blocked = !footwear || mounted || swimming || !fresh || !available;
    const wasCoasting = this.coasting;
    this.coasting = false;
    if (blocked) {
      this.active = false; this.blendProgress = 0;
      this.visualSpeed = 0; this.phase = 0;
      this.coastSeconds = 0; this.coastSample = null;
    }
    else if (!grounded) {
      const closeDescent = descending && descendingGround && Number.isFinite(airGap)
        && airGap >= 0 && airGap <= SKATE_COAST_HEIGHT;
      // Each new authoritative downhill observation can renew the short pose
      // carry. A continuous slope may publish tiny gaps for seconds, whereas
      // a stopped stream, takeoff or cliff must not keep it alive.
      if (closeDescent && Number.isFinite(serverTime) && (this.coastSample === null || serverTime > this.coastSample)) {
        this.coastSeconds = 0; this.coastSample = serverTime;
      }
      this.coastSeconds += dt;
      this.coasting = this.active && (wasCoasting || this.blend === 1) && closeDescent
        && this.coastSeconds <= SKATE_COAST_SECONDS
        && this.speed >= SKATE_EXIT_SPEED;
      this.active = this.coasting;
    }
    else {
      this.coastSeconds = 0; this.coastSample = null;
      if (this.active) this.active = this.speed >= SKATE_EXIT_SPEED;
      else this.active = this.speed >= SKATE_ENTER_SPEED;
    }
    const target = this.active ? 1 : 0;
    const duration = this.active ? SKATE_BLEND_SECONDS : SKATE_BLEND_OUT_SECONDS;
    this.blendProgress += Math.sign(target - this.blendProgress)
      * Math.min(Math.abs(target - this.blendProgress), dt / duration);
    this.blend = smoothstep(this.blendProgress);
    if (this.blend) this.visualSpeed = smoothObservedSpeed(this.visualSpeed, this.speed, dt, SKATE_SPEED_BLEND_SECONDS);
    else { this.visualSpeed = 0; this.phase = 0; }
    const { ids, mix } = sourceBlend(this.visualSpeed);
    const nativeRate = sourceRates[ids[0]] + (sourceRates[ids[1]] - sourceRates[ids[0]]) * mix;
    this.rate = this.blend ? nativeRate * Math.min(1, this.visualSpeed / SKATE_ENTER_SPEED) : 0;
    this.state = mounted ? 'mounted' : this.coasting ? 'coasting' : !grounded ? 'airborne' : swimming ? 'swimming'
      : !fresh ? 'stale' : !footwear ? 'normal' : !available ? 'unavailable'
        : this.active ? this.blend < 1 ? 'blending-in' : 'skating'
          : this.blend > 0 ? 'blending-out' : this.speed > 0.08 ? 'walking' : 'idle';
    return this;
  }
}

// Selected source world-space orientations are adapted to existing Chao
// rotation channels. Shadow's limb count and proportions are never copied.
// Source left/right map by the verified aligned model positions, not names.
const TARGETS = [
  ['001', 'body'], ['014', 'head'],
  ['003', 'leftUpperArm'], ['004', 'leftHand'],
  ['010', 'rightUpperArm'], ['011', 'rightHand'],
  ['006', 'leftFoot'], ['013', 'rightFoot'],
];

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
  let sourceRates = data ? nativeSourceRates(data) : SKATE_SOURCE_RATES;
  // The lower toe of the original skate pair defines clearance. Normalize
  // against standing Shadow, not absolute SA2 bone positions on the Chao.
  let clearanceRatio=0;
  const meanLifts=new WeakMap();
  function sourceLift(clip,stand){
    if(!meanLifts.has(clip)){
      const toeY=f=>Math.min(f.positions?.leftToe?.[1],f.positions?.rightToe?.[1]);
      const base=toeY(stand),span=stand.positions?.head?.[1]-base;
      const sum=clip.frames.reduce((total,f)=>total+Math.max(0,toeY(f)-base),0);
      meanLifts.set(clip,span>0 && Number.isFinite(sum) ? sum/(span*clip.frames.length) : 0);
    }
    return meanLifts.get(clip);
  }
  let strokeRotation = null;
  let disposed = false, clipName = '', deltaTime = 0;
  if (!data && targets.length) void fetchMotion().then(value => {
    if (!disposed && supportedMotion(value)) { data = value; sourceRates = nativeSourceRates(data); }
  });

  function restoreBase() {
    for (const target of targets) if (target.base && !target.node.isDisposed()) {
      target.node.rotationQuaternion.copyFrom(target.base);
      target.base = null;
    }
  }

  function update(dt, state) {
    deltaTime = frameDelta(dt);
    cadence.update(deltaTime, { ...state, available: !!data && targets.length === TARGETS.length,
      sourceRates });
    if (!cadence.blend) {clipName = '';clearanceRatio=0;}
  }

  function apply() {
    if (!data || !root || root.isDisposed() || !cadence.blend) return;
    const { ids, mix } = sourceBlend(cadence.visualSpeed);
    const clips = ids.map(id => data.clips.find(clip => clip.nativeId === id));
    const duration = clips[0].durationSeconds + (clips[1].durationSeconds - clips[0].durationSeconds) * mix;
    cadence.phase = (cadence.phase + deltaTime * cadence.rate / duration) % 1;
    // Run119 supplies the Chao leg pivots underneath the retargeted shoes.
    // Keep those pivots on the same stride as the shoe rotations.
    const run=imported.run;
    if(run?.isStarted && run.to>run.from){
      const frame=run.getCurrentFrame(), phase=(frame-run.from)/(run.to-run.from);
      const error=((cadence.phase-phase+1.5)%1)-.5;
      const locked=(phase+error*cadence.blend+1)%1;
      run.goToFrame(run.from+locked*(run.to-run.from));
    }
    const stand=data.clips.find(c=>c.nativeId===169)?.frames[0];
    if(stand)clearanceRatio=cadence.blend*((1-mix)*sourceLift(clips[0],stand)+mix*sourceLift(clips[1],stand));
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
      const weight = SKATE_ROTATION_WEIGHTS[target.semantic] ?? 1;
      if (weight < 1) delta = Quaternion.Slerp(Quaternion.Identity(), delta, weight).normalize();
      const angle = 2 * Math.acos(Math.min(1, Math.abs(delta.w)));
      const limit = (SKATE_ROTATION_LIMITS[target.semantic] || 50) * Math.PI / 180;
      if (angle > limit) delta = Quaternion.Slerp(Quaternion.Identity(), delta, limit / angle).normalize();
      if(target.semantic==='head')strokeRotation=delta;
      const desiredWorld = delta.multiply(target.restWorld).normalize();
      const parent = relativeRotation(target.node.parent, root);
      const desiredLocal = parent.conjugate().multiply(desiredWorld).normalize();
      target.base = target.node.rotationQuaternion.clone();
      target.node.rotationQuaternion.copyFrom(Quaternion.Slerp(target.base, desiredLocal, cadence.blend).normalize());
      target.node.computeWorldMatrix(true);
    }
  }

  return { update, apply, restoreBase, get clearanceRatio(){return clearanceRatio;}, get headStroke(){return cadence.blend ? strokeRotation : null;},
    get diagnostics() { return { active: cadence.blend > 0, state: cadence.state,
      coasting: cadence.coasting,
      speed: cadence.speed, visualSpeed: cadence.visualSpeed, phase: cadence.phase,
      rate: cadence.rate, blend: cadence.blend, clip: clipName }; },
    dispose() { disposed = true; restoreBase(); },
  };
}
