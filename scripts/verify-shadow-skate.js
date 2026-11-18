import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed Sonic Adventure 2 required.');
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const manifest = JSON.parse(await readFile('.local/extracted/shadow-skate/manifest.json','utf8'));
const report = JSON.parse(await readFile('.local/imports/shadow-skate-conversion-report.json','utf8'));
const bytes = await readFile('.local/imports/shadow-skate-motion.json');
const document = JSON.parse(bytes);
assert.equal(document.version,1);
assert.equal(document.fps,60);
assert.equal(document.quaternionOrder,'xyzw');
assert.equal(document.basis,'right-handed glTF Y-up model space');
assert.equal(document.sourceBones.length,62);
assert.equal(document.bones.length,16);
assert.equal(document.clips.length,9);
assert.equal(report.bytes,bytes.length);
assert.equal(report.sha256,digest(bytes));
assert.equal(report.rigUnchanged,true);
assert.equal(report.chaoRigLoadedOrModified,false);
assert.equal(digest(await readFile(manifest.nativeModel)),manifest.nativeModelSha256);
assert.equal(document.sourceModelSha256,manifest.nativeModelSha256);
const sourceBones = new Map(document.sourceBones.map(bone=>[bone.nodeId,bone]));
assert.equal(sourceBones.size,62);
function vector(values,length){assert.equal(values.length,length);assert.ok(values.every(Number.isFinite));}
function quaternion(values){vector(values,4);assert.ok(Math.abs(Math.hypot(...values)-1)<1e-6,'Quaternion must be unit length.');}
for (const bone of document.sourceBones) {
  quaternion(bone.restWorld);vector(bone.restPosition,3);
  const seen=new Set([bone.nodeId]);let at=bone;
  while(at.parentNodeId!==null){assert.ok(sourceBones.has(at.parentNodeId));assert.ok(!seen.has(at.parentNodeId));seen.add(at.parentNodeId);at=sourceBones.get(at.parentNodeId);}
}
quaternion(document.alignmentQuaternion);
let sampledFrames=0,rotations=0;
for (const clip of document.clips) {
  const source=manifest.clips.find(item=>item.id===clip.nativeId);
  assert.ok(source);
  assert.equal(digest(await readFile(source.nativeFile)),source.sha256);
  assert.equal(clip.sourceSha256,source.sha256);
  assert.deepEqual(clip.nativeStates,source.nativeStates);
  assert.equal(clip.frames.length,clip.frameCount);
  assert.equal(clip.frameCount,clip.sourceFrameEnd-clip.sourceFrameStart+1);
  assert.ok(Math.abs(clip.durationSeconds-clip.frameCount/60)<1e-9);
  sampledFrames+=clip.frameCount;
  for(const[index,frame]of clip.frames.entries()){
    assert.ok(Math.abs(frame.timeSeconds-index/60)<1e-8);
    assert.equal(Object.keys(frame.rotationsByNode).length,62);
    assert.equal(Object.keys(frame.positionsByNode).length,62);
    assert.equal(Object.keys(frame.rotations).length,16);
    for(const bone of document.sourceBones){
      const rotation=frame.rotationsByNode[bone.nodeId];quaternion(rotation);vector(frame.positionsByNode[bone.nodeId],3);rotations++;
      if(index>0){const previous=clip.frames[index-1].rotationsByNode[bone.nodeId];assert.ok(rotation.reduce((sum,value,i)=>sum+value*previous[i],0)>=-1e-7,'Quaternion signs must be continuous.');}
    }
    for(const bone of document.bones){assert.deepEqual(frame.rotations[bone.semantic],frame.rotationsByNode[bone.nodeId]);assert.deepEqual(frame.positions[bone.semantic],frame.positionsByNode[bone.nodeId]);}
  }
  if([172,173,174].includes(clip.nativeId)){
    assert.ok(clip.frameCount>20);
    for(const side of ['leftFoot','rightFoot']){
      const first=clip.frames[0].positions[side];
      const excursion=Math.max(...clip.frames.map(frame=>Math.hypot(...frame.positions[side].map((value,i)=>value-first[i]))));
      assert.ok(excursion>0.1,`Actual native ${side} skating trajectory expected.`);
    }
  }
}
for(const source of manifest.sources){const original=await readFile(path.join(game.gamePath,source.relativePath));assert.equal(original.length,source.bytes);assert.equal(digest(original),source.sha256);}
const summary={version:1,passed:true,originalsUnchanged:manifest.sources.length,sourceBones:62,semanticBones:16,clips:document.clips.length,sampledFrames,unitQuaternionSamples:rotations,bytes:bytes.length,sha256:digest(bytes),rigUnchanged:true,chaoRigLoadedOrModified:false,
  nativeSkateClips:document.clips.filter(clip=>[172,173,174].includes(clip.nativeId)).map(({nativeId,name,frameCount})=>({nativeId,name,frameCount})),
  limits:['Source motion data validation only; Chao retarget amplitude, boot contact, blending, speed threshold and runtime cadence require browser acceptance.','Source positions must not be copied into the Chao rig; the runtime adaptation preserves its local positions and scales.']};
await writeFile('.local/imports/shadow-skate-verification.json',`${JSON.stringify(summary,null,2)}\n`);
console.log(JSON.stringify(summary,null,2));
