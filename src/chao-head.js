import { Matrix, Quaternion, Vector3 } from '@babylonjs/core';

// Ease native neck rotations toward their neutral frame during fast movement.
// Never write a joint position: the skull and eyes always inherit the neck.
export function createSteadyHead(imported) {
  const actor=imported.visual.parent, nodes=imported.visual.getDescendants();
  const joint=id=>nodes.find(n=>n.name.includes(id+'_object_'));
  const head=joint('016'), mesh=head?.getChildMeshes().find(m=>m.getTotalVertices()>0);
  const center=mesh?.getBoundingInfo().boundingBox.center.clone();
  const chain=[joint('014'),joint('015'),head];
  const ready=chain.every(n=>n?.rotationQuaternion) && head.parent===chain[1] && chain[1].parent===chain[0] && !!actor;
  const rotation=(n,ref)=>{
    const q=Quaternion.Identity();
    n.computeWorldMatrix(true).multiply(Matrix.Invert(ref.computeWorldMatrix(true))).decompose(new Vector3(),q,new Vector3());
    return q.normalize();
  };
  // Remove the shared glTF reflection and uniform Chao scale before
  // comparing orientations. A reflected actor-space matrix has no unique
  // rotation and can turn a small torso sway into a half-turn.
  const rigFrame=joint('001')?.parent;
  const neutral=ready && rigFrame ? chain.map(n=>rotation(n,rigFrame)) : [];
  let originals=null,blend=0,stroke=null,mode='natural';
  function restoreBase(){
    if(originals)chain.forEach((n,i)=>{if(!n.isDisposed())n.rotationQuaternion.copyFrom(originals[i]);});
    originals=null;
  }
  function update(dt,{gait,skateBlend=0,headStroke=null,mounted=false}){
    if(!neutral.length || mounted){restoreBase();blend=0;mode=mounted?'mounted':'unavailable';return;}
    const target=gait==='Run'||skateBlend>0 ? 1 : 0;
    blend+=(target-blend)*(1-Math.exp(-Math.min(.1,Math.max(0,dt))/.075));
    if(Math.abs(blend-target)<.001)blend=target;
    stroke=headStroke && skateBlend>0 ? Quaternion.Slerp(Quaternion.Identity(),headStroke,skateBlend) : null;
    mode=skateBlend>0?'skate':target?'run':blend?'releasing':'natural';
  }
  function apply(){
    if(!neutral.length || !blend || head.isDisposed())return;
    restoreBase();
    originals=chain.map(n=>n.rotationQuaternion.clone());
    for(let i=0;i<chain.length;i++){
      const n=chain[i], desired=stroke ? stroke.multiply(neutral[i]) : neutral[i];
      const local=rotation(n.parent,rigFrame).conjugate().multiply(desired).normalize();
      n.rotationQuaternion.copyFrom(Quaternion.Slerp(originals[i],local,.85*blend).normalize());
    }
  }
  return {update,apply,restoreBase,
    renderedCenter:()=>mesh && !mesh.isDisposed() ? Vector3.TransformCoordinates(center,mesh.worldMatrixFromCache) : null,
    get diagnostics(){
      const available=ready && !head.isDisposed() && !actor.isDisposed();
      const position=available ? head.computeWorldMatrix(true).multiply(Matrix.Invert(actor.computeWorldMatrix(true))).getTranslation().asArray() : null;
      return {available,mode,position,active:blend>0,blend};
    }
  };
}
