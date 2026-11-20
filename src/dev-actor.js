import { TransformNode } from '@babylonjs/core';
export async function attachDevActor(item,key,ctx){
  const container=await ctx.load('dev-'+key);
  if(!container||item.root.isDisposed())return;
  const metal = key==='metalsonic' || key==='metalsonicblack';
  const source=container.transformNodes.concat(container.meshes).find(n=>n.metadata?.gltf?.extras?.sa2RosterActions);
  const actions=source ? JSON.parse(source.metadata.gltf.extras.sa2RosterActions) : null;
  const gaitRates=actions ? nativeGaits.map(([name])=>actions[name].rate) : null;
  const visual=new TransformNode('dev-'+key,ctx.scene);visual.parent=item.root;visual.rotation.y=-Math.PI/2;
  const entries=container.instantiateModelsToScene(n=>'dev-'+n,false,{doNotInstantiate:true});
  visual.metadata={importedEntries:entries};
  for(const n of entries.rootNodes)n.parent=visual;
  for(const g of entries.animationGroups)g.stop();
  const clips=Object.fromEntries(Object.keys(actions || Object.fromEntries([...nativeGaits.map(([n])=>n),'Jump','Fall','Attack'].map(n=>[n,true]))).map(k=>[k,entries.animationGroups.find(g=>g.name.endsWith(k))]));
  clips.Idle?.start(true,gaitRates?.[0] ?? 1);
  const meshes=visual.getChildMeshes().filter(m=>m.getTotalVertices());
  const ys=meshes.flatMap(m=>{m.computeWorldMatrix(true);const b=m.getBoundingInfo().boundingBox;return [b.minimumWorld.y,b.maximumWorld.y].map(y=>y-item.root.position.y);});
  const height=key.endsWith('walk')?1.65:1;
  const scale=height/Math.max(.01,Math.max(...ys)-Math.min(...ys));
  visual.scaling.setAll(scale);visual.position.y=-Math.min(...ys)*scale;
  for(const m of meshes){m.isPickable=false;m.receiveShadows=true;ctx.shadows.addShadowCaster(m,false);}
  for(const m of item.proxyMeshes)m.setEnabled(false);
  item.emotion.setEnabled(false);item.nameplate.position.y=height+.22;
  item.devModel={clips,actions,visual,gaitRates,eyeMeshes:metal ? meshes.filter(m=>/attach_0000(?:1650|143C)/.test(m.name)) : [],baseY:visual.position.y,eyeLift:0,activeGait:0,weights:[1,0,0,0,0,0],meshes,skeletons:[...new Set(meshes.map(m=>m.skeleton).filter(Boolean))],height,speed:0,gaits:rosterGaits.filter(([name])=>clips[name])};
  if(metal)item.devModel.standEye=eyeFloor(item.devModel)-item.root.position.y;
  if(item.self)ctx.canvas.dataset.devCharacter=key;
}

// Overlap neighboring native poses over small speed bands. Stop zero-weight
// groups so releasing movement leaves only the original standing loop.
// SA2 PC AnimationInfo states 0,6,7,8,9,10 for models 418 and 65.
const nativeGaits=[['Idle',0],['Walk',.06],['Speedwalk',.60],['Run',1.0],['FastSkate',1.42],['SprintSkate',1.83]];
const rosterGaits = [['Idle',0],['Walk',.25],['Run',1.1],['FastSkate',1.55],['SprintSkate',1.95]];
function rosterAction(model,name,loop){
  const clip=model.clips[name];
  if(!clip)return;
  for(const g of Object.values(model.clips))if(g?.isStarted)g.stop();
  model.weights.fill(0);model.activeAction=name;
  clip.start(loop,model.actions?.[name]?.rate || 1);clip.setWeightForAllAnimatables(1);
}
export function rosterAttack(model){if(model)rosterAction(model,'Attack',false);}
export function updateRosterGait(model,sample,dt,now){
  if(model.activeAction==='Attack' && model.clips.Attack?.isPlaying)return;
  const special={grind:'Grind',climb:'Climb',glide:'Glide',dig:'Dig',homing:'Homing',flight:sample?.descending?'Fall':sample?.rising?'Jump':'Flight'}[sample?.tech];
  if(special && model.clips[special] && now-sample.observedAt<350){if(model.activeAction!==special)rosterAction(model,special,true);return;}
  const air=sample?.valid && sample.airGap>.012 && Number.isFinite(sample.airGap) && now-sample.observedAt<350;
  if(air){
    const name=sample.descending ? 'Fall' : 'Jump';
    if(model.activeAction!==name)rosterAction(model,name,true);
    return;
  }
  if(model.activeAction){model.clips[model.activeAction]?.stop();model.activeAction=null;}
  const gaits=model.gaits;
  if(!gaits.length)return;
  const target = sample?.valid && now-sample.observedAt < 350 ? sample.speed : 0;
  model.speed += (target-model.speed)*(1-Math.exp(-dt*(target ? 10 : 24)));
  if (!target && model.speed < .015) model.speed=0;
  const speed=model.speed;
  if(model.gaitRates){
    // Play one original action at its exported rate. Thresholds map garden
    // speed units to SA2 actions; hysteresis prevents snapshot jitter.
    const levels=nativeGaits;

    let i=model.activeGait;
    while(i+1<levels.length && speed>=levels[i+1][1]+.02)i++;
    while(i>0 && speed<levels[i][1]-.02)i--;
    if(Number.isInteger(sample?.rosterGait) && sample.rosterGait>=0 && sample.rosterGait<6)i=sample.rosterGait;
    model.activeGait=i;
    // Preserve each clip's native timing and transform. Fade the previous
    // pose out instead of snapping to the first frame of the next action.
    const alpha=1-Math.exp(-dt/.095);
    const outgoing=levels.map(([name],j)=>({clip:model.clips[name],w:model.weights[j],j}))
      .filter(x=>x.j>0 && x.clip?.isStarted && x.clip.to>x.clip.from)
      .sort((a,b)=>b.w-a.w)[0]?.clip;
    const phase=outgoing ? (outgoing.getCurrentFrame()-outgoing.from)/(outgoing.to-outgoing.from) : 0;
    for(let j=0;j<levels.length;j++){
      const clip=model.clips[levels[j][0]];
      if(!clip)continue;
      let w=model.weights[j]+((j===i?1:0)-model.weights[j])*alpha;
      if(j!==i && w<.001)w=0;
      model.weights[j]=w;
      if(w>0){
        if(!clip.isStarted){
          clip.start(true,model.gaitRates[j]);
          if(j && outgoing)clip.goToFrame(clip.from+(clip.to-clip.from)*phase);
        }
        clip.setWeightForAllAnimatables(w);
      }else if(clip.isStarted)clip.stop();
    }
    return;
  }
  const upper=gaits.findIndex(([,at])=>at>speed);
  const low=upper<0 ? gaits.length-1 : Math.max(0,upper-1);
  const mix=upper>low ? (speed-gaits[low][1])/(gaits[upper][1]-gaits[low][1]) : 0;
  for(const [i,[name]] of gaits.entries()){
    const group=model.clips[name];
    if(!group)continue;
    const weight=i===low ? 1-mix : i===upper ? mix : 0;
    if(weight>0){
      if(!group.isStarted)group.start(true);
      group.setWeightForAllAnimatables(weight);
    }else if(group.isStarted) group.stop();
  }
}

// Both Metal skins share model 418 and its two native eye meshes. Correct
// only the visual root's elevation; never rotate the head or change a joint.
function eyeFloor(model){
  for(const sk of model.skeletons)sk.prepare(true);
  let y=Infinity;
  for(const mesh of model.eyeMeshes){
    const p=mesh.getPositionData(true,true), w=mesh.computeWorldMatrix(true).m;
    for(let i=0;i<p.length;i+=3)y=Math.min(y,p[i]*w[1]+p[i+1]*w[5]+p[i+2]*w[9]+w[13]);
  }
  return y;
}
// A rotated bounding box can extend well below a walker's actual sole.
// Project the animated vertices onto world Y after the native pose evaluates.
export function groundRosterActor(model, targetY, parentScaleY, dt=1/60) {
  // Metal's native actions animate height relative to the standing origin.
  // Pinning each pose's lowest vertex to the floor erases the authored hover.
  if(model.gaitRates){
    if(Number.isFinite(model.standEye)){
      const nativeY=eyeFloor(model)-targetY-model.eyeLift*parentScaleY;
      const hover=model.weights.slice(3).reduce((a,b)=>a+b,0);
      const target=Math.max(0,model.standEye-nativeY)*hover/parentScaleY;
      model.eyeLift+=(target-model.eyeLift)*(1-Math.exp(-dt/.08));
      model.visual.position.y=model.baseY+model.eyeLift;
    }
    return;
  }
  let sole = Infinity;
  for (const skeleton of model.skeletons) skeleton.prepare(true);
  for (const mesh of model.meshes) {
    const w = mesh.computeWorldMatrix(true).m;
    const positions = mesh.getPositionData(true, true);
    if (!positions) continue;
    for (let i = 0; i < positions.length; i += 3) {
      sole = Math.min(sole, positions[i]*w[1] + positions[i+1]*w[5] + positions[i+2]*w[9] + w[13]);
    }
  }
  if (Number.isFinite(sole)) {
    model.visual.position.y += (targetY-sole)/parentScaleY;
  }
}
