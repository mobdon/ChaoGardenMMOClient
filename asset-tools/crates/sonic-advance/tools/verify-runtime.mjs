// Compare compiled native Rust and Rust/WASM state + pixels. No ROM data is
// logged, saved, uploaded or served. Requires the user's exact local source.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const [romPath, nativePath] = process.argv.slice(2);
if (!romPath || !nativePath || process.argv.length !== 4) {
  throw new Error('Usage: node tools/verify-runtime.mjs LOCAL_ASOE_ROM NATIVE_REPLAY_EXECUTABLE');
}
const wasmPath = new URL('../wasm/target/wasm32-unknown-unknown/release/asoe_runtime_wasm.wasm', import.meta.url);
const plan = JSON.parse(await readFile(new URL('./replay-plan.json', import.meta.url), 'utf8'));
const rom = await readFile(romPath);
assert.equal(rom.length, 8 * 1024 * 1024);
assert.equal(createHash('sha256').update(rom).digest('hex'), 'afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020');
const native = spawnSync(nativePath, [romPath], { encoding:'utf8', timeout:120000, maxBuffer:1024*1024, windowsHide:true });
assert.equal(native.status, 0, native.stderr || native.error?.message);
const expected = JSON.parse(native.stdout);
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const w = instance.exports;
const text = (ptr,len) => new TextDecoder().decode(new Uint8Array(w.memory.buffer,ptr,len));
const error = () => text(w.asoe_error_ptr(),w.asoe_error_len());
const ok = status => assert.equal(status,0,error());
const snapshot = () => { const ptr=w.asoe_snapshot_ptr();assert.notEqual(ptr,0,error());return JSON.parse(text(ptr,w.asoe_snapshot_len())); };
const pixels = () => { const ptr=w.asoe_frame_ptr();const len=w.asoe_frame_len();assert.equal(len,240*160*4);return new Uint8Array(w.memory.buffer,ptr,len); };
const checkpoint = () => ({ snapshot:snapshot(),rgbaSha256:createHash('sha256').update(pixels()).digest('hex') });

assert.equal(w.asoe_step(0,1),-1);
assert.equal(w.asoe_reset(),-1);
assert.equal(w.asoe_allocate_rom(1),0);
assert.equal(w.asoe_load(rom.length),-1);
assert.equal(w.asoe_frame_len(),0);
assert.equal(w.asoe_snapshot_ptr(),0);assert.equal(w.asoe_snapshot_len(),0);
let ptr=w.asoe_allocate_rom(rom.length);
assert.notEqual(ptr,0,error());
assert.equal(w.asoe_load(rom.length),-1,'zero-filled wrong source must fail fingerprint');
assert.match(error(),/ASOE|SHA|source|header|ROM/i);
ptr=w.asoe_allocate_rom(rom.length);
assert.notEqual(ptr,0,error());
new Uint8Array(w.memory.buffer,ptr,rom.length).set(rom);
ok(w.asoe_load(rom.length));
const actual=[checkpoint()];
for (const segment of plan) {
  for(let tick=0;tick<segment.ticks;tick++) ok(w.asoe_step(segment.mask,1));
  actual.push(checkpoint());
}
assert.deepEqual(actual,expected,'native/WASM snapshot or framebuffer diverged');
const before=checkpoint();
assert.equal(w.asoe_load(0),-1);
assert.deepEqual(checkpoint(),before,'invalid replacement length destroyed the valid session');
ptr=w.asoe_allocate_rom(rom.length);
assert.notEqual(ptr,0,error());
assert.equal(w.asoe_load(rom.length),-1);
assert.deepEqual(checkpoint(),before,'invalid replacement source destroyed the valid session');
for (const [mask,ticks] of [[255,1],[0,0],[0,5],[0xffffffff,0xffffffff]]) assert.equal(w.asoe_step(mask,ticks),-1);
assert.deepEqual(checkpoint(),before,'rejected input changed state/pixels');
ok(w.asoe_reset());
assert.equal(snapshot().frame,0);assert.equal(snapshot().rings,0);assert.equal(snapshot().resets,before.snapshot.resets+1);

// Batching must preserve every intermediate tick and ring/pose state.
const batched = await WebAssembly.instantiate(await readFile(wasmPath), {});
const b=batched.instance.exports;
ptr=b.asoe_allocate_rom(rom.length);
new Uint8Array(b.memory.buffer,ptr,rom.length).set(rom);
assert.equal(b.asoe_load(rom.length),0);
for(const segment of plan) for(let left=segment.ticks;left>0;) { const count=Math.min(4,left);assert.equal(b.asoe_step(segment.mask,count),0);left-=count; }
const statePtr=b.asoe_snapshot_ptr();
const state=JSON.parse(new TextDecoder().decode(new Uint8Array(b.memory.buffer,statePtr,b.asoe_snapshot_len())));
assert.deepEqual(state,before.snapshot);
assert.equal(createHash('sha256').update(new Uint8Array(b.memory.buffer,b.asoe_frame_ptr(),b.asoe_frame_len())).digest('hex'),before.rgbaSha256);
console.log(JSON.stringify({ result:'PASS',checkpoints:actual.length,ticks:plan.reduce((n,s)=>n+s.ticks,0),wasmSha256:createHash('sha256').update(await readFile(wasmPath)).digest('hex'),checkpointsMetadata:actual },null,2));
