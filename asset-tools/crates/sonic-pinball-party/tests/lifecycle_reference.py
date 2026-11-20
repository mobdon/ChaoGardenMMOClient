"""Run the original A3VE lifecycle router. ROM/Unicorn are local test inputs only."""
import argparse
import hashlib
import json
import struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
from ball_reference import ROM_SHA256, SENTINEL, STATE

BASE = 0x08000000
PTR = 0x030040b0
TABLE = 0x03003068
WIDTH = {0x2e51c:0,0x5c544:0,0x5724:3,0x42758:1,0x12ab4:1,0x389c:1,0x3d3a4:1,0x3046c:3,0x2614:6}
def generate(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:
        u.mem_map(a,n)
    u.mem_write(BASE,rom)
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    def put(a,n,v): u.mem_write(a,v.to_bytes(n,'little'))
    def hook(u,a,n,_):
        off=a-BASE
        if off in WIDTH:
            args=[u.reg_read(r) for r in regs[:WIDTH[off]]]
            if WIDTH[off]>4: args+=list(struct.unpack('<II',u.mem_read(u.reg_read(UC_ARM_REG_SP),8)))
            calls.append([off,args])
            for at,n,v in mutations.get(off,[]):put(at,n,v); touched.update(range(at,at+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if a<0x03007000:touched.update(range(a,a+n))
    def read(u,access,a,n,v,_):
        if a>=BASE and not BASE+0x2e1a0<=a<BASE+0x2e51c and not BASE+0x29a40<=a<BASE+0x29ba8:
            reads[a]=int.from_bytes(u.mem_read(a,n),'little')
    u.hook_add(UC_HOOK_CODE,hook);u.hook_add(UC_HOOK_MEM_WRITE,write);u.hook_add(UC_HOOK_MEM_READ,read)
    cases=[]
    for st in list(range(14))+[14,65535]:
        for k in range(12):
            table=k%2 if k<9 else [2,3,65535][k-9]
            alt=STATE+0x400
            init=[[PTR,4,STATE],[TABLE,2,table],[STATE+0x28,2,st],[STATE+0x10,2,[8,8,8,8,8,8,0,65535,7][k%9]],
                  [STATE+0x9a,2,[31,32,96,97,307,308,375,376,65535][k%9]],
                  [STATE+0xa2,2,[752,752,751,65535][k%4]],[STATE+3,1,int(k%4!=3)],
                  [STATE+6,1,[0,255,128,1][k%4]],[STATE+0x12,2,[238,404,237,405][k%4]],
                  [STATE+0x14,1,13],[STATE+0x6e,2,k%2],[STATE+0xb8,2,22],[STATE+0xa8,2,22],[STATE+0xaa,2,22],
                  [0x02036060,2,21],[0x02036064,2,17],[0x0203296c,2,1],[0x02035746,2,k%2],[0x020329a0,2,[8,9,15,7][k%4]],
                  [0x0203524c,2,65535],[0x03002948,2,7 if k==4 else 0],[0x03002960,2,k%6],
                  [0x03000516,2,1],[0x030040c2,2,int(k!=7)],[0x020355b8,2,k%2]]
            mutations={}
            if k in [8,9,10]:mutations={x:[[PTR,4,alt],[alt+6,1,130],[alt+0x28,2,65535]] for x in [0x42758,0x2e51c,0x5c544]}
            if k==5:mutations={0x12ab4:[[STATE+0x28,2,255]],0x389c:[[0x020355b8,2,1]]}
            if k==3:init.append([STATE+8,4,1])
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            before={}
            for a,n,v in init:
                put(a,n,v)
                before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            touched=set();reads={};calls=[]
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+0x2e1a1,SENTINEL,count=10000)
            assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(touched) if u.mem_read(a,1)[0]!=before.get(a,0)]
            cases.append(dict(init=init,mutations=mutations,calls=calls,diff=diff,reads=reads))
    return {'schemaVersion':1,'romSha256':ROM_SHA256,'resources':[],'routines':[dict(name=name,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for name,a,b in [('lifecycle_dispatch',0x2e1a0,0x2e1e0),('lifecycle_table01',0x2e1e0,0x2e51c),('lifecycle_clear_pair',0x459e0,0x459f0)]],'cases':cases}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);args=p.parse_args()
    args.output.write_text(json.dumps(generate(args.rom.read_bytes()),separators=(',',':'))+'\n')
