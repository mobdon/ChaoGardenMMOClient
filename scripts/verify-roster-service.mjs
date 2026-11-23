import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';import {mkdtemp} from 'node:fs/promises';import {resolve} from 'node:path';
import {DEV_ROSTER} from '../src/dev-roster.js';
const dir=await mkdtemp(resolve('.local/roster-proof-'));
const proc=spawn(resolve('.local/rust-gnu-validation/debug/chao-game-server.exe'),[],{cwd:dir,windowsHide:true,env:{...process.env,CHAO_NATIVE_LAND:resolve('.local/extracted/pc-chao/Chao'),CHAO_SKY_RAILS:resolve('.local/extracted/dev-roster/sky-rails.tsv'),CHAO_ROSTER_TABLE:resolve('.local/extracted/dev-roster/pc-tables.tsv'),CHAO_DATA_DIR:dir,GAME_HOST:'127.0.0.1',GAME_PORT:'0',CHAO_OWNER_BOOTSTRAP:'true'}});
const pause=ms=>new Promise(r=>setTimeout(r,ms));const clients=[];
try{
 const url=await new Promise((ok,no)=>{proc.once('error',no);proc.once('exit',()=>no(Error('server exited')));proc.stdout.on('data',b=>{let m=String(b).match(/http:\/\/127\.0\.0\.1:\d+/);if(m)ok(m[0]);});});
 async function client(){const s=await(await fetch(url+'/api/session',{method:'POST',headers:{'content-type':'application/json'},body:'{"name":"Roster test"}'})).json();const w=new WebSocket(url.replace('http:','ws:')+'/ws?token='+encodeURIComponent(s.token));clients.push(w);
 let p;const events=[];w.onmessage=e=>{const m=JSON.parse(e.data);if(m.type==='snapshot')p=m.players.find(x=>x.id===m.selfId);if(m.type==='event' && m.playerId===s.player.id)events.push(m.kind);};
 const wait=async pred=>{for(let i=0;i<80;i++){if(p&&pred(p))return p;await pause(35);}throw Error('Snapshot/event timed out in '+p?.devActor);};await wait(()=>true);return {wait,events,send:m=>w.send(JSON.stringify(m)),act:(action,extra={})=>w.send(JSON.stringify({type:'action',action,...extra}))};}
 const dev=await client();assert.equal((await dev.wait(()=>true)).rosterEnabled,true);
 const visitor=await client();visitor.act('dev-actor',{character:'metalsonic',admin:true});await pause(350);assert.equal((await visitor.wait(()=>true)).devActor,null);
 for(const character of Object.keys(DEV_ROSTER)){
  dev.act('dev-actor',{character});await dev.wait(p=>p.devActor===character);
  dev.send({type:'input',dx:.15,dz:0,jump:true});await dev.wait(p=>p.jumpOffset>.01 && p.rosterGait>0);
  dev.send({type:'input',dx:0,dz:0,jump:false});await dev.wait(p=>p.jumpOffset===0);
  dev.events.length=0;dev.act('attack');await dev.wait(()=>dev.events.includes(character.endsWith('walk')?'mech-shot':'attack'));await pause(150);
 }
 await pause(180);dev.act('mission');await dev.wait(p=>p.zone==='mission');await pause(180);dev.act('dev-actor',{character:'ssonic'});await dev.wait(p=>p.devActor==='ssonic');
 const rings=(await dev.wait(()=>true)).rings;
 for(let i=0;i<14;i++){dev.send({type:'input',tech:true});await pause(100);}
 await dev.wait(p=>p.rosterTech==='flight' && p.jumpOffset>2.5 && p.rings<rings);
 const visitorState=await visitor.wait(()=>true);assert.equal(visitorState.devActor,null);
 await pause(180);dev.act('dev-actor',{character:''});await dev.wait(p=>!p.devActor);console.log('All 17 selected, moved, jumped, landed, attacked and returned to Chao. Visitor could not select.');
}finally{for(const w of clients)w.close();proc.kill();}
