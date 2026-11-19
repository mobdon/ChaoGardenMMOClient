"""Execute a composed original OBJ frame pipeline without interception.

Descriptor inputs and RAM are synthetic; the affine tables are authenticated
native resources. Only input scalars and full workspace hashes are published.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import control_reference as control

def generate(path):
    o=control.NativeOracle(path)
    o.uc.mem_map(0x02000000,0x40000)
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    regs=(UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3)
    rng=random.Random(0x26142700)
    cases=[]
    for k in range(40):
        seed=(k*59+17)&255
        o.uc.mem_write(0x02000000,bytes(0x40000))
        o.uc.mem_write(0x03000000,bytes((seed+i*37)&255 for i in range(0x8000)))
        patches=[[0x03000018,1,0],[0x030023b0,4,0]]
        descriptors=[]
        for i in range(4):
            p=0x02010000+i*128
            descriptors.append(p)
            for j in range(i+1):
                for off,val in [(0,rng.randrange(65535)),(2,rng.randrange(65536)),(4,rng.randrange(65536))]:patches.append([p+j*6+off,2,val])
            patches.append([p+(i+1)*6,2,65535])
        for a,n,v in patches:o.uc.mem_write(a,v.to_bytes(n,'little'))
        operations=[[0x1dc4,[k%32]],[0x2580,[]]]
        for i in range(3):operations.append([0x236c,[i,(k*17+i*63)&255,[0,256,1][i],[256,65535,0][i],k%4]])
        for i in range([0,1,2,8,16,64,127,128][k%8]):
            operations.append([0x2614,[rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32),descriptors[i%4],rng.randrange(1<<32),rng.randrange(1<<32)]])
        operations.append([0x2700,[]])
        for entry,args in operations:
            for reg,value in zip(regs,list(args)+[0]*4):o.uc.reg_write(reg,value)
            for i,value in enumerate(args[4:]):o.uc.mem_write(0x03007e00+i*4,struct.pack('<I',value))
            o.uc.reg_write(UC_ARM_REG_SP,0x03007e00);o.uc.reg_write(UC_ARM_REG_LR,0x03007f01)
            o.uc.emu_start(0x08000000+entry+1,0x03007f00,count=1000000)
            if o.uc.reg_read(UC_ARM_REG_PC)!=0x03007f00:raise ValueError('bounded original graphics pipeline did not return')
        cases.append(dict(seed=seed,patches=patches,operations=operations,iwramSha256=hashlib.sha256(o.uc.mem_read(0x03000000,0x7000)).hexdigest(),ewramSha256=hashlib.sha256(o.uc.mem_read(0x02000000,0x40000)).hexdigest()))
    spans={}
    for name,a,b in [('begin',0x1dc4,0x1e64),('affine',0x236c,0x2514),('submit',0x2614,0x2690),('compose',0x2700,0x28ec)]:spans[name]=dict(offset=a,length=b-a,sha256=hashlib.sha256(o.rom[a:b]).hexdigest())
    return dict(schemaVersion=1,sourceSha256=control.ROM_SHA256,scope='Composed original frame reset/affine/queue/submission/heap/OAM instructions with no hooks; not DMA/PPU output',routines=spans,cases=cases)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--rom',required=True,type=Path);p.add_argument('--write',required=True,type=Path);a=p.parse_args()
    v=generate(a.rom);a.write.write_text(json.dumps(v,separators=(',',':'))+'\n',encoding='utf8');print(json.dumps({'cases':len(v['cases'])}))
