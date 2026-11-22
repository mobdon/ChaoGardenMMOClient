import { Matrix, Quaternion, Vector3 } from '@babylonjs/core';

export const HEAD_LOCK_SECONDS = .18;
// World-space distance, applied along the actor's local up axis.
export const SKATE_HEAD_BOB_AMPLITUDE = .002;
const unit = value => Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0;
const smoothstep = value => value * value * (3 - 2 * value);

// Presentation-only compensation for inherited torso sway and visual foot
// grounding. Capture the visible native head frame relative to the authoritative
// actor wrapper, not the animated torso or the ground-adjusted visual root.
export function createSteadyHead(imported) {
  const actor = imported.visual.parent;
  // 014/015 are animated neck pivots. 016 owns the skull and face subtree;
  // anchoring only the neck still leaves native head rotation underneath it.
  const head = imported.visual.getDescendants().find(node => /(?:^|-)016_object_/.test(node.name));
  const drawnHead = head?.getChildMeshes().find(mesh => mesh.getTotalVertices() > 0);
  const drawnCenter = drawnHead?.getBoundingInfo().boundingBox.center.clone();
  const available = !!actor && !!head?.rotationQuaternion && !!head.parent;
  let anchor = null, base = null, blend = 0, blendProgress = 0, disposed = false;
  let mode = available ? 'natural' : 'unavailable', phase = 0, bobAmplitude = 0, bobOffsetY = 0, bobTargetY = 0;
  if (available) {
    actor.computeWorldMatrix(true); head.computeWorldMatrix(true);
    anchor = head.getWorldMatrix().multiply(Matrix.Invert(actor.getWorldMatrix()));
  }

  function restoreBase() {
    if (base && !head.isDisposed()) {
      head.position.copyFrom(base.position);
      head.rotationQuaternion.copyFrom(base.rotation);
    }
    base = null;
  }

  function update(dt, { mounted = false, gait = 'Idle', skateBlend = 0, skatePhase = 0 } = {}) {
    if (disposed || !available || mounted) {
      restoreBase(); blendProgress = 0; blend = 0; bobAmplitude = 0; bobOffsetY = 0; bobTargetY = 0;
      mode = disposed ? 'disposed' : !available ? 'unavailable' : 'mounted';
      return;
    }
    // Native idle/walk/run keep their entire head motion (the run's head bob is
    // native). Only a skate locks the head; its small stroke uses its source phase.
    const skate = unit(skateBlend);
    const locking = skate > 0;
    dt = Number.isFinite(dt) ? Math.max(0, Math.min(.1, dt)) : 0;
    const target = locking ? 1 : 0;
    blendProgress += Math.sign(target - blendProgress)
      * Math.min(Math.abs(target - blendProgress), dt / HEAD_LOCK_SECONDS);
    blend = smoothstep(blendProgress);
    phase = Number.isFinite(skatePhase) ? (skatePhase % 1 + 1) % 1 : 0;
    // The final output is blended once below, so its observed amplitude is
    // exactly this bounded amount even during the lock transition.
    bobAmplitude = SKATE_HEAD_BOB_AMPLITUDE * skate * blend;
    bobTargetY = Math.sin(phase * Math.PI * 2) * SKATE_HEAD_BOB_AMPLITUDE * skate;
    bobOffsetY = bobTargetY * blend;
    mode = skate > 0 ? 'skate' : locking ? 'run' : blend > 0 ? 'blending-out' : 'natural';
  }

  function apply() {
    if (disposed || !blend || !available || head.isDisposed() || actor.isDisposed()) return;
    actor.computeWorldMatrix(true); head.parent.computeWorldMatrix(true);
    const parent = head.parent.getWorldMatrix();
    if (!Number.isFinite(parent.determinant()) || Math.abs(parent.determinant()) < 1e-12) return;
    const actorUpLength = Vector3.TransformNormal(Vector3.Up(), actor.getWorldMatrix()).length();
    if (!Number.isFinite(actorUpLength) || actorUpLength < 1e-8) return;
    const target = anchor.clone();
    // Apply the full target offset before interpolation; blend then supplies
    // the smooth transition weight already reflected in diagnostics. Convert
    // its world amplitude to actor units so scaled Chao keep the same bound.
    target.setTranslation(anchor.getTranslation().add(new Vector3(0, bobTargetY / actorUpLength, 0)));
    const local = target.multiply(actor.getWorldMatrix()).multiply(Matrix.Invert(parent));
    const position = new Vector3(), rotation = Quaternion.Identity();
    if (!local.decompose(new Vector3(), rotation, position)
      || !position.asArray().every(Number.isFinite) || !rotation.asArray().every(Number.isFinite)) return;
    base = { position: head.position.clone(), rotation: head.rotationQuaternion.clone() };
    head.position.copyFrom(Vector3.Lerp(base.position, position, blend));
    head.rotationQuaternion.copyFrom(Quaternion.Slerp(base.rotation, rotation.normalize(), blend).normalize());
    head.computeWorldMatrix(true);
  }

  return { update, apply, restoreBase,
    // Inspect the matrix actually selected by the last render. Recomputing a
    // descendant here could hide a rendering-order bug from screen-space QA.
    renderedCenter() {
      return drawnHead && !drawnHead.isDisposed()
        ? Vector3.TransformCoordinates(drawnCenter, drawnHead.worldMatrixFromCache) : null;
    },
    get diagnostics() {
      let position = null;
      if (available && !head.isDisposed() && !actor.isDisposed()) {
        head.computeWorldMatrix(true); actor.computeWorldMatrix(true);
        position = head.getWorldMatrix().multiply(Matrix.Invert(actor.getWorldMatrix())).getTranslation().asArray();
      }
      return { available, blend, active: blend > 0, mode, bobAmplitude, bobOffsetY, phase, position };
    },
    dispose() { restoreBase(); disposed = true; blendProgress = 0; blend = 0; bobAmplitude = 0; bobOffsetY = 0; bobTargetY = 0; mode = 'disposed'; },
  };
}
