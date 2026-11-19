"""Execute actual A2NE Tiny reward/return Thumb and exact native save C.

Requires local Unicorn2.1.3 for test-only CPU execution. Source/payload/vectors
remain under ignored .local. Rust runtime has no emulator dependency.
"""
import argparse
import hashlib
import json
import pathlib
import re
import struct
import subprocess
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR
from unicorn.arm_const import UC_ARM_REG_LR,UC_ARM_REG_PC,UC_ARM_REG_SP,UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R3

PAYLOAD_SHA256="84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca"
PIN="2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES={"chao_garden_src_save.c":"cf321666f783207195fb42299473fa920aacd9cc","chao_garden_include_save.h":"14833d791e1846d56db545b21ece0dae36fbf8f6","chao_garden_asm_code.s":"7ec760b4772bbab9d4e25282324f248728521c07"}

def next_word(seed):
    seed^=seed<<13&0xffffffff;seed^=seed>>17;return(seed^(seed<<5))&0xffffffff

class Cpu:
    def __init__(self,payload):
        self.cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB);self.cpu.mem_map(0x02000000,0x40000);self.cpu.mem_write(0x02000000,payload);self.cpu.mem_map(0x03000000,0x8000);self.cpu.mem_map(0x04000000,0x1000)
        self.calls=[];self.ready=0;self.cpu.hook_add(UC_HOOK_CODE,self.hook);self.cpu.hook_add(UC_HOOK_INTR,self.divide)
    def divide(self,cpu,number,_):
        pc=cpu.reg_read(UC_ARM_REG_PC)
        if number!=2 or bytes(cpu.mem_read(pc-2,2))!=b"\x06\xdf":raise ValueError("Unexpected native interrupt")
        numerator=cpu.reg_read(UC_ARM_REG_R0);denominator=cpu.reg_read(UC_ARM_REG_R1)
        if denominator!=30 or numerator>32767:raise ValueError("Garden initializer left bounded native Mod domain")
        quotient,remainder=divmod(numerator,denominator)
        cpu.reg_write(UC_ARM_REG_R0,quotient);cpu.reg_write(UC_ARM_REG_R1,remainder);cpu.reg_write(UC_ARM_REG_R3,quotient)
    def hook(self,cpu,address,size,_):
        if address in [0x020016d8,0x020012e0,0x0200f6b8,0x02000e28,0x0200c20c,0x02009df4]:
            self.calls.append(address)
            if address==0x02009df4:cpu.reg_write(UC_ARM_REG_R0,self.ready)
            cpu.reg_write(UC_ARM_REG_PC,cpu.reg_read(UC_ARM_REG_LR))
    def put32(self,address,value):self.cpu.mem_write(address,struct.pack("<I",value&0xffffffff))
    def u32(self,address):return struct.unpack("<I",self.cpu.mem_read(address,4))[0]
    def run(self,start,end=0x0203fff0):
        self.calls=[];self.cpu.reg_write(UC_ARM_REG_SP,0x03007000);self.cpu.reg_write(UC_ARM_REG_LR,0x0203fff1);self.cpu.emu_start(start|1,end,count=1000)
        if self.cpu.reg_read(UC_ARM_REG_PC)!=end:raise ValueError("Native reward routine exceeded instruction bound")

def rewards(payload,output):
    cpu=Cpu(payload);vectors=[];seed=0xa26a0001
    for row in range(4096):
        seed=next_word(seed);cpu.put32(0x030013a8,seed);cpu.run(0x02008810)
        vectors.extend([seed,cpu.cpu.reg_read(UC_ARM_REG_R0),cpu.u32(0x030013a8)])
    (output/"initializer.bin").write_bytes(struct.pack(f"<{len(vectors)}I",*vectors));vectors=[];seed=0xa26a0001
    edges=[0,1,127,99998,99999,100000,0xfffffff0,0xffffffff]
    for row in range(4096):
        seed=next_word(seed);rings=edges[row]if row<len(edges)else seed
        seed=next_word(seed);reward=row if row<256 else seed&255
        cpu.put32(0x03005274,rings);cpu.cpu.mem_write(0x03001970,bytes([reward]));cpu.run(0x0200a2dc)
        vectors.extend([rings,reward,cpu.u32(0x03005274)])
    (output/"signed.bin").write_bytes(struct.pack(f"<{len(vectors)}I",*vectors))
    vectors=[];seed=0xa26a0002;counter_edges=[0x8000,0xffff,0,14,15,16,32766,32767]
    for row in range(4096):
        seed=next_word(seed);rings=edges[row]if row<len(edges)else seed
        seed=next_word(seed);reward=seed&65535
        seed=next_word(seed);counter=counter_edges[row]if row<len(counter_edges)else seed&65535
        seed=next_word(seed);callback=0 if seed&1 else 0x02012345
        cpu.put32(0x03005274,rings);cpu.cpu.mem_write(0x03001ad4,struct.pack("<H",reward));cpu.put32(0x03003330,0x0200f621);cpu.put32(0x03003334,callback);cpu.cpu.mem_write(0x0300333c,struct.pack("<H",counter));cpu.run(0x0200f620)
        after=struct.unpack("<H",cpu.cpu.mem_read(0x0300333c,2))[0];brightness=struct.unpack("<H",cpu.cpu.mem_read(0x04000054,2))[0]
        vectors.extend([rings,reward,counter,callback,cpu.u32(0x03005274),after,brightness,cpu.u32(0x03003330),cpu.calls.count(0x02000e28),len(cpu.calls)])
    (output/"bounce.bin").write_bytes(struct.pack(f"<{len(vectors)}I",*vectors))
    vectors=[];seed=0xa26a0003
    for row in range(4096):
        r=[]
        for _ in range(5):seed=next_word(seed);r.append(seed)
        mode=row&1;active=r[0]&255;phase=r[1];main=r[2];rings=r[3];reward=r[4]&255;ready=(row>>1)&1
        active_at,phase_at,start,end=(0x030019d4,0x030019d8,0x0200bdb4,0x0200bdd2)if mode==0 else(0x03001ab2,0x03001ab4,0x0200d174,0x0200d196)
        cpu.put32(0x03005274,rings);cpu.cpu.mem_write(0x03001970,bytes([reward]));cpu.cpu.mem_write(active_at,bytes([active]));cpu.put32(phase_at,phase);cpu.put32(0x03003330,main);cpu.ready=ready;cpu.run(start,end)
        vectors.extend([mode,active,phase,main,rings,reward,ready,cpu.cpu.mem_read(active_at,1)[0],cpu.u32(phase_at),cpu.u32(0x03003330),cpu.u32(0x03005274),cpu.calls.count(0x02009df4)])
    (output/"memory-exit.bin").write_bytes(struct.pack(f"<{len(vectors)}I",*vectors))

PREAMBLE=r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef uint8_t u8;typedef int8_t s8;typedef uint16_t u16;typedef int16_t s16;typedef uint32_t u32;typedef int32_t s32;
static u8 flash[65536];static u32 seedValue,initialWord;static u8 failures;static u32 attempts[5],attemptCount;
u32 gIntrTable[4];struct{u32 unk0;}gUnknown_02000008;u32 gUnknown_03005274,gSaveVersion;u8 gSaveSectorNum;
static void IdentifyFlash(void){}static void SetFlashTimerIntr(u32 i,void*p){(void)i;(void)p;}
static void ReadFlash(u32 sector,u32 offset,void*dst,u32 bytes){if(sector>=16||offset+bytes>4096)exit(7);memcpy(dst,flash+sector*4096+offset,bytes);}
static s16 sub_0200185c(void*data,s32 size){u8*p=data;u16 crc=0xffff;for(s32 i=0;i<size;i++){crc^=p[i];for(int b=0;b<8;b++)crc=crc&1?(crc>>1)^0x8408:crc>>1;}return~crc;}
static u32 sub_02008810(void){return initialWord;}static u32 sub_0200151c(void){return seedValue;}static void sub_02001528(u32 v){seedValue=v;}
static void m4aMPlayAllStop(void){}static void m4aSoundMain(void){}static void VBlankIntrWait(void){}static void m4aSoundVSyncOff(void){}static void m4aSoundVSyncOn(void){}
static u32 ProgramFlashSectorAndVerifyNBytes(u32 sector,void*data,u32 size){if(sector<10||sector>=16||size!=0x858||attemptCount>=5)exit(8);attempts[attemptCount++]=sector;if(failures&(1<<(sector-10)))return 1;memset(flash+sector*4096,255,4096);memcpy(flash+sector*4096,data,size);return 0;}
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
'''
DRIVER=r'''
_Static_assert(sizeof(struct TCG_SaveSectorData)==0x858,"Native garden struct mismatch");
int main(int argc,char**argv){if(argc!=2)return 2;FILE*f=fopen(argv[1],"wb");if(!f)return 3;u32 seed=0xa26a0004;
 for(u32 scenario=0;scenario<64;scenario++){
  for(u32 i=0;i<sizeof(flash);i++){seed=next(seed);flash[i]=seed>>24;}
  for(u32 sector=0;sector<6;sector++){
   struct TCG_SaveSectorData*r=(void*)(flash+(sector+10)*4096);r->header.unk2=0x1965;
   if(scenario%8==1)r->header.unk4=37;if(scenario%8==2)r->header.unk4=sector==0?0xffffffff:1;
   r->header.checksum=sub_0200185c(&r->header.unk2,0x856);
   if(scenario%8==0||sector>scenario%6)r->header.unk2=0;
   if(scenario%8==3&&sector==scenario%6)r->header.checksum^=1;
  }
  seed=next(seed);u32 config=seed;seed=next(seed);initialWord=seed;seed=next(seed);u32 initialSeed=seed;
  seed=next(seed);u32 newRings=scenario%4==0?99998:scenario%4==1?0xfffffff0:seed;seed=next(seed);u32 newSeed=seed;failures=scenario;
  u32 header[]={config,initialWord,initialSeed,newRings,newSeed,failures,scenario,0};if(fwrite(header,sizeof(header),1,f)!=1||fwrite(flash,sizeof(flash),1,f)!=1)return 4;
  gUnknown_02000008.unk0=config;seedValue=initialSeed;LoadGameState();u32 load[]={gSaveSectorNum,gSaveVersion,gUnknown_03005274,seedValue};
  if(fwrite(load,sizeof(load),1,f)!=1||fwrite(&gSaveGameState,sizeof(gSaveGameState),1,f)!=1)return 4;
  gUnknown_03005274=newRings;seedValue=newSeed;attemptCount=0;for(int i=0;i<5;i++)attempts[i]=255;SaveGameState();
  u32 success=!(failures&(1<<(gSaveSectorNum)));u32 write[]={gSaveSectorNum,gSaveVersion,gUnknown_03005274,seedValue,attemptCount,success,attempts[0],attempts[1],attempts[2],attempts[3],attempts[4]};
  if(fwrite(write,sizeof(write),1,f)!=1||fwrite(&gSaveGameState,sizeof(gSaveGameState),1,f)!=1||fwrite(flash,sizeof(flash),1,f)!=1)return 4;
 }if(fclose(f))return 5;return 0;}
'''

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ["payload","reference-dir","output-dir","summary","zig"]:parser.add_argument("--"+name,type=pathlib.Path,required=True)
    a=parser.parse_args()
    if ".local"not in a.output_dir.resolve().parts:parser.error("Original inputs and vectors must remain under.local")
    payload=a.payload.read_bytes()
    if hashlib.sha256(payload).hexdigest()!=PAYLOAD_SHA256:raise ValueError("Wrong A2NE embedded garden payload")
    sources={}
    for name,expected in SOURCES.items():
        raw=(a.reference_dir/name).read_bytes()
        if hashlib.sha1(f"blob {len(raw)}\0".encode()+raw).hexdigest()!=expected:raise ValueError(f"Wrong exact source{name}")
        sources[name]=raw.decode()
    a.output_dir.mkdir(parents=True,exist_ok=True);rewards(payload,a.output_dir)
    strip=lambda text:re.sub(r"(?m)^#include[^\n]*","",text)
    generated="\n".join([PREAMBLE,strip(sources["chao_garden_include_save.h"]),"struct TCG_SaveSectorData gSaveGameState;",strip(sources["chao_garden_src_save.c"]),DRIVER])
    c=a.output_dir/"garden-save.c";exe=a.output_dir/"garden-save.exe";out=a.output_dir/"save.bin";c.write_text(generated)
    subprocess.run([str(a.zig),"cc","-std=gnu11","-O1","-fwrapv","-fno-strict-aliasing",str(c),"-o",str(exe)],check=True);subprocess.run([str(exe),str(out)],check=True)
    outputs={}
    for name,cases,size in [("initializer",4096,12),("signed",4096,12),("bounce",4096,40),("memory-exit",4096,48),("save",64,135436)]:
        raw=(a.output_dir/f"{name}.bin").read_bytes()
        if len(raw)!=cases*size:raise ValueError(f"Incomplete native output{name}")
        outputs[name]=dict(cases=cases,rowBytes=size,bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
    routines={name:dict(offset=start,bytes=end-start,sha256=hashlib.sha256(payload[start:end]).hexdigest())for name,start,end in [("initializer",0x8810,0x882c),("bounded-random",0x1554,0x1588),("signed-credit",0xa2dc,0xa304),("memory-return-first",0xbdb4,0xbdd2),("memory-return-second",0xd174,0xd196),("bounce-return",0xf620,0xf684)]}
    report=dict(schemaVersion=1,referenceCommit=PIN,sourceGitBlobs=SOURCES,payloadSha256=PAYLOAD_SHA256,routines=routines,outputs=outputs,boundary="Actual-payload Thumb initializer/reward/return execution; bounded BIOS signed-Div shim, graphics/results-ready/task-restore explicit service stubs. Exact original save C with named in-memory flash/random/audio bindings; no physical flash or real-save compatibility claim")
    a.summary.write_text(json.dumps(report,indent=2)+"\n");print(json.dumps(report))

if __name__=="__main__":main()
