import garden from '../asset-tools/crates/tiny-game-presentation/catalog/garden-actors.json';
let visible=null;
export const toyScene=()=>visible;
const art=new Map();
export function drawPlacedToys(ctx,front=false) {
  for(const t of visible?.items||[]){
    if(!t.owned || front!==Boolean(t.id===0 && visible.play?.id===0))continue;
    const file='garden-toy-'+t.id+'-'+t.frame+'.png';
    const entry=garden.assets.find(a=>a.filename===file);if(!entry)continue;
    if(!art.has(file)){const i=new Image();i.src='/phone-tiny/__tiny-garden/'+file;art.set(file,i);}
    const img=art.get(file);if(img.naturalWidth!==entry.width)continue;
    const [dx,dy]=entry.source.native.anchorOffset;
    ctx.drawImage(img,t.x+dx,t.y+dy);
  }
}
export function createTinyToys({root,send}) {
  let care,online,profile,pending,ack,signature,menu,feedback='';
  function act(id,buy){
    const item=visible?.items?.find(i=>i.id===id);
    if(!enabled(item,buy))return;
    const msg={type:'care',requestId:crypto.randomUUID(),expectedRevision:care.revision,intent:{kind:buy?'tiny_toy_buy':'tiny_toy_play',id}};
    if(send(msg)!==false){pending=msg;render();}
  }
  function enabled(item,buy){return online&&!pending&&!visible?.play&&care?.eligibility?.canCare===true&&item&&(buy?item.offered&&profile?.rings>=item.price:item.owned);}
  const canvas=document.querySelector('#tiny-watch-canvas');
  canvas?.addEventListener('click',e=>{
    const b=canvas.getBoundingClientRect(),x=(e.clientX-b.left)/b.width*240,y=(e.clientY-b.top)/b.height*160;
    const t=visible?.items?.find(t=>t.owned&&Math.abs(t.x-x)<=12&&y>=t.y-24&&y<=t.y+4);
    if(t)act(t.id,false);
  });
  function render(){
    if(!menu){menu=document.createElement('section');menu.className='tiny-shop';menu.dataset.stockSection='toys';root.after(menu);}
    const rows=visible?.items||[];menu.hidden=!rows.length;
    const key=JSON.stringify([rows.map(t=>[t.id,t.owned,t.offered,t.price,!!enabled(t,!t.owned)]),feedback]);
    if(key===signature)return;signature=key;menu.replaceChildren();
    const h=document.createElement('h2');h.textContent='Garden toys';menu.append(h);
    const grid=document.createElement('div');grid.className='tiny-shop-grid';menu.append(grid);
    for(const t of rows){
      const c=document.createElement('article');c.className='tiny-shop-item';c.dataset.stockKey='toy:'+t.id;
      const pic=document.createElement('img');pic.src='/phone-tiny/__tiny-garden/garden-toy-'+t.id+'-0.png';pic.alt='';pic.height=40;pic.style.imageRendering='pixelated';
      const title=document.createElement('h3');title.textContent=t.name;
      const button=document.createElement('button');button.type='button';button.className='small-button';
      button.textContent=t.owned?['Play trumpet','Watch rubber duck','Watch TV'][t.id]:t.offered?'Buy for '+t.price+' rings':'Locked';
      button.disabled=!enabled(t,!t.owned);button.onclick=()=>act(t.id,!t.owned);
      c.append(pic,title,button);grid.append(c);
    }
    if(feedback){const note=document.createElement('p');note.setAttribute('role','status');note.textContent=feedback;menu.append(note);}
  }
  return {update(c,p,live){
    if(p?.id!==profile?.id || !live){pending=null;ack=null;feedback='';}
    care=c;profile=p;online=live;visible=c?.tinyToys?.rulesVersion==='tiny-toys-v1'?c.tinyToys:null;
    if(ack!=null&&c?.revision>=ack){pending=null;ack=null;}
    render();
  },handleEvent(e){if(e.kind!=='care-result'||!pending||pending.requestId!==e.requestId)return false;
    if(e.accepted){ack=e.revision;feedback='';if(care.revision>=ack){pending=null;ack=null;}}
    else{pending=null;feedback='Could not use the toy: '+e.code;}render();return true;}};
}

