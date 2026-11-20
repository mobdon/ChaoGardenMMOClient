import './tiny-native-games.css';

const originals = new Map([['memory','a2ne-memory-v1'], ['ball','a3ve-ball-v1'], ['rock_paper_scissors','asoe-rps-v1'], ['bounce','a2ne-bounce-v1']]);
export function hasOriginalGame(id, capabilities) {
  const entry = capabilities?.nativeTinyGames?.[id];
  return originals.has(id) && entry?.native === true && entry.rulesVersion === originals.get(id);
}
const keys = { ArrowLeft: 32, ArrowRight: 16, ArrowUp: 64, ArrowDown: 128, KeyZ: 1, Space: 1, KeyX: 2, Enter: 8 };
const masks = { bounce: 0xfb, memory: 0xf3, ball: 0xfb, rock_paper_scissors: 0x31 };
const tips = {
  bounce: 'Touch and hold where the spring should move. Tap to launch.',
  memory: 'Tap a card to turn it over. During the shuffle, tap to trip the Chao.',
  ball: 'Touch a column to aim. Tap A or B on the picture to fire.',
  rock_paper_scissors: 'Tap a lane to throw its card.',
};
// Presentation and bounded held buttons only. The Rust session owns every
// frame, cursor, hit, board, timer, random draw, result and saved ring change.
export function createNativeTinyGames({ root, send }) {
  const stage = document.createElement('section'); stage.className = 'tiny-native-game'; stage.hidden = true;
  const canvas = document.createElement('canvas'); canvas.width = 720; canvas.height = 480;
  canvas.setAttribute('role', 'img'); canvas.setAttribute('aria-label', 'Original Tiny Chao Garden minigame');
  const notice = document.createElement('p'); notice.className = 'tiny-panel-copy'; notice.setAttribute('role', 'status');
  const frame = document.createElement('div'); frame.className='tiny-touch-frame'; frame.append(canvas);
  // These transparent buttons sit on the original 6 x 5 board, which
  // occupies the left 192 pixels of the 240-pixel game frame.
  const cards=document.createElement('div');cards.className='tiny-memory-touch';cards.hidden=true;frame.append(cards);
  const cardButtons=Array.from({length:30},(_,i)=>{
    const b=document.createElement('button');b.type='button';b.dataset.cell=i;
    b.setAttribute('aria-label', 'Card, row '+(1+Math.floor(i/6))+', column '+(1+i%6));
    cards.append(b);return b;
  });
  let memoryPicks=[], sentPick=null, pickDeadline=0;
  const overlay = document.createElement('div'); overlay.className='tiny-touch-overlay'; frame.append(overlay);
  const buttons = new Map();
  for (const [bit,label] of [[8,'Ⅱ'],[1,'A'],[2,'B'],[64,'Resume'],[128,'Quit']]) {
    const button=document.createElement('button'); button.type='button'; button.textContent=label;
    button.setAttribute('aria-label', {8:'Pause',1:'Fire A',2:'Fire B',64:'Resume',128:'Quit round'}[bit]);
    overlay.append(button); buttons.set(bit,button);
  }
  stage.append(frame,notice); root.after(stage);
  let target=null, finger=null, origin=null, tapMark=null, busyUntil=0;
  const context = canvas.getContext('2d');
  const images = new Map(), keyboard = new Set(), pointers = new Map();
  let run = null, enabled = false, visible = false, active = false, sent = 0, assetCatalog = null;
  const image = key => {
    if (images.has(key)) return images.get(key);
    const asset = assetCatalog?.assets?.find(asset => asset.key === key || asset.filename === key);
    if (!asset) return null;
    const value = new Image(); value.onload = value.onerror = () => draw();
    value.src = `/phone-tiny/__tiny-games/${asset.filename}`; images.set(key, value); return value;
  };
  void fetch('/phone-tiny/__tiny-games/catalog.json', { cache: 'no-store' }).then(response => {
    if (!response.ok) return null; return response.json();
  }).then(value => { if (value && Array.isArray(value.assets)) { assetCatalog = value; draw(); } }).catch(() => {});
  const paint = (key, x, y, width, height) => {
    const sprite = image(key);
    if (!sprite?.complete || !sprite.naturalWidth) return false;
    context.drawImage(sprite, x, y, width || sprite.naturalWidth, height || sprite.naturalHeight); return true;
  };
  function held() {
    let value = 0; for (const bit of keyboard) value |= bit; for (const bit of pointers.values()) value |= bit;
    return enabled && visible && !document.hidden ? value & (masks[run?.gameId] || 0) : 0;
  }
  function transmit(force = false) {
    if (!run?.id || !active) return;
    const value = held();
    if (force || value !== sent) { send({ type: 'tiny-input', sessionId: run.id, input: { kind: 'native_buttons', held: value } }); sent = value; }
    for (const [bit, button] of buttons) button.classList.toggle('held', !!(value & bit));
  }
  function release() { memoryPicks=[];sentPick=null;tapMark=null; target=null; finger=null; origin=null; keyboard.clear(); pointers.clear(); transmit(); }
  window.addEventListener('keydown', event => {
    if (!active || !visible || !enabled || !keys[event.code] || !(keys[event.code] & masks[run.gameId])) return;
    event.preventDefault(); event.stopImmediatePropagation(); keyboard.add(keys[event.code]); transmit();
  });
  window.addEventListener('keyup', event => { if (!keys[event.code]) return; keyboard.delete(keys[event.code]); transmit(); });
  window.addEventListener('blur', release); window.addEventListener('pagehide', release);
  document.addEventListener('visibilitychange', () => { if (document.hidden) release(); });
  function pulse(bit) { pointers.set('pulse',bit); transmit(); pointers.delete('pulse'); transmit(); }
  function advancePicks(){
    const s=run?.view?.state;
    if(!enabled || !visible || run?.gameId!=='memory' || s?.phase!=='matching'){memoryPicks=[];sentPick=null;return;}
    const picked=(s.selected||[]).filter(Boolean).map(c=>c.row*6+c.column);
    if(sentPick!==null){
      if(picked.includes(sentPick) || !s.cells[sentPick]?.occupied)sentPick=null;
      else if(performance.now()>pickDeadline){sentPick=null;memoryPicks=[];return;}
      else return;
    }
    if(picked.length===2 || !s.cursor)return;
    while(memoryPicks.length){
      const i=memoryPicks.shift();
      if(picked.includes(i)||!s.cells[i]?.occupied)continue;
      // The original cursor wraps. At most three diagonal or straight steps
      // are needed; apply A on the final step, as in the native callback.
      const shortest=(d,n)=>((d+n*2+Math.floor(n/2))%n)-Math.floor(n/2);
      let dx=shortest(i%6-s.cursor[0],6),dy=shortest(Math.floor(i/6)-s.cursor[1],5);
      sentPick=i;pickDeadline=performance.now()+2000;
      do {let bit=(dx<0?32:dx>0?16:0)|(dy<0?64:dy>0?128:0);
        dx-=Math.sign(dx);dy-=Math.sign(dy);pulse(bit|(!dx&&!dy?1:0));
      } while(dx||dy);
      break;
    }
  }
  cards.addEventListener('click',e=>{
    const b=e.target.closest('button[data-cell]');if(!b||b.disabled||!enabled||!visible)return;
    const i=Number(b.dataset.cell);
    if(i===sentPick||memoryPicks.includes(i)||memoryPicks.length+(sentPick!==null?1:0)>=2)return;
    memoryPicks.push(i);tapMark={x:i%6*32,y:Math.floor(i/6)*32,w:32,h:32,until:performance.now()+500};advancePicks();draw();
  });
  function steer(bit) { pointers.set('steer',bit); transmit(); }
  function place(e) {
    const rect=canvas.getBoundingClientRect();
    return [(e.clientX-rect.left)*240/rect.width,(e.clientY-rect.top)*160/rect.height];
  }
  function follow() {
    if (!target || !active || !visible || !enabled) return;
    const s=run.view?.state, t=target;
    if (!s || performance.now()>t.until) {target=null;steer(0);return;}
    if (t.menu) {
      if(s.screen!=='pause'){target=null;return;}
      if(s.quitSelected===t.quit){target=null;pulse(1);}
      else if(!t.wait){t.wait=true;pulse(t.quit?128:64);}
      return;
    }
    let x,y;
    if(run.gameId==='rock_paper_scissors' && s.screen==='playing') x=s.lane;
    else if(run.gameId==='ball' && s.screen==='playing') x=s.player?.column;
    else if(run.gameId==='bounce' && s.screen==='playing') {
      const dx=t.x-s.springX; steer(Math.abs(dx)<5?0:dx>0?16:32);return;
    } else {target=null;steer(0);return;}
    if(!Number.isFinite(x)) return;
    // The native callbacks move first, then process A in the same frame.
    // A tap one step away needs no intervening snapshot to select its card.
    if(run.gameId==='rock_paper_scissors'){
      if(s.cards?.[t.x]?.state!==0){target=null;busyUntil=performance.now()+700;return;}
      const d=t.x-x;
      if(Math.abs(d)<=1){target=null;pulse((d===0?0:d>0?16:32)|1);return;}
    }
    const move=x!==t.x?(x<t.x?16:32):y!==t.y?(y<t.y?128:64):0;
    if(move) {
      if(run.gameId==='ball' && s.player.phase>2)return;
      const position=x+':'+y;
      if(t.last!==position || performance.now()-t.stepAt>400){t.last=position;t.stepAt=performance.now();pulse(move);}
    } else if(run.gameId!=='ball' || s.player.phase<=2) {
      target=null;if(t.action)pulse(t.action);
    }
  }
  for(const [bit,button] of buttons) button.addEventListener('click',()=>{
    if(!active||!visible||!enabled)return;
    if(bit===64||bit===128){target={menu:true,quit:bit===128,until:performance.now()+1500};follow();}
    else if(run.gameId==='ball'&&target&&!target.menu&&bit!==8){target.action=bit;follow();}
    else {if(bit===8){target=null;steer(0);}pulse(bit);}
  });
  function aim(x,y) {
    const s=run.view.state, id=run.gameId, until=performance.now()+2500;
    if(s.screen==='pause')return;
    if(id==='rock_paper_scissors' && s.screen==='playing'){
      if(x>=0 && x<192){const lane=Math.floor(x/64);target={x:lane,action:1,until};tapMark={x:lane*64+12,y:116,w:32,h:32,until:performance.now()+380};}
    } else if(id==='ball' && s.screen==='playing'){
      const col=Math.max(1,Math.min(10,Math.floor(x/16)));target={x:col,until};tapMark={x:col*16,y:0,w:16,h:160,until:performance.now()+450};
    } else if(id==='bounce' && s.screen==='playing'){target={x,until:Infinity};}
    else {pulse(1);return;}
    follow();draw();
  }
  canvas.addEventListener('pointerdown',e=>{
    if(!active||!visible||!enabled||finger!==null||e.button!==0)return;
    e.preventDefault();canvas.setPointerCapture(e.pointerId);finger=e.pointerId;origin=place(e);
    if(['bounce','ball','rock_paper_scissors'].includes(run.gameId) && run.view.state.screen==='playing')aim(...origin);
  });
  canvas.addEventListener('pointermove',e=>{
    if(e.pointerId!==finger)return;
    const [x,y]=place(e);
    if(Math.hypot(x-origin[0],y-origin[1])>4)origin[2]=true;
    if(run.view.state.screen==='playing' && (run.gameId==='bounce'||run.gameId==='ball' && target?.x!==Math.max(1,Math.min(10,Math.floor(x/16)))))aim(x,y);
  });
  canvas.addEventListener('pointerup',e=>{
    if(e.pointerId!==finger)return;
    const tap=!origin[2],s=run.view.state;finger=null;origin=null;
    if(run.gameId==='bounce' && s.screen==='playing'){target=null;steer(0);if(tap)pulse(1);}
    else if(tap && !(['ball','rock_paper_scissors'].includes(run.gameId)&&s.screen==='playing') && !(run.gameId==='memory'&&s.phase==='matching'))aim(...place(e));
  });
  for(const type of ['pointercancel','lostpointercapture'])canvas.addEventListener(type,e=>{if(e.pointerId===finger)release();});
  setInterval(() => { if (active && enabled && visible && held()) transmit(true); }, 100);
  function draw() {
    if (!active) return;
    const state = run?.view?.state; if (!state) return;
    context.clearRect(0, 0, canvas.width, canvas.height); context.save(); context.scale(3, 3); context.imageSmoothingEnabled = false;
    let ready = true;
    const art=(key,x,y,w,h)=>{ready=paint(key,x,y,w,h)&&ready;};
    const anchored=(key,x,y,m)=>{
      const meta=assetCatalog?.assets?.find(row=>row.key===key);
      const [ox,oy]=meta?.source?.native?.anchorOffset||[0,0];
      context.save();context.translate(x,y);
      if(meta && m?.scaleX && m?.scaleY){
        context.translate(ox+meta.width/2,oy+meta.height/2);
        context.rotate(m.rotation*Math.PI/32768);context.scale(256/m.scaleX,256/m.scaleY);
        art(key,-meta.width/2,-meta.height/2);
      }else art(key,ox,oy);
      context.restore();
    };
    if(run.gameId==='memory'){
      art('memory-board',0,0);
      for(const [i,c] of (state.cells||[]).entries()){
        const x=i%6*32,y=Math.floor(i/6)*32;art('memory-zone-'+c.ringValue,x,y);
        if(c.occupied)art(c.face==null?'memory-card-back':'memory-card-'+c.face,x,y);
      }
      // Interface markers for the not-yet-exported cursor and shuffle walker.
      if(state.cursor){context.strokeStyle='#ffdd48';context.lineWidth=2;context.strokeRect(state.cursor[0]*32+1,state.cursor[1]*32+1,30,30);}
      if(state.walker){context.fillStyle='#fff';context.fillRect(state.walker.x-2,state.walker.y-2,4,4);}
    }else if(run.gameId==='ball'){
      art('ball-background',0,0);
      const names={1:'ball-color-1',2:'ball-color-2',48:'ball-ring-1',64:'ball-ring-5',80:'ball-ring-10',240:'ball-ceiling'};
      for(let row=0;row<10;row++)for(let col=0;col<10;col++){const cell=state.board?.[row]?.[col]||0;
        const key=names[cell&240]||names[cell&15];if(key)art(key,16+col*16,row*16-state.scroll);}
      if(state.player)anchored('ball-player-'+Number(state.player.facingRight)+'-'+Math.min(17,state.player.animFrame),state.player.x,159);
      if(state.shot)anchored('ball-shot-'+state.shot.color+'-0',8+16*state.shot.column,state.shot.y);
      for(const ring of state.fallingRings||[])anchored('ball-ring-falling',ring.x,ring.y);
    }else if(run.gameId==='bounce'){
      art('bounce-world',0,-state.scroll);
      const actor=(a,m)=>{if(a?.visible&&a.descriptor!=null)anchored(['bounce-obj',a.descriptor.toString(16),a.tile,a.style,a.transform,a.paletteLayer].join('-'),a.x,a.y,m);};
      for(const a of state.clouds||[])actor(a);for(const a of state.targets||[])actor(a);
      for(const p of state.bumpers||[])actor(p.actor,p.affine);
      for(const p of state.springParts||[])actor(p.actor,p.affine);
      actor(state.chao);art('bounce-hud',0,0);
    }else{
      art('rps-background',0,0);
      for(const t of state.targets||[])art('rps-target-'+t.gesture,t.x,t.y);
      for(const c of state.cards||[])if(c.gesture!=null)art('rps-card-'+c.gesture,c.x,c.y);
      if(state.actor)art('rps-actor-frame-0',state.actor.x-12,state.actor.y-24);
      context.fillStyle='#ffdd48';context.fillRect(state.markerX,155,12,2);
    }
    if(tapMark && performance.now()<tapMark.until){context.strokeStyle='#fff';context.lineWidth=1;context.strokeRect(tapMark.x+1,tapMark.y+1,tapMark.w-2,tapMark.h-2);}
    context.restore();
    notice.textContent = !ready ? 'Original game artwork is still loading.' : state.screen==='pause' ? 'Paused. Tap Resume or Quit on the game screen.' : run.gameId==='memory' && state.phase==='reveal' ? 'Memorize the cards. You can pick a card after the shuffle.' : run.gameId==='memory' && state.phase==='shuffle' ? 'The Chao is shuffling. Tap to trip it.' : run.gameId==='rock_paper_scissors' && performance.now()<busyUntil ? 'That card is still in play. Tap a ready card.' : tips[run.gameId];
    canvas.setAttribute('aria-label', `${run.gameId}: ${state.phase || state.screen || 'playing'}, ${state.rings || 0} rings${state.cursor ? `, cursor column ${state.cursor[0] + 1}, row ${state.cursor[1] + 1}` : ''}`);
  }
  return {
    update(next, connection) {
      const nextActive = !!next && originals.has(next.gameId) && originals.get(next.gameId) === next.rulesVersion;
      if (run?.id !== next?.id || !nextActive || !connection.focused || !connection.connected || next?.phase !== 'playing') release();
      run = next; active = nextActive; visible = connection.focused === true; enabled = connection.connected === true && run?.phase === 'playing';
      stage.hidden = !active;
      const state=run?.view?.state;
      cards.hidden=!(active && run.gameId==='memory' && state?.phase==='matching');
      if(!cards.hidden)for(const [i,b] of cardButtons.entries())b.disabled=!enabled||!visible||!state.cells?.[i]?.occupied;
      advancePicks();
      if (active) {
        const paused=run.view?.state?.screen==='pause';
        for(const [bit,button] of buttons){
          button.hidden=paused ? bit!==64 && bit!==128 : bit===8 ? !['ball','bounce'].includes(run.gameId) : ![1,2].includes(bit)||run.gameId!=='ball'||run.view?.state?.screen!=='playing';
          button.disabled=!enabled||!visible;
        }
        follow();draw();
      }
      return active;
    },
    get active() { return active; },
    get metrics() { const state = run?.view?.state; return state ? [`${state.rings || 0} run rings`, state.phase || state.screen || 'Playing', run.gameId === 'memory' ? `${state.matchedPairs}/7 pairs` : run.gameId === 'ball' ? `Stage ${state.stage}/4` : run.gameId==="bounce" ? `${state.lives} lives` : `${state.seconds}s`] : []; },
  };
}
