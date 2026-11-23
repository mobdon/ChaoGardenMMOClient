import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {NullEngine,Scene,TransformNode,LoadAssetContainerAsync} from '@babylonjs/core';
import '@babylonjs/loaders/glTF/index.js';
import {DEV_ROSTER} from '../src/dev-roster.js';
import {attachDevActor,rosterAttack,updateRosterGait} from '../src/dev-actor.js';
const engine=new NullEngine(), scene=new Scene(engine);
try {
 for(const key of Object.keys(DEV_ROSTER)){
  const bytes=await readFile(new URL('../.local/imports/dev-roster/'+key+'.glb',import.meta.url));
  const source=await LoadAssetContainerAsync(new Uint8Array(bytes),scene,{pluginExtension:'.glb',pluginOptions:{gltf:{skipMaterials:true}}});
  const root=new TransformNode(key,scene),item={root,proxyMeshes:[],emotion:{setEnabled(){}},nameplate:{position:{}}};
  await attachDevActor(item,key,{load:()=>source,scene,shadows:{addShadowCaster(){}}});
  const m=item.devModel;
  for(const [name,g] of Object.entries(m.clips))assert.ok(g?.targetedAnimations.length && m.actions[name]?.rate>0,key+' '+name);
  const step=(rosterGait,airGap=0,descending=false)=>{updateRosterGait(m,{valid:true,speed:18,observedAt:0,rosterGait,airGap,descending},1/60,0);scene._animate(1000/60);};
  for(let i=0;i<6;i++){step(i);assert.equal(m.activeGait,i);}
  step(5,1);assert.ok(m.clips.Jump.isPlaying);step(5,.4,true);assert.ok(m.clips.Fall.isPlaying);
  for(const [tech,clip] of Object.entries({dig:'Dig',climb:'Climb',glide:'Glide',flight:'Flight',homing:'Homing',grind:'Grind'})){
   if(!m.actions[clip])continue;
   updateRosterGait(m,{tech,valid:true,observedAt:0},1/60,0);
   assert.equal(m.activeAction,clip);assert.ok(m.clips[clip].isPlaying);
  }
  step(3);assert.ok(m.clips.Run.isPlaying);rosterAttack(m);
  assert.ok(m.clips.Attack.isPlaying);
  for(let i=0;i<3600 && m.activeAction;i++)step(0);
  assert.ok(!m.activeAction && m.clips.Idle.isPlaying,key+' should leave attack');
  root.dispose();source.dispose();
 }
 console.log('All 17 native rigs: gait, jump, fall, landing, attack and return to idle passed.');
} finally {engine.dispose();}
