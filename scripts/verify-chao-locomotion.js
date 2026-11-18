import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';

const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
async function glb(file){const bytes=await readFile(file);assert.equal(bytes.readUInt32LE(0),0x46546c67);assert.equal(bytes.readUInt32LE(8),bytes.length);const size=bytes.readUInt32LE(12);return{bytes,document:JSON.parse(bytes.subarray(20,20+size)),binary:bytes.subarray(28+size)};}
function accessorData(asset,index){
  const a=asset.document.accessors[index],v=asset.document.bufferViews[a.bufferView];assert.ok(!a.sparse);
  const sizes={5120:1,5121:1,5122:2,5123:2,5125:4,5126:4},counts={SCALAR:1,VEC2:2,VEC3:3,VEC4:4,MAT2:4,MAT3:9,MAT4:16};
  const element=sizes[a.componentType]*counts[a.type];assert.ok(element);
  const stride=v.byteStride||element,base=(v.byteOffset||0)+(a.byteOffset||0),parts=[];
  for(let i=0;i<a.count;i++)parts.push(asset.binary.subarray(base+i*stride,base+i*stride+element));
  return{descriptor:{type:a.type,count:a.count,componentType:a.componentType,normalized:a.normalized||false,min:a.min,max:a.max},bytes:Buffer.concat(parts)};
}
function equalAccessor(a,ai,b,bi){const x=accessorData(a,ai),y=accessorData(b,bi);assert.deepEqual(x.descriptor,y.descriptor);assert.ok(x.bytes.equals(y.bytes),'Accessor values changed.');}
const source=await glb('.local/imports/activity-motions/chao-actions.glb'),output=await glb('.local/imports/chao-locomotion.glb');
const original=source.document.animations.find(a=>a.name==='alm_run'),selected=output.document.animations[0];
assert.equal(output.document.animations.length,1);assert.equal(selected.name,'alm_run');
assert.deepEqual(output.document.nodes,source.document.nodes);assert.deepEqual(output.document.scenes,source.document.scenes);assert.deepEqual(output.document.materials,source.document.materials);assert.deepEqual(output.document.textures,source.document.textures);
assert.deepEqual(selected.channels,original.channels);
for(let i=0;i<original.samplers.length;i++){
  const a=original.samplers[i],b=selected.samplers[i];assert.equal(a.interpolation,b.interpolation);
  equalAccessor(source,a.input,output,b.input);equalAccessor(source,a.output,output,b.output);
}
let meshAccessors=0;
for(let m=0;m<source.document.meshes.length;m++)for(let p=0;p<source.document.meshes[m].primitives.length;p++){
  const a=source.document.meshes[m].primitives[p],b=output.document.meshes[m].primitives[p];assert.equal(a.material,b.material);assert.equal(a.mode,b.mode);assert.deepEqual(Object.keys(a.attributes),Object.keys(b.attributes));
  for(const key of Object.keys(a.attributes)){equalAccessor(source,a.attributes[key],output,b.attributes[key]);meshAccessors++;}
  if(a.indices!==undefined){equalAccessor(source,a.indices,output,b.indices);meshAccessors++;}
}
for(let i=0;i<source.document.skins.length;i++){const a=source.document.skins[i],b=output.document.skins[i];assert.deepEqual(a.joints,b.joints);assert.equal(a.skeleton,b.skeleton);equalAccessor(source,a.inverseBindMatrices,output,b.inverseBindMatrices);}
for(let i=0;i<source.document.images.length;i++){
  const a=source.document.bufferViews[source.document.images[i].bufferView],b=output.document.bufferViews[output.document.images[i].bufferView];
  assert.ok(source.binary.subarray(a.byteOffset||0,(a.byteOffset||0)+a.byteLength).equals(output.binary.subarray(b.byteOffset||0,(b.byteOffset||0)+b.byteLength)));
}
const state=JSON.parse(await readFile('.local/extracted/activity-motions/manifest.json','utf8')).commonStates.find(row=>row.index===119);
assert.equal(state.nativeName,'alm_run');assert.equal(state.nativeMotionLabel,'animation_002D8310');
const provenance=output.document.asset.extras.chaoLocomotion;
assert.equal(provenance.nativeState,119);assert.equal(hash(await readFile(provenance.nativeFile)),provenance.sourceSha256);assert.deepEqual(provenance.tableProperties,state.tableProperties);
const animatedNodes=new Set(selected.channels.map(channel=>channel.target.node));assert.equal(animatedNodes.size,40);assert.equal(selected.channels.length,120);
const report={version:1,passed:true,sourceSha256:hash(source.bytes),outputSha256:hash(output.bytes),bytes:output.bytes.length,nativeState:119,clip:'alm_run',nativeMotionLabel:state.nativeMotionLabel,animatedNativeNodes:animatedNodes.size,channels:selected.channels.length,animationSamplersPreserved:original.samplers.length,meshAccessorsPreserved:meshAccessors,imagesPreserved:output.document.images.length,gltfNodes:output.document.nodes.length,sourceHierarchyRestAndGeometryPreserved:true,allRetainedAnimationAccessorBytesExact:true};
await writeFile('.local/imports/chao-locomotion-verification.json',`${JSON.stringify(report,null,2)}\n`);
console.log(JSON.stringify(report,null,2));
