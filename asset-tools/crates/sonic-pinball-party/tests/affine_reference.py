"""Native A3VE OBJ affine matrix writer/allocator, no hooks or BIOS seams.

Synthetic tables are independently seeded. Native-table cases publish only
requests and output hashes, keeping the original table bytes local.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import control_reference as control

BASE=0x03000e50
COUNT=0x0300005c
ROUTINES={"write":(0x236c,0x2514),"allocate":(0x2514,0x2560)}

def generate(path):
    oracle=control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    from unicorn import UC_HOOK_MEM_WRITE
    from unicorn.arm_const import UC_ARM_REG_R0
    writes=[]
    def record(uc,access,address,size,value,_):
        if BASE<=address<BASE+1024 or address==COUNT:
            writes.append([address,size,value])
    oracle.uc.hook_add(UC_HOOK_MEM_WRITE,record)
    rng=random.Random(0x236c)
    requests=[]
    for angle in range(256):
        for flags in range(4):
            index=angle%32
            sx=[0,1,255,256,257,0xffff][(angle+flags)%6]
            sy=[0xffff,0x10001,0x10000,16,256,257][(angle*3+flags)%6]
            requests.append(("write",[index,angle,sx,sy,flags],rng.randrange(256)))
    for index in [31,32,63,128,255,256,0xffffffff]:
        requests.append(("write",[index,0x100,0x10000,0xffff0100,0xffffffff],rng.randrange(256)))
    for i in range(128):
        requests.append(("allocate",[rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32)],i*3%256))
    def run(native):
        oracle.uc.mem_write(0x08066c78,oracle.rom[0x66c78:0x67078] if native else struct.pack('<512H',*[(i*997+0x8001)&65535 for i in range(512)]))
        cases=[]
        for i,(operation,args,count) in enumerate(requests):
            seed=(i*59+17)&255
            oracle.uc.mem_write(BASE,bytes((seed+j*37)&255 for j in range(1024)))
            oracle.uc.mem_write(COUNT,bytes([count]))
            if operation=="write":oracle.uc.mem_write(0x03007e00,struct.pack('<I',args[4]))
            writes.clear()
            oracle.call(operation,*args[:4])
            cases.append(dict(operation=operation,arguments=args,count=count,seed=seed,result=oracle.uc.reg_read(UC_ARM_REG_R0) if operation=="allocate" else None,
                              afterCount=oracle.uc.mem_read(COUNT,1)[0],writes=list(writes),sha256=hashlib.sha256(oracle.uc.mem_read(BASE,1024)).hexdigest()))
        return cases
    synthetic=run(False)
    native=run(True)
    spans={name:dict(offset=a,length=b-a,sha256=hashlib.sha256(oracle.rom[a:b]).hexdigest()) for name,(a,b) in ROUTINES.items()}
    for name,a in [("sine",0x66c78),("cosine",0x66e78)]:spans[name]=dict(offset=a,length=512,sha256=hashlib.sha256(oracle.rom[a:a+512]).hexdigest())
    return dict(schemaVersion=1,sourceSha256=control.ROM_SHA256,scope="Original writer/allocator and software division execute without interception; synthetic and actual native affine tables separate; not PPU pixel acceptance",routines=spans,synthetic=synthetic,native=native)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",required=True,type=Path)
    p.add_argument("--write",required=True,type=Path)
    a=p.parse_args()
    v=generate(a.rom)
    a.write.write_text(json.dumps(v,separators=(',',':'))+'\n',encoding='utf8')
    print(json.dumps({"synthetic":len(v["synthetic"]),"native":len(v["native"])}))
