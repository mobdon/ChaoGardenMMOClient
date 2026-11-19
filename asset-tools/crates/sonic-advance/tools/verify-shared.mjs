// No files written. Only a locally supplied ASOE image enters WASM memory.
import assert from 'node:assert/strict';
import {readFile, stat} from 'node:fs/promises';
import {createHash} from 'node:crypto';
const path = process.argv[2];
assert.equal((await stat(path)).size, 8388608);
const rom = await readFile(path);
const wasm = await readFile(new URL('../wasm/target/wasm32-unknown-unknown/release/asoe_runtime_wasm.wasm',import.meta.url));
const {instance:{exports:a}} = await WebAssembly.instantiate(wasm);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
// Allocate first: a WASM allocation may invalidate the previous memory view.
function load(title=false) {
  const ptr=a.asoe_allocate_rom(rom.length);
  assert.ok(ptr); new Uint8Array(a.memory.buffer,ptr,rom.length).set(rom);
  assert.equal((title?a.asoe_load_title:a.asoe_load)(rom.length),0);
}
function snapshot() {
  const ptr=a.asoe_snapshot_ptr(); assert.ok(ptr);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(a.memory.buffer,ptr,a.asoe_snapshot_len())));
}
function pixels() {return new Uint8Array(a.memory.buffer,a.asoe_frame_ptr(),a.asoe_frame_len());}
function step(mask,n=1) {assert.equal(a.asoe_step(mask,n),0);}
load(true);
let time=0;
for (const {tick,bgr555Sha256} of JSON.parse(await readFile(new URL('../tests/retail-title-frames.json',import.meta.url),'utf8'))) {
  while(time<tick) {const n=Math.min(4,tick-time);step(0,n);time+=n;}
  const p=pixels(), packed=Buffer.alloc(240*160*2);
  for(let i=0;i<p.length;i+=4) packed.writeUInt16LE((p[i]>>3)|((p[i+1]>>3)<<5)|((p[i+2]>>3)<<10),i/2);
  assert.equal(hash(packed),bgr555Sha256);
}
assert.equal(a.asoe_select_act(0),0);
const initial=snapshot();
assert.equal(a.asoe_select_act(12),-1);
assert.equal(a.asoe_step(2,1),-1); assert.equal(a.asoe_step(0,5),-1);
assert.equal(a.asoe_allocate_rom(rom.length-1),0);
const invalid=a.asoe_allocate_rom(rom.length); assert.ok(invalid);
assert.equal(a.asoe_load(rom.length),-1); assert.deepEqual(snapshot(),initial);
const mask=i=>(i>=30?16:0)|(i%180>=100&&i%180<120?1:0);
const states=[];
for(let i=0;i<600;i++) {step(mask(i));states.push(snapshot());}
assert.equal(hash(JSON.stringify(states)),'ab9fc4e27419236a7ffd9d1f7fd93ad63946a7d951695c3f151f5fb40b542506');
assert.equal(hash(pixels().filter((_,i)=>i%4!==3)),'0e987956075c9482f117bd87d58d4705879e1f70ac18deeda24f9b3f791eae0f');
assert.equal(a.asoe_reset(),0); assert.deepEqual(snapshot(),initial);
for(let i=0;i<25;i++)step(0);
const fields=['xQ8','yQ8','vxQ8','vyQ8','cameraX','cameraY','collisionLayer','localRings'];
const packed=Buffer.alloc(600*32);
for(let i=0;i<600;i++){step(mask(i));const s=snapshot();fields.forEach((k,j)=>packed.writeInt32LE(s[k],32*i+4*j));}
assert.equal(hash(packed),'f3191a348b928b9fcf28f22314a091a750675941f28b35707dbb5e42338f8f34');
console.log('PASS: 11 retail title frames, 600 desktop snapshots and final RGB, 600 retail movement frames, invalid inputs and reset.');
