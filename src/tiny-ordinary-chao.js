import rigCatalog from '../asset-tools/crates/tiny-game-presentation/catalog/garden-rig-poses.json';
import {getTinyFlameLayer} from './tiny-flame.js';

const images=new Map(), rigs=new Map(rigCatalog.assets.map(a=>[a.filename,a]));
function load(name) {
  if (!images.has(name)) {const img=new Image();img.src='/phone-tiny/__tiny-garden/'+name;images.set(name,img);}
  return images.get(name);
}
// The Rust snapshot selects the activity. Only display time drives the cycle.
export function tinyPose(profile,{time=0,walking=false,direction=0,gardenPose}={}) {
  const ticks=Math.floor(Math.max(0,time)*262144/4389000);
  const mirror=direction>=5, side=direction===2||direction===6;
  const frame=Number.isInteger(gardenPose)&&gardenPose>=0&&gardenPose<67?gardenPose
    :profile?.watch?.activity==='rest'?((ticks&96)===0?53:54)
    :walking?(side?3:direction>=3&&direction<=5?6:0)+[0,1,0,2][Math.floor(ticks/18)%4]:27;
  return {frame,mirror,ticks};
}
export function drawGardenChao(ctx,profile,x,y,size,options,variant,flame) {
  const form=String(profile?.form||'').toLowerCase();
  const pose=tinyPose(profile,options);
  const normal=form==='normal';
  let name=variant?variant+'-chaos':form;
  if(name==='dark-chaos'&&profile.admin){
    if(profile.gear?.footwear==='shadow-air-shoes')name+='-footwear';
    if(profile.gear?.hands==='knuckles-hammer-gloves')name+='-hands';
    name+='-chest';
  }
  const reaction=careReactions.get(profile?.id);
  const elapsed=reaction?performance.now()-reaction.start:Infinity;
  if(!Number.isInteger(options.gardenPose)&&elapsed<2400){
    const pet=reaction.kind==='pet';
    pose.frame=pet&&name.endsWith('-chest')?67+Math.min(4,Math.floor(elapsed/480))
      :(pet?39:60)+Math.floor(elapsed/300)%2;
  }
  const file=normal?'garden-normal-'+Number(pose.mirror)+'.png':'garden-'+name+'.png';
  if(!normal&&!rigs.has(file))return false;
  const img=load(file),cell=normal?40:64;
  if(!img.complete||img.naturalWidth!==cell*8||img.naturalHeight!==cell*9)return false;
  const scale=size/(normal?34:55),foot=y+size*.375;
  ctx.save();ctx.imageSmoothingEnabled=false;ctx.translate(x,foot);if(!normal&&pose.mirror)ctx.scale(-1,1);
  ctx.drawImage(img,(pose.frame%8)*cell,Math.floor(pose.frame/8)*cell,cell,cell,-cell*.5*scale,-(normal?35:.86*cell)*scale,cell*scale,cell*scale);
  const glow=variant==='dark'&&getTinyFlameLayer(flame);
  if(glow){
    const top=(rigs.get(file).headY[pose.frame]-.86*cell)*scale;
    ctx.globalCompositeOperation='lighter';
    ctx.drawImage(glow,(Math.floor((options.time||0)*.018)%16)*32,0,32,32,-size*.15,top-size*.27,size*.3,size*.3);
  }
  ctx.restore();return true;
}


const careReactions=new Map();
let observedOwner, lastCare;
export function observeCarePose(player,care,connected) {
  const actions=(care?.history||[]).filter(e=>e.kind==='pet'||(e.kind==='consume'&&/^(garden-fruit|tiny-fruit:[0-6])$/.test(e.itemId)));
  if(player?.watch?.lastCare?.kind==='pet')actions.push({...player.watch.lastCare,id:'watch:'+player.watch.lastCare.at});
  const latest=actions.sort((a,b)=>b.at-a.at)[0];
  const key=latest?.id;
  if(!connected||observedOwner!==player?.id){careReactions.clear();observedOwner=connected?player?.id:null;lastCare=key;return;}
  if(key&&key!==lastCare){careReactions.set(player.id,{kind:latest.kind,start:performance.now()});lastCare=key;}
}
