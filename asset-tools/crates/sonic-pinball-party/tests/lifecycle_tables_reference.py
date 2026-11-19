"""Run the original A3VE table 2 and 3 lifecycle handlers. ROM/Unicorn are local test inputs only."""
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
WIDTH = {0x162c:3,0x2f634:1,0x3920:2,0x5724:3,0x42758:1,0x12ab4:1,0x389c:1,0x3d3a4:1,0x3046c:3,0x2614:6}
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
        if BASE+0x66c54<=a<BASE+0x66c74:
            reads[a]=int.from_bytes(u.mem_read(a,n),'little')
    u.hook_add(UC_HOOK_CODE,hook);u.hook_add(UC_HOOK_MEM_WRITE,write);u.hook_add(UC_HOOK_MEM_READ,read)
    cases=[]
    for table,st in [(t,s) for t,ss in [(2,list(range(104))+[65535]),(3,list(range(6))+[65535])] for s in ss]:
        for k in range(8 if st in (1,2,3,4,10,11,60,66,71,73,100,101,102) else 1):
            alt=STATE+0x400
            init=[[PTR,4,STATE],[TABLE,2,table],[STATE+0x28,2,st],[STATE+0x10,2,[16,16,16,65535,32768,5,4,0][k]],
                  [STATE+0x9a,2,[321,320,223,224,303,304,48,49][k]],
                  [STATE+0x98,2,[47,48,0xffff,0x8000][k%4]],[STATE+0x12,2,377],[STATE+0x14,1,11],[STATE+3,1,1],
                  [STATE+6,1,k%2],[STATE+0x6e,2,[0,2,5,1][k%4]],[STATE+0xaa,2,[17,65535][k%2]],[STATE+0xa8,2,19],[STATE+0xb8,2,17],
                  [0x03000770,2,k],[0x03002948,2,7 if k%2 else 1],[0x03000516,2,1],[0x030040c2,2,int(k!=1)],
                  [0x02037424,1,[255,0,6,13,4,127,128,2][k]],[0x020355b8,2,k%2],[0x02035fa0,2,k%2]]
            for a in [0x02036060,0x02036064,0x0203298a,0x02032994,0x02035746,0x020355e0,0x02036148]:
                init.append([a,2,111])
            if st in (101,102) and k==2:
                init += [[STATE+0x9a,2,321],[STATE+0x6e,2,2]]
            mutations={}
            if k==2:
                mutations={x:[[PTR,4,alt],[alt+0x28,2,65535],[alt+0x6e,2,0]] for x in [0x42758,0x389c,0x12ab4,0x3046c,0x2f634]}
            if k==3:mutations={0x162c:[[STATE+0x28,2,67]],0x3046c:[[0x02037424,1,255]],0x3920:[[0x02037424,1,19]]}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            before={}
            for a,n,v in init:
                put(a,n,v)
                before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            touched=set();reads={};calls=[]
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+(0x2e51d if table==2 else 0x5c545),SENTINEL,count=10000)
            assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(touched) if u.mem_read(a,1)[0]!=before.get(a,0)]
            cases.append(dict(table=table,init=init,mutations=mutations,calls=calls,diff=diff,reads=reads))
    return {'schemaVersion':1,'romSha256':ROM_SHA256,'resources':[],'routines':[dict(name=name,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for name,a,b in [('lifecycle_table2',0x2e51c,0x2ea04),('lifecycle_table3',0x5c544,0x5c660),('lifecycle_bank_base',0x3628,0x3640)]],'cases':cases}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);args=p.parse_args()
    args.output.write_text(json.dumps(generate(args.rom.read_bytes()),separators=(',',':'))+'\n')
