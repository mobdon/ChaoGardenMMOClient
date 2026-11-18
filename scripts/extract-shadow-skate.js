import { copyFile, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const run = promisify(execFile);
const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed Sonic Adventure 2 required.');
const tools = path.resolve(process.env.SA_TOOLS_DIR || '.local/tools/sa-tools-786');
const output = path.resolve('.local/extracted/shadow-skate');
await mkdir(output, { recursive:true });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sources = [];
for (const [archive, utility] of [['teriosmtn','splitMTN'],['plcommtn','splitMTN'],['teriosmdl','splitMDL']]) {
  const relativePath = `resource/gd_PC/${archive}.prs`, original = path.join(game.gamePath,relativePath);
  const bytes = await readFile(original), copy = path.join(output,`${archive}.prs`);
  await copyFile(original,copy);
  const result = await run('dotnet',[path.join(tools,`bin/lib/${utility}.dll`),copy],{windowsHide:true,timeout:120000,maxBuffer:8*1024*1024,env:{...process.env,DOTNET_ROLL_FORWARD:'Major'}});
  await writeFile(path.join(output,`${archive}.log`),result.stdout+result.stderr);
  if (hash(await readFile(original))!==hash(bytes)) throw new Error('Original archive changed.');
  sources.push({relativePath,sha256:hash(bytes),bytes:bytes.length,unchanged:true});
}
const mappings = await readFile(path.join(tools,'GameConfig/PC_SA2/MDL/TERIOSMTN.ini'),'utf8');
const selected = [18,27,169,170,171,172,173,174,175];
const clips = [];
for (const id of selected) {
  const line=mappings.split(/\r?\n/).find(line=>line.startsWith(`${id}=`));
  if (!line) throw new Error(`Missing official motion mapping:${id}`);
  const [mappedFile,description,nativeName]=line.slice(line.indexOf('=')+1).split('|');
  const nativeFile=`.local/extracted/shadow-skate/${mappedFile.replace('figure/bin/','')}`, bytes=await readFile(nativeFile);
  clips.push({id,description,nativeName,nativeFile,sha256:hash(bytes),bytes:bytes.length,role:id>=172&&id<=174?'skate-cycle':id===175||id===27?'turn':id===18?'brake':'reference'});
}
const exe=await readFile(path.join(game.gamePath,'sonic2app.exe'));
sources.push({relativePath:'sonic2app.exe',sha256:hash(exe),bytes:exe.length,unchanged:true});
const pe=exe.readUInt32LE(60),optional=pe+24,base=exe.readUInt32LE(optional+28),sections=optional+exe.readUInt16LE(pe+20);
function atVA(va,length){for(let i=0;i<exe.readUInt16LE(pe+6);i++){const at=sections+i*40,rva=exe.readUInt32LE(at+12),size=exe.readUInt32LE(at+16);if(va-base>=rva&&va-base+length<=rva+size)return exe.readUInt32LE(at+20)+va-base-rva;}throw new Error(`Unmapped native address:${va.toString(16)}`);}
const animationStates=Array.from({length:249},(_,index)=>{const at=atVA(0x00a188c8+index*16,16);return{index,animNum:exe.readInt16LE(at),modelNum:exe.readInt16LE(at+2),motionMode:exe.readInt16LE(at+4),nextAnimation:exe.readInt16LE(at+6),transitionSpeed:exe.readFloatLE(at+8),animationSpeed:exe.readFloatLE(at+12)};});
for(const clip of clips)clip.nativeStates=animationStates.filter(state=>state.animNum===clip.id);
const nativeModel='.local/extracted/shadow-skate/teriosmdl/65.sa2mdl';
const manifest={version:1,sources,clips,nativeModel,nativeModelSha256:hash(await readFile(nativeModel)),nativeAnimationTable:{address:'0x00a188c8',count:249,stride:16,states:animationStates},
  evidence:['Official SA Tools786 PC_SA2/MDL/TERIOSMTN.ini maps172/173/174 to genuine Shadow Skate Jog/Run/Sprint;175 is Turn Around,18 Common Brake.','X-Hax SA2Variables.h ShadowAnimList249 entries and SA2Structs.h AnimationInfo layout.'],
  limits:['Native gait resource selection and playback metadata; Chao retargeting is an adaptation.','Motion mode/state selection and movement speed thresholds are not reverse-engineered here.','No Chao rig, shape, original file or live scene is modified.']};
await writeFile(path.join(output,'manifest.json'),`${JSON.stringify(manifest,null,2)}\n`);
console.log(JSON.stringify({originalsUnchanged:sources.length,modelBytes:(await stat(nativeModel)).size,clips:clips.map(({id,description,bytes,nativeStates})=>({id,description,bytes,nativeStates}))},null,2));
