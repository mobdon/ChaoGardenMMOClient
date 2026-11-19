// Independent hash-only oracle for raw 4bpp indices and BGR555 representations.
// No proprietary input or decoded pixels are embedded or written by this tool.
import { readFileSync, openSync, readSync, closeSync } from 'node:fs';
import { createHash } from 'node:crypto';
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
if(process.argv.length!==3)throw Error('usage: node tools/reference-parity.mjs LOCAL_ASOE_ROM');
const file=openSync(process.argv[2],'r');
const bounded=Buffer.alloc(8388609);let read=0;
try { while(read<bounded.length) { const n=readSync(file,bounded,read,bounded.length-read,null);if(n===0)break;read+=n; } } finally { closeSync(file); }
const rom=bounded.subarray(0,read);
if(rom.length!==8388608 || sha256(rom)!=='afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020')throw Error('unsupported source');
const catalog=JSON.parse(readFileSync(new URL('../catalog.json',import.meta.url)));
const outputs=[];
for(const record of catalog) {
 if(record.addressSpace!=='rom')continue;
 const source=rom.subarray(record.offset,record.offset+record.length);
 if(sha256(source)!==record.sha256)throw Error('source span mismatch');
 let codec,output;
 if(record.id.includes('.ui.') && record.id.endsWith('.tiles')) {
  codec='4bpp-indices'; output=Buffer.alloc(source.length*2);
  for(let i=0;i<source.length;i++){output[i*2]=source[i]%16;output[i*2+1]=Math.floor(source[i]/16);}
 } else if(record.domain==='palettes') {
  if(source.length%2)throw Error('partial palette word');
  codec='bgr555-rgba-opaque';output=Buffer.alloc(source.length*2);
  for(let i=0;i<source.length/2;i++) {
   const word=source.readUInt16LE(i*2);
   for(let c=0;c<3;c++){const v=Math.floor(word/(2**(c*5)))%32;output[i*4+c]=v*8+Math.floor(v/4);}
   output[i*4+3]=255;
  }
 } else continue;
 outputs.push({id:record.id,codec,bytes:output.length,sha256:sha256(output)});
}
console.log(JSON.stringify({sourceSha256:sha256(rom),policy:'Tile-major 4bpp indices; BGR555 bit replication, opaque palette entries. Bounded candidate spans do not establish original array lengths.',outputs},null,2));
