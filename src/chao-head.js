import { Matrix, Quaternion, Vector3 } from '@babylonjs/core';

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
  let anchor = null, base = null, blend = available ? 1 : 0, disposed = false;
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

  function update(_dt, { mounted = false } = {}) {
    // Keep it fixed through idle, walk, run, skate, stops and jumps. A gait
    // threshold or fade reintroduces bob at every speed change. Seated pilots
    // keep their separate cockpit pose instead of this standing anchor.
    blend = !disposed && available && !mounted ? 1 : 0;
  }

  function apply() {
    if (disposed || !blend || !available || head.isDisposed() || actor.isDisposed()) return;
    actor.computeWorldMatrix(true); head.parent.computeWorldMatrix(true);
    const parent = head.parent.getWorldMatrix();
    if (!Number.isFinite(parent.determinant()) || Math.abs(parent.determinant()) < 1e-12) return;
    const local = anchor.multiply(actor.getWorldMatrix()).multiply(Matrix.Invert(parent));
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
      return { available, blend, active: blend > 0, position };
    },
    dispose() { restoreBase(); disposed = true; blend = 0; },
  };
}
