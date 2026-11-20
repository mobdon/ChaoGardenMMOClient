"""Native A3VE Garden background/task/draw-list/OAM instruction oracle.

Synthetic descriptors, tasks and queue bytes only. Task callback35CC is an
explicit mutating test boundary. SWI0C is a word-fill primitive; no task game
logic, BIOS timing, PPU or end-to-end acceptance is implied by these fixtures.
"""
import argparse
import json
import os
from pathlib import Path
import random
import struct
import reference_parity as p
import spp_vectors  # noqa: E402

FIXTURE=spp_vectors.path('garden-services-vectors.json')
BASE=0x02000000
ROUTINES=[('backgrounds',0x1420,0x151c),('tasks',0xeac,0xed4),('unlink',0xed4,0xf34),
          ('compose',0x1088,0x117c),('sort',0x117c,0x1290),('draw',0x1374,0x1420),
          ('rng',0x151c,0x1554),('queue',0x17a4,0x1830),('snapshot',0xdb0,0xeac),('affineSnapshot',0xf34,0x1088)]

def pattern(seed,n):return bytes((seed+37*i)&255 for i in range(n))

def generate(path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    rom=p.bounded_read(Path(path),p.ROM_LENGTH);p.validate_rom(rom)
    payload,consumed=p.decode(rom[p.GARDEN_OFFSET:],p.GARDEN_LENGTH)
    assert consumed==p.GARDEN_STORED and p.sha256(payload)==p.GARDEN_SHA256
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x40000),(0x03000000,0x8000),(0x06000000,0x20000)]:cpu.mem_map(a,n)
    cpu.mem_write(BASE,bytes(payload));calls=[];mutate=False
    def task_callback(c,a,size,user):
        obj=c.reg_read(UC_ARM_REG_R0);calls.append([0x35cc,[obj]])
        if mutate and obj==0x03005320+2*36:c.mem_write(obj+6,b'\x04')
        c.reg_write(UC_ARM_REG_PC,c.reg_read(UC_ARM_REG_LR))
    cpu.hook_add(UC_HOOK_CODE,task_callback,begin=BASE+0x35cc,end=BASE+0x35cc)
    def bios(c,irq,user):
        assert irq==2 and c.reg_read(UC_ARM_REG_PC) in (BASE+0x11ae2,BASE+0x11ae6)
        src,dst,control=[c.reg_read(r) for r in [UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]]
        if control==0x01000200:
            val=bytes(c.mem_read(src,4));c.mem_write(dst,val*512)
        else:
            assert control==0x04000288
            for i in range(648):c.mem_write(dst+i*4,bytes(c.mem_read(src+i*4,4)))
    cpu.hook_add(UC_HOOK_INTR,bios)
    rng=random.Random(0x14201088);specs=[]
    def case(off,args=None):return {'seed':rng.randrange(256),'entry':off,'args':args or [],'patches':[],'operations':[],'mutate':False}
    def patch(c,a,data):c['patches'].append([a,data.hex()])
    for k in range(100):
        c=case(0x1420);count=k%9;patch(c,0x030014f8,bytes([count]))
        for j in range(count):
            typ=(k+j)%3;w=[0,1,6,32,33,64][(k+j)%6];h=[0,1,2,4,8][j%5]
            src=0 if typ!=0 else 0x02030000+j*0x800
            if typ==2:w=255
            dest=0x06000000+j*0x1000
            patch(c,0x030013d8+12*j,struct.pack('<IIBBH',src,dest,w,h,rng.randrange(65536)))
        specs.append(c)
    for k in range(64):
        c=case(0x1088);patch(c,0x03001088,bytes(148))
        n=[0,1,4,20,70][k%5];links=[[],[],[],[]]
        for j in range(n):
            layer=j%4;idx=j+4;links[layer].append(idx)
            obj=0x03005320+(j+2)*36;desc=0x02030000+j*0x100
            patch(c,0x03001089+idx*2,bytes([j+2]))
            patch(c,obj+0x10,struct.pack('<I',desc));patch(c,obj+0xa,struct.pack('<H',rng.randrange(65536)))
            patch(c,obj+0xe,struct.pack('<H',rng.randrange(65536)));patch(c,obj+0x14,struct.pack('<H',rng.randrange(65536)))
            patch(c,obj+0x16,rng.randbytes(3))
            pieces=130 if k==63 else 1+j%3
            for q in range(pieces):
                data=bytearray(rng.randbytes(8));data[7]=int(q+1<pieces)
                patch(c,desc+q*8,bytes(data))
        for layer,indices in enumerate(links):
            chain=[layer]+indices
            for a,b in zip(chain,chain[1:]):patch(c,0x03001088+a*2,bytes([b]))
        specs.append(c)
    for k in range(64):
        c=case(0x117c);n=k%71;patch(c,0x0300127a,bytes([n]));patch(c,0x030011d8,bytes(162))
        patch(c,0x03001088,bytes(148));patch(c,0x03001080,bytes(range(4)));patch(c,0x0300111c,b'\x04')
        for j in range(n):patch(c,0x03001148+j*2,bytes([rng.randrange(162),j+2]))
        specs.append(c)
    for k in range(16):
        c=case(0x1374);specs.append(c)
        c=case(0x13dc,[0x03005320]);specs.append(c)
        c=case(0x1398,[rng.randrange(1<<32),[0,161,162,208,209,255,256,0xffffffff][k%8]])
        patch(c,0x0300127a,bytes([k]));specs.append(c)
    for k in range(12):
        c=case(0xeac);c['mutate']=bool(k%2)
        for j in range(6):
            obj=0x03005320+j*36
            patch(c,obj,struct.pack('<I',BASE+0x35cd));patch(c,obj+4,bytes([j,max(j-1,0),j+1 if j<5 else 0]))
        specs.append(c)
        d=case(0xed4,[0x03005320+3*36]);d['patches']=list(c['patches'])
        patch(d,0x03000608,bytes([69 if k%2 else 3]));patch(d,0x03000606,bytes([k]))
        specs.append(d)
    for k in range(64):
        for off,args in [(0x151c,[]),(0x1528,[rng.randrange(1<<32)]),(0x1534,[]),(0x17a4,[])]:
            c=case(off,args);specs.append(c)
        c=case(0x17b0,[rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32),rng.randrange(1<<32)])
        patch(c,0x030014f8,bytes([[0,1,22,23,24,255][k%6]]));specs.append(c)
        c=case(0x1254,[rng.randrange(1<<32),k%4]);patch(c,0x03001080,bytes(range(4)));patch(c,0x03001088,bytes(148));patch(c,0x0300111c,bytes([4+k]));specs.append(c)
        for off in (0xdb0,0xe28,0xf34,0xfd4):specs.append(case(off))
    for c in specs:
        seed=c['seed'];cpu.mem_write(0x03000000,pattern(seed,0x8000));cpu.mem_write(0x06000000,pattern(seed+7,0x20000));cpu.mem_write(0x02030000,pattern(seed+19,0x10000))
        for a,h in c['patches']:cpu.mem_write(a,bytes.fromhex(h))
        calls.clear();mutate=c['mutate']
        for reg,val in zip([UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3],c['args']+[0]*4):cpu.reg_write(reg,val)
        cpu.reg_write(UC_ARM_REG_SP,0x03007e00);cpu.reg_write(UC_ARM_REG_LR,0x03007f01)
        for i,arg in enumerate(c['args'][4:]):cpu.mem_write(0x03007e00+i*4,struct.pack('<I',arg))
        cpu.emu_start(BASE+c['entry']+1,0x03007f00,count=2000000)
        assert cpu.reg_read(UC_ARM_REG_PC)==0x03007f00
        c.update(calls=list(calls),iwramSha256=p.sha256(cpu.mem_read(0x03000000,0x7000)),vramSha256=p.sha256(cpu.mem_read(0x06000000,0x20000)))
        if c['entry'] in (0x151c,0x1528,0x1534,0x17b0):c['return']=cpu.reg_read(UC_ARM_REG_R0)
    return {'schemaVersion':1,'romSha256':p.ROM_SHA256,'payloadSha256':p.GARDEN_SHA256,'scope':__doc__.strip(),
            'routines':[{'name':name,'space':'garden','offset':a,'length':b-a,'sha256':p.sha256(payload[a:b])}for name,a,b in ROUTINES],'cases':specs}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--rom',default=os.environ.get('SPP_ROM'));parser.add_argument('--write',type=Path);a=parser.parse_args()
    if not a.rom:parser.error('provide local --rom')
    v=generate(a.rom)
    if a.write:a.write.write_text(json.dumps(v,separators=(',',':'))+'\n',encoding='utf8')
    elif v!=json.loads(FIXTURE.read_text(encoding='utf8')):raise ValueError('Garden service vectors differ')
    print(json.dumps({'cases':len(v['cases']),'wholeGardenPorted':False}))
