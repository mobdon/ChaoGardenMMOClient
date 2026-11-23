import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

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
const assetRoot=resolve(process.env.CHAO_ASSET_ROOT || '.');
const source=await glb(resolve(assetRoot,'.local/imports/activity-motions/chao-actions.glb')),output=await glb('.local/imports/chao-locomotion.glb');
assert.deepEqual(output.document.animations.map(animation=>animation.name),['alm_run','alm_jump_up','alm_jump_down']);
assert.deepEqual(output.document.nodes,source.document.nodes);assert.deepEqual(output.document.scenes,source.document.scenes);assert.deepEqual(output.document.materials,source.document.materials);assert.deepEqual(output.document.textures,source.document.textures);
let samplers=0;
for(const selected of output.document.animations){const original=source.document.animations.find(animation=>animation.name===selected.name);assert.ok(original);
assert.deepEqual(selected.channels,original.channels);
for(let i=0;i<original.samplers.length;i++){
  const a=original.samplers[i],b=selected.samplers[i];assert.equal(a.interpolation,b.interpolation);
  equalAccessor(source,a.input,output,b.input);equalAccessor(source,a.output,output,b.output);
}samplers+=original.samplers.length;}
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
const states=JSON.parse(await readFile(resolve(assetRoot,'.local/extracted/activity-motions/manifest.json'),'utf8')).commonStates;
const provenance=output.document.asset.extras.chaoLocomotion;
assert.equal(provenance.version,2);assert.equal(provenance.clips.length,3);
for(const clip of provenance.clips){const state=states.find(row=>row.index===clip.nativeState);assert.equal(clip.nativeName,state.nativeName);assert.equal(clip.nativeMotionLabel,state.nativeMotionLabel);
assert.equal(hash(await readFile(resolve(assetRoot,clip.nativeFile))),clip.sourceSha256);
assert.deepEqual(clip.states,states.filter(row=>row.nativeFile===state.nativeFile).map(row=>({index:row.index,tableProperties:row.tableProperties})));}
for(const selected of output.document.animations){const animatedNodes=new Set(selected.channels.map(channel=>channel.target.node));assert.equal(animatedNodes.size,40);assert.equal(selected.channels.length,120);}
const report={version:2,passed:true,sourceSha256:hash(source.bytes),outputSha256:hash(output.bytes),bytes:output.bytes.length,clips:provenance.clips.map(clip=>({nativeState:clip.nativeState,nativeName:clip.nativeName,nativeMotionLabel:clip.nativeMotionLabel})),animatedNativeNodes:40,channels:360,animationSamplersPreserved:samplers,meshAccessorsPreserved:meshAccessors,imagesPreserved:output.document.images.length,gltfNodes:output.document.nodes.length,sourceHierarchyRestAndGeometryPreserved:true,allRetainedAnimationAccessorBytesExact:true};
await writeFile('.local/imports/chao-locomotion-verification.json',`${JSON.stringify(report,null,2)}\n`);
console.log(JSON.stringify(report,null,2));
