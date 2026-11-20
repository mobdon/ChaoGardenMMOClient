let weeds=[], sprite;
export function drawGardenWeeds(ctx) {
  if(!weeds.length)return;
  if(!sprite){sprite=new Image();sprite.src='/phone-tiny/__tiny-garden/tiny-weed-a2ne.png';}
  if(sprite.naturalWidth!==16)return;
  for(const w of weeds){const size=16*w.scale64/64;if(size>0)ctx.drawImage(sprite,Math.round(w.x-size/2),Math.round(w.y+1-size),size,size);}
}
// Render owner snapshots; a tap proposes a slot, never time, rewards or coordinates.
export function createTinyWeeds({send}) {
  let root, care, owner, connected, pending, acknowledged;
  const buttons=new Map();
  function clear(){for(const button of buttons.values())button.remove();buttons.clear();}
  function render(){
    root ||= document.querySelector('.tiny-garden-crop');
    if(!root)return;
    if(acknowledged!=null && care?.revision>=acknowledged){pending=null;acknowledged=null;}
    const items=care?.tinyWeeds?.rulesVersion==='a2ne-weeds-v1'?care.tinyWeeds.items:[];
    weeds=items||[];
    const keep=new Set();
    for(const w of items||[]){
      const key=w.slot+':'+w.generation;keep.add(key);
      let b=buttons.get(key);
      if(!b){
        b=document.createElement('button');b.type='button';b.className='tiny-weed';
        b.onclick=()=>{if(b.disabled||pending)return;const msg={type:'care',requestId:crypto.randomUUID(),expectedRevision:care.revision,intent:{kind:'tiny_weed_pull',slot:w.slot,generation:w.generation}};
          if(send(msg)!==false){pending=msg;render();}};
        root.append(b);buttons.set(key,b);
      }
      b.style.left=w.x/176*100+'%';b.style.top=w.y/160*100+'%';
      b.disabled=!connected||!!pending||!w.ready||care.eligibility?.canCare!==true;
      b.setAttribute('aria-label',w.pulling?'Pulling weed':w.ready?'Pull weed':'Weed sprouting');
    }
    for(const [key,b] of buttons)if(!keep.has(key)){b.remove();buttons.delete(key);}
  }
  return {update(c,online,id){if(id!==owner||!online){pending=null;acknowledged=null;if(id!==owner)clear();}owner=id;care=c;connected=online;render();},
    handleEvent(e){if(!pending||e.kind!=='care-result'||e.requestId!==pending.requestId)return false;
      if(e.accepted)acknowledged=e.revision;else pending=null;render();return true;}};
}
