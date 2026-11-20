import test from 'node:test';
import assert from 'node:assert/strict';
import { Matrix, NullEngine, Quaternion, Scene, TransformNode, Vector3 } from '@babylonjs/core';
import { createSteadyHead } from '../src/chao-head.js';

function rig() {
  const engine = new NullEngine(), scene = new Scene(engine);
  const make = (name, parent, xyz) => {
    const node = new TransformNode(name, scene); node.parent = parent;
    node.position.set(...xyz); node.rotationQuaternion = Quaternion.Identity(); return node;
  };
  const actor = make('actor', null, [3, 2, -4]);
  const visual = make('visual', actor, [0, .015, 0]); visual.scaling.setAll(.1);
  const importedRoot = make('gltf', visual, [0, 0, 0]); importedRoot.scaling.z = -1;
  const body = make('001_object_body', importedRoot, [0, 1, 0]);
  const neck = make('014_object_neck', body, [0, .7, 0]);
  const pivot = make('015_object_pivot', neck, [0, .1, 0]);
  const head = make('016_object_head', pivot, [0, .2, .1]);
  const eye = make('018_object_eye', head, [.1, .1, .2]);
  const foot = make('006_object_foot', body, [.3, -1, .2]);
  const relative = node => {
    node.computeWorldMatrix(true); actor.computeWorldMatrix(true);
    return node.getWorldMatrix().multiply(Matrix.Invert(actor.getWorldMatrix()));
  };
  const local = node => ({ p: node.position.asArray(), q: node.rotationQuaternion.asArray(), s: node.scaling.asArray(), parent: node.parent });
  const control = createSteadyHead({ visual });
  return { engine, actor, visual, body, neck, pivot, head, eye, foot, relative, local, control };
}
const closeMatrix = (actual, expected, message) => assert.ok(actual.asArray().every((value, index) =>
  Math.abs(value - expected.asArray()[index]) < 1e-6), message);
test('skull and eyes follow the native torso and neck through actor lift and turns',()=>{
  const r=rig();
  try{
    const skull=r.local(r.head),parent=r.head.parent;
    for(let frame=0;frame<90;frame++){
      r.body.position.y=1+frame*.01;r.body.rotationQuaternion.copyFrom(Quaternion.RotationYawPitchRoll(0,frame*.004,0));
      r.visual.position.y=frame*.002;
      const nativeRotation=r.head.rotationQuaternion.clone();
      const untreated=r.relative(r.head);r.control.update(1/60,{gait:'Run'});r.control.apply();
      assert.deepEqual(r.head.position.asArray(),skull.p);
      const expected=r.relative(r.head).getTranslation();
      const diagnostics=r.control.diagnostics;
      assert.equal(diagnostics.mode,'run');
      assert.ok(Vector3.Distance(Vector3.FromArray(diagnostics.position),expected)<1e-6);
      if(frame===89){
        const q=Quaternion.Identity();r.head.computeWorldMatrix(true).multiply(Matrix.Invert(r.body.parent.computeWorldMatrix(true))).decompose(new Vector3(),q,new Vector3());
        assert.ok(Math.abs(q.x)<.06,'run damps inherited torso pitch');
      }
      r.control.restoreBase();closeMatrix(r.relative(r.head),untreated);assert.deepEqual(r.local(r.head),skull);assert.equal(r.head.parent,parent);
    }
  }finally{r.engine.dispose();}
});
