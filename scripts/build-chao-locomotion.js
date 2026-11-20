// Select genuine native Chao Run119 and jump/recovery132–137; preserve their hierarchy
// and every retained binary accessor byte, compacting only unused resources.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const sourceFile='.local/imports/activity-motions/chao-actions.glb';
const outputFile='.local/imports/chao-locomotion.glb';
const assetRoot=resolve(process.env.CHAO_ASSET_ROOT || '.');
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const source=await readFile(resolve(assetRoot,sourceFile));
assert.equal(source.readUInt32LE(0),0x46546c67);
const jsonSize=source.readUInt32LE(12),document=JSON.parse(source.subarray(20,20+jsonSize));
assert.equal(source.readUInt32LE(24+jsonSize),0x004e4942);
const binary=source.subarray(28+jsonSize);
const manifest=JSON.parse(await readFile(resolve(assetRoot,'.local/extracted/activity-motions/manifest.json'),'utf8'));
const library=JSON.parse(await readFile(resolve(assetRoot,'.local/imports/activity-motions/conversion-report.json'),'utf8')).libraries.find(row=>row.name==='chao-actions');
const clips=[];
for(const[index,name]of[[119,'alm_run'],[132,'alm_jump_up'],[135,'alm_jump_down']]){
  const state=manifest.commonStates.find(row=>row.index===index);
  assert.equal(state.nativeName,name);assert.equal(state.nativeFile,`common/${name}.saanim`);
  const evidence=library.actions.find(row=>row.name===name);assert.ok(evidence);
  const nativeFile=`.local/extracted/activity-motions/${state.nativeFile}`;
  assert.equal(digest(await readFile(resolve(assetRoot,nativeFile))),evidence.sourceSha256);
  clips.push({nativeState:index,nativeName:name,nativeMotionLabel:state.nativeMotionLabel,nativeFile,sourceSha256:evidence.sourceSha256,frameRange:evidence.frameRange,
    states:manifest.commonStates.filter(row=>row.nativeFile===state.nativeFile).map(row=>({index:row.index,tableProperties:row.tableProperties}))});
}
const selected=clips.map(clip=>document.animations.find(animation=>animation.name===clip.nativeName));
assert.ok(selected.every(Boolean));for(const animation of selected)assert.equal(animation.channels.length,120);
const retained=structuredClone(document);retained.animations=structuredClone(selected);
const accessorIds=new Set();
for(const mesh of retained.meshes)for(const primitive of mesh.primitives){
  for(const accessor of Object.values(primitive.attributes))accessorIds.add(accessor);
  if(primitive.indices!==undefined)accessorIds.add(primitive.indices);
  for(const target of primitive.targets||[])for(const accessor of Object.values(target))accessorIds.add(accessor);
  assert.ok(!primitive.extensions,'Unexpected compressed/extension geometry requires explicit remapping.');
}
for(const skin of retained.skins||[])if(skin.inverseBindMatrices!==undefined)accessorIds.add(skin.inverseBindMatrices);
for(const animation of retained.animations)for(const sampler of animation.samplers){accessorIds.add(sampler.input);accessorIds.add(sampler.output);}
const accessors=[...accessorIds].sort((a,b)=>a-b),accessorMap=new Map(accessors.map((old,index)=>[old,index]));
const viewIds=new Set();
for(const old of accessors){const accessor=retained.accessors[old];if(accessor.bufferView!==undefined)viewIds.add(accessor.bufferView);if(accessor.sparse){viewIds.add(accessor.sparse.indices.bufferView);viewIds.add(accessor.sparse.values.bufferView);}}
for(const image of retained.images||[])if(image.bufferView!==undefined)viewIds.add(image.bufferView);
const views=[...viewIds].sort((a,b)=>a-b),viewMap=new Map(views.map((old,index)=>[old,index]));
const parts=[];let length=0;
retained.bufferViews=views.map(old=>{const view=structuredClone(document.bufferViews[old]);assert.equal(view.buffer,0);const padding=(-length)&3;if(padding){parts.push(Buffer.alloc(padding));length+=padding;}const start=view.byteOffset||0,bytes=binary.subarray(start,start+view.byteLength);assert.equal(bytes.length,view.byteLength);parts.push(bytes);view.byteOffset=length;length+=bytes.length;return view;});
retained.accessors=accessors.map(old=>{const accessor=structuredClone(document.accessors[old]);if(accessor.bufferView!==undefined)accessor.bufferView=viewMap.get(accessor.bufferView);if(accessor.sparse){accessor.sparse.indices.bufferView=viewMap.get(accessor.sparse.indices.bufferView);accessor.sparse.values.bufferView=viewMap.get(accessor.sparse.values.bufferView);}return accessor;});
for(const mesh of retained.meshes)for(const primitive of mesh.primitives){for(const key of Object.keys(primitive.attributes))primitive.attributes[key]=accessorMap.get(primitive.attributes[key]);if(primitive.indices!==undefined)primitive.indices=accessorMap.get(primitive.indices);for(const target of primitive.targets||[])for(const key of Object.keys(target))target[key]=accessorMap.get(target[key]);}
for(const skin of retained.skins||[])if(skin.inverseBindMatrices!==undefined)skin.inverseBindMatrices=accessorMap.get(skin.inverseBindMatrices);
for(const animation of retained.animations)for(const sampler of animation.samplers){sampler.input=accessorMap.get(sampler.input);sampler.output=accessorMap.get(sampler.output);}
for(const image of retained.images||[])if(image.bufferView!==undefined)image.bufferView=viewMap.get(image.bufferView);
retained.buffers=[{byteLength:length}];
retained.asset.extras={...retained.asset.extras,chaoLocomotion:{version:2,clips,referenceFps:60,note:'Genuine Chao Run/jump/recovery; original hierarchy/rest/geometry/channel bytes preserved. State timing and native-to-Chaos retargeting remain separate.'}};
let encoded=Buffer.from(JSON.stringify(retained));encoded=Buffer.concat([encoded,Buffer.alloc((-encoded.length)&3,0x20)]);
let compact=Buffer.concat(parts);compact=Buffer.concat([compact,Buffer.alloc((-compact.length)&3)]);
const header=Buffer.alloc(20);header.writeUInt32LE(0x46546c67,0);header.writeUInt32LE(2,4);header.writeUInt32LE(28+encoded.length+compact.length,8);header.writeUInt32LE(encoded.length,12);header.writeUInt32LE(0x4e4f534a,16);
const binHeader=Buffer.alloc(8);binHeader.writeUInt32LE(compact.length,0);binHeader.writeUInt32LE(0x004e4942,4);
const output=Buffer.concat([header,encoded,binHeader,compact]);
// Verify unchanged hierarchy/skin references and all retained native bytes.
assert.deepEqual(retained.nodes,document.nodes);assert.deepEqual(retained.scenes,document.scenes);
for(const[old,index]of viewMap){const from=document.bufferViews[old],to=retained.bufferViews[index];assert.ok(binary.subarray(from.byteOffset||0,(from.byteOffset||0)+from.byteLength).equals(compact.subarray(to.byteOffset,to.byteOffset+to.byteLength)));}
await mkdir('.local/imports',{recursive:true});await writeFile(outputFile,output);
const report={version:2,passed:true,source:sourceFile,sourceSha256:digest(source),output:outputFile,sha256:digest(output),bytes:output.length,sourceBytes:source.length,clips,channels:selected.reduce((sum,animation)=>sum+animation.channels.length,0),nativeNodeCount:40,gltfNodes:retained.nodes.length,meshes:retained.meshes.length,retainedAccessors:accessors.length,retainedBufferViews:views.length,retainedBinaryBytes:length,allRetainedBinaryViewsExact:true,hierarchyRestAndGeometryPreserved:true,referenceFps:60};
await writeFile('.local/imports/chao-locomotion-proof.json',`${JSON.stringify(report,null,2)}\n`);
console.log(JSON.stringify(report,null,2));
