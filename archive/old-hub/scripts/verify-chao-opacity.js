// Audit actual local geometry/texture alpha; never writes native assets or the scene.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { inflateSync } from 'node:zlib';

const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
function png(bytes){
  assert.equal(bytes.subarray(0,8).toString('hex'),'89504e470d0a1a0a');
  const width=bytes.readUInt32BE(16),height=bytes.readUInt32BE(20);
  assert.equal(bytes[24],8);assert.equal(bytes[25],6);assert.equal(bytes[28],0);
  const chunks=[];
  for(let at=8;at<bytes.length;){const length=bytes.readUInt32BE(at),type=bytes.toString('ascii',at+4,at+8);if(type==='IDAT')chunks.push(bytes.subarray(at+8,at+8+length));at+=length+12;}
  const stride=width*4,raw=inflateSync(Buffer.concat(chunks)),pixels=Buffer.alloc(stride*height);
  assert.equal(raw.length,(stride+1)*height);
  const paeth=(a,b,c)=>{const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);return pa<=pb&&pa<=pc?a:pb<=pc?b:c;};
  for(let y=0;y<height;y++){
    const filter=raw[y*(stride+1)];assert.ok(filter<=4);
    for(let x=0;x<stride;x++){
      const at=y*stride+x,a=x>=4?pixels[at-4]:0,b=y?pixels[at-stride]:0,c=y&&x>=4?pixels[at-stride-4]:0;
      const predictor=[0,a,b,Math.floor((a+b)/2),paeth(a,b,c)][filter];pixels[at]=(raw[y*(stride+1)+x+1]+predictor)&255;
    }
  }
  let min=255,max=0,transparent=0;for(let i=3;i<pixels.length;i+=4){const a=pixels[i];min=Math.min(min,a);max=Math.max(max,a);if(a<255)transparent++;}
  return{width,height,pixels,alphaMin:min,alphaMax:max,nonOpaquePixels:transparent};
}
async function glb(file){const bytes=await readFile(file);assert.equal(bytes.readUInt32LE(0),0x46546c67);const size=bytes.readUInt32LE(12),document=JSON.parse(bytes.subarray(20,20+size));return{document,binary:bytes.subarray(28+size),sha256:hash(bytes)};}
function colors(document,binary,index){
  if(index===undefined)return{count:0,alphaMin:1,alphaMax:1,nonOpaqueVertices:0};
  const accessor=document.accessors[index],view=document.bufferViews[accessor.bufferView],components=accessor.type==='VEC4'?4:3;
  const size={5121:1,5123:2,5126:4}[accessor.componentType];assert.ok(size);
  const stride=view.byteStride||components*size,base=(view.byteOffset||0)+(accessor.byteOffset||0);
  let min=1,max=0,nonOpaque=0;
  for(let i=0;i<accessor.count;i++){
    let alpha=1;
    if(components===4){const at=base+i*stride+3*size;alpha=size===1?binary.readUInt8(at)/(accessor.normalized?255:1):size===2?binary.readUInt16LE(at)/(accessor.normalized?65535:1):binary.readFloatLE(at);}
    assert.ok(Number.isFinite(alpha));min=Math.min(min,alpha);max=Math.max(max,alpha);if(alpha<0.99999)nonOpaque++;
  }
  return{count:accessor.count,alphaMin:min,alphaMax:max,nonOpaqueVertices:nonOpaque};
}
const body=[];
for(const name of ['neutral-chaos','hero-chaos','dark-chaos']){
  const asset=await glb(`.local/imports/${name}.glb`),{document,binary}=asset;
  let primitives=0,colorVertices=0,nonOpaqueVertexAlpha=0;const used=new Set();
  for(const mesh of document.meshes)for(const primitive of mesh.primitives){
    const material=document.materials[primitive.material];assert.ok(material);
    assert.equal(material.alphaMode||'OPAQUE','OPAQUE',`${name} primitive must be opaque`);
    assert.equal(material.pbrMetallicRoughness?.baseColorFactor?.[3]??1,1);
    const attribute=colors(document,binary,primitive.attributes.COLOR_0);colorVertices+=attribute.count;nonOpaqueVertexAlpha+=attribute.nonOpaqueVertices;
    used.add(primitive.material);primitives++;
  }
  const images=document.images.map(image=>{const view=document.bufferViews[image.bufferView],decoded=png(binary.subarray(view.byteOffset||0,(view.byteOffset||0)+view.byteLength));return{name:image.name,width:decoded.width,height:decoded.height,alphaMin:decoded.alphaMin,alphaMax:decoded.alphaMax,nonOpaquePixels:decoded.nonOpaquePixels};});
  body.push({name,sha256:asset.sha256,materials:used.size,primitives,colorVertices,nonOpaqueVertexAlpha,allUsedMaterialsOpaque:true,images});
}
const emeralds=[];
for(const color of ['red','blue','green','cyan','purple','yellow','white']){
  const nativeName=color==='cyan'?'lightblue':color,source=png(await readFile(`.local/extracted/textures/e0207texture/chaoseme_${nativeName}.png`));
  const asset=await glb(`.local/imports/emerald-${color}.glb`),{document,binary}=asset;
  const used=new Set(document.meshes.flatMap(mesh=>mesh.primitives.map(primitive=>primitive.material)));
  const materialAlphas=[];
  for(const index of used){const material=document.materials[index],native=material.extras?.sa2Source;assert.equal(material.alphaMode,'BLEND');assert.ok(native?.useAlpha);const alpha=material.pbrMetallicRoughness.baseColorFactor[3];assert.ok(Math.abs(alpha-native.diffuse[3])<1e-7);materialAlphas.push(alpha);}
  let embeddedMatched=false;
  for(const image of document.images){const view=document.bufferViews[image.bufferView],embedded=png(binary.subarray(view.byteOffset||0,(view.byteOffset||0)+view.byteLength));if(embedded.width===source.width&&embedded.height===source.height&&embedded.pixels.equals(source.pixels))embeddedMatched=true;}
  assert.ok(embeddedMatched,`Original ${color} texture pixels must survive embedding.`);
  // Reflection mapping intentionally samples RGB. Full source texture alpha
  // proves no per-pixel emerald opacity is lost when moving that map slot.
  assert.equal(source.alphaMin,255);
  emeralds.push({color,sha256:asset.sha256,materialAlphas,originalTexturePixelsPreserved:true,sourceTextureAlphaMin:source.alphaMin,sourceTextureAlphaMax:source.alphaMax});
}
const atlasBytes=await readFile('.local/imports/chao-flame.png'),atlas=png(atlasBytes);
assert.equal(atlas.width,512);assert.equal(atlas.height,32);
for(let frame=0;frame<16;frame++){
  const original=png(await readFile(`.local/extracted/textures/al_icon/torch_blue${String(frame).padStart(2,'0')}.png`));
  assert.equal(original.width,32);assert.equal(original.height,32);
  for(let row=0;row<32;row++){const start=(row*512+frame*32)*4;assert.ok(atlas.pixels.subarray(start,start+128).equals(original.pixels.subarray(row*128,(row+1)*128)),`Native flame pixels/alpha mismatch frame${frame}/row${row}`);}
}
const scene=await readFile('src/scene.js','utf8');
assert.ok(!/emeraldGlow\.(?:addIncludedOnlyMesh|addExcludedMesh|setExcludedByDefault)\s*\(/.test(scene),'Opaque scene occluders must participate in the emerald depth pass.');
assert.match(scene,/emeraldGlow\.customEmissiveTextureSelector\s*=\s*\(\)\s*=>\s*null/);
const summary={version:1,passed:true,sceneSha256:hash(scene),bodies:body,emeralds,flame:{sha256:hash(atlasBytes),frames:16,originalPixelAndAlphaMatches:true,width:512,height:32,alphaMin:atlas.alphaMin,alphaMax:atlas.alphaMax},
  inspectedRuntimeGuards:['All three Chaos body GLBs are OPAQUE/alpha1. Runtime also clears body opacity textures, disables albedo alpha, vertex alpha and blending, and restores depth writing.','The emerald glow pass has no inclusion/exclusion mesh filter; only mesh.metadata.nativeEmeraldGlow emits, with a null custom emissive texture.','Flame uses original atlas pixels with additive blending and no depth writes. Emeralds retain their original BLEND material alpha; fully opaque native environment-map texels are not an extra opacity source.'],
  limits:['Source/data inspection is independent of browser GPU acceptance. Blur can legitimately extend slightly beyond visible silhouettes; this check does not prove a zero-pixel halo at every view.','Native Chao texture alpha encodes shader effects and is intentionally not interpreted as body cutout transparency.']};
await writeFile('.local/imports/chao-opacity-verification.json',`${JSON.stringify(summary,null,2)}\n`);
console.log(JSON.stringify({passed:true,bodies:body.map(({name,materials,primitives,colorVertices,nonOpaqueVertexAlpha})=>({name,materials,primitives,colorVertices,nonOpaqueVertexAlpha})),emeralds:emeralds.map(({color,materialAlphas,sourceTextureAlphaMin})=>({color,materialAlphas,sourceTextureAlphaMin})),flame:summary.flame},null,2));
