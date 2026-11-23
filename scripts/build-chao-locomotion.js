// Select the genuine native Chao Run119 clip; preserve its original hierarchy
// and every retained binary accessor byte, compacting only unused resources.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';

const sourceFile='.local/imports/activity-motions/chao-actions.glb';
const outputFile='.local/imports/chao-locomotion.glb';
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const source=await readFile(sourceFile);
assert.equal(source.readUInt32LE(0),0x46546c67);
const jsonSize=source.readUInt32LE(12),document=JSON.parse(source.subarray(20,20+jsonSize));
assert.equal(source.readUInt32LE(24+jsonSize),0x004e4942);
const binary=source.subarray(28+jsonSize);
const manifest=JSON.parse(await readFile('.local/extracted/activity-motions/manifest.json','utf8'));
const state=manifest.commonStates.find(row=>row.index===119);
assert.equal(state.nativeName,'alm_run');assert.equal(state.nativeFile,'common/alm_run.saanim');
const library=JSON.parse(await readFile('.local/imports/activity-motions/conversion-report.json','utf8')).libraries.find(row=>row.name==='chao-actions');
const evidence=library.actions.find(row=>row.name==='alm_run');assert.ok(evidence);
const nativeFile=`.local/extracted/activity-motions/${state.nativeFile}`;
assert.equal(digest(await readFile(nativeFile)),evidence.sourceSha256);
const sit=manifest.commonStates.find(row=>row.index===81);
assert.equal(sit.nativeName,'alm_sit_dowwwn');
const selected=[state,sit].map(row=>document.animations.find(a=>a.name===row.nativeName));
assert.ok(selected.every(Boolean));assert.equal(selected[0].channels.length,120);
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
retained.asset.extras={...retained.asset.extras,chaoLocomotion:{nativeState:119,nativeName:state.nativeName,nativeMotionLabel:state.nativeMotionLabel,nativeFile,sourceSha256:evidence.sourceSha256,tableProperties:state.tableProperties,referenceFps:60,seatedPilot:{state:81,name:sit.nativeName,endFrame:Number(sit.tableProperties.EndFrame)},note:'Genuine Chao Run and sit-down; original hierarchy/rest/geometry/channel bytes preserved. Runtime native-to-Chaos retargeting remains separate.'}};
let encoded=Buffer.from(JSON.stringify(retained));encoded=Buffer.concat([encoded,Buffer.alloc((-encoded.length)&3,0x20)]);
let compact=Buffer.concat(parts);compact=Buffer.concat([compact,Buffer.alloc((-compact.length)&3)]);
const header=Buffer.alloc(20);header.writeUInt32LE(0x46546c67,0);header.writeUInt32LE(2,4);header.writeUInt32LE(28+encoded.length+compact.length,8);header.writeUInt32LE(encoded.length,12);header.writeUInt32LE(0x4e4f534a,16);
const binHeader=Buffer.alloc(8);binHeader.writeUInt32LE(compact.length,0);binHeader.writeUInt32LE(0x004e4942,4);
const output=Buffer.concat([header,encoded,binHeader,compact]);
// Verify unchanged hierarchy/skin references and all retained native bytes.
assert.deepEqual(retained.nodes,document.nodes);assert.deepEqual(retained.scenes,document.scenes);
for(const[old,index]of viewMap){const from=document.bufferViews[old],to=retained.bufferViews[index];assert.ok(binary.subarray(from.byteOffset||0,(from.byteOffset||0)+from.byteLength).equals(compact.subarray(to.byteOffset,to.byteOffset+to.byteLength)));}
await writeFile(outputFile,output);
const report={version:1,passed:true,source:sourceFile,sourceSha256:digest(source),output:outputFile,sha256:digest(output),bytes:output.length,sourceBytes:source.length,nativeFile,nativeSha256:evidence.sourceSha256,nativeState:119,clip:'alm_run',channels:120,nativeNodeCount:40,gltfNodes:retained.nodes.length,meshes:retained.meshes.length,retainedAccessors:accessors.length,retainedBufferViews:views.length,retainedBinaryBytes:length,allRetainedBinaryViewsExact:true,hierarchyRestAndGeometryPreserved:true,tableProperties:state.tableProperties,referenceFps:60};
await writeFile('.local/imports/chao-locomotion-proof.json',`${JSON.stringify(report,null,2)}\n`);
console.log(JSON.stringify(report,null,2));
