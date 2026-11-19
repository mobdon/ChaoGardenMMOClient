"""Native sound-DMA gate transactions with one controlled CpuSet zero-fill seam.

Unicorn does not model DMA hardware. SWI0B memory effects for this exact fixed
zero-word fill are supplied by the oracle; no authentic BIOS/timing is claimed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import control_reference as control

INFO=0x02002000
MAGIC=0x68736d53
ROUTINES={"gate040":(0x5f040,0x5f0bc),"gate0bc":(0x5f0bc,0x5f0f8),"cpu_set":(0x5feb4,0x5feb8)}

def generate(path):
    oracle=control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    oracle.uc.mem_map(0x02000000,0x40000)
    from unicorn import UC_HOOK_INTR,UC_HOOK_MEM_WRITE
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2
    writes=[]
    fills=[]
    def interrupt(uc,number,_):
        if number!=2:
            raise ValueError("unexpected ARM exception")
        src,dst,count=(uc.reg_read(r) for r in (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2))
        if count!=0x05000318 or bytes(uc.mem_read(src,4))!=bytes(4):
            raise ValueError("unexpected CpuSet inputs")
        fills.append([dst,count])
        uc.mem_write(dst,bytes(0x318*4))
    def write(uc,access,address,size,value,_):
        if INFO<=address<INFO+0x1000 or 0x04000000<=address<0x04001000:
            writes.append([address,size,value])
    oracle.uc.hook_add(UC_HOOK_INTR,interrupt)
    oracle.uc.hook_add(UC_HOOK_MEM_WRITE,write)
    cases=[]
    for entry,name in [(0x5f040,"gate040"),(0x5f0bc,"gate0bc")]:
        for signature in [0,1,0xffffffff,MAGIC-10,MAGIC-1,MAGIC,MAGIC+1,MAGIC+10]:
            for bits in range(4):
                seed=(signature+bits*59+entry)&255
                info=bytearray((seed+i*37)&255 for i in range(0x1000))
                struct.pack_into("<I",info,0,signature)
                dma1=0x12345678 if bits&1 else 0x10345678
                dma2=0xfedcba98 if bits&2 else 0xfcdcba98
                io=bytearray(0x1000)
                struct.pack_into("<I",io,0xc4,dma1)
                struct.pack_into("<I",io,0xd0,dma2)
                oracle.uc.mem_write(INFO,bytes(info))
                oracle.uc.mem_write(0x03007ff0,struct.pack("<I",INFO))
                oracle.uc.mem_write(0x04000000,bytes(io))
                writes.clear();fills.clear()
                oracle.call(name)
                cases.append(dict(entry=entry,signature=signature,seed=seed,dma1=dma1,dma2=dma2,writes=list(writes),fills=list(fills),
                                  infoSha256=hashlib.sha256(oracle.uc.mem_read(INFO,0x1000)).hexdigest(),
                                  ioSha256=hashlib.sha256(oracle.uc.mem_read(0x04000000,0x1000)).hexdigest()))
    return dict(schemaVersion=1,sourceSha256=control.ROM_SHA256,
                scope="Exact native gate instructions; controlled SWI0B zero-fill only; no DMA hardware or BIOS timing",
                routines={name:dict(offset=a,length=b-a,sha256=hashlib.sha256(oracle.rom[a:b]).hexdigest()) for name,(a,b) in ROUTINES.items()},cases=cases)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",required=True,type=Path)
    p.add_argument("--write",required=True,type=Path)
    a=p.parse_args()
    data=generate(a.rom)
    a.write.write_text(json.dumps(data,separators=(",",":"))+"\n",encoding="utf8")
    print(json.dumps({"verified":True,"cases":len(data["cases"])}))
