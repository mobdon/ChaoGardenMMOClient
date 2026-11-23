import assert from 'node:assert/strict';
import {NullEngine,Scene,TransformNode,LoadAssetContainerAsync} from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import {attachDevActor,updateRosterGait,groundRosterActor} from '../src/dev-actor.js';
import {readFile} from 'node:fs/promises';
const engine=new NullEngine(),scene=new Scene(engine);
try {
 for(const key of ['metalsonic','metalsonicblack','terios']){
  const bytes=await readFile(new URL('../.local/imports/dev-roster/'+key+'.glb',import.meta.url));
  const source=await LoadAssetContainerAsync(new Uint8Array(bytes),scene,{pluginExtension:'.glb'});
  const root=new TransformNode('actor',scene);
  const item={root,proxyMeshes:[],emotion:{setEnabled(){}},nameplate:{position:{}}};
  await attachDevActor(item,key,{load:()=>source,scene,shadows:{addShadowCaster(){}}});
  const model=item.devModel;
  const range=(meshes)=>{
   for(const sk of model.skeletons)sk.prepare(true);
   let min=Infinity,max=-Infinity;
   for(const mesh of meshes){
    const w=mesh.computeWorldMatrix(true).m, p=mesh.getPositionData(true,true);
    for(let i=0;i<p.length;i+=3){
     const y=p[i]*w[1]+p[i+1]*w[5]+p[i+2]*w[9]+w[13]-root.position.y;
     min=Math.min(min,y);max=Math.max(max,y);
    }
   }
   assert.ok(Number.isFinite(min));return {min,max};
  };
  const anchor=model.visual.position.clone();
  for(const [speed,name] of [[0,'Idle'],[.4,'Walk'],[.8,'Speedwalk'],[1.2,'Run'],[1.6,'FastSkate'],[1.99,'SprintSkate'],[0,'Idle']]){
   for(let i=0;i<120;i++){
    root.position.y=Math.sin(i*.03)*.4;root.rotation.y=i*.02;
    updateRosterGait(model,{valid:true,speed,observedAt:i},1/60,i);
    scene._animate(1000/60);groundRosterActor(model,root.position.y,1);
    assert.equal(model.visual.position.x,anchor.x);assert.equal(model.visual.position.z,anchor.z);
    assert.equal(model.visual.rotation.x,0);
    if(i>80 && speed>=1.2 && model.eyeMeshes.length)
      assert.ok(range(model.eyeMeshes).min>=model.standEye-.04,'face falls below standing eye line');

   }
   assert.deepEqual(Object.entries(model.clips).filter(([,g])=>g?.isStarted).map(([k])=>k),[name]);
   console.log(key,name,'lowest vertex relative to ground',range(model.meshes).min.toFixed(3));
  }
  root.dispose();source.dispose();
 }
}finally{engine.dispose();}

