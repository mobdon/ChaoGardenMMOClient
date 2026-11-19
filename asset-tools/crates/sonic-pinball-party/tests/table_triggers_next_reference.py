"""Execute native 344E8 and 347DC contact updates and score writes, including multi-frame contact sequences."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x389c:1,0x3046c:3,0x45a18:0,0x43124:1,0x2f9b4:3,0x459fc:1}
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,v.to_bytes(n,'little'))
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]
    def cb(u,a,n,_):
        off=a-BASE
        if off in WIDTH:
            calls.append([off,[u.reg_read(r) for r in regs[:WIDTH[off]]]])
            for a,n,v in mutations.get(off,[]):put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if STATE<=a<0x03007000:writes.update(range(a,a+n))
    u.hook_add(UC_HOOK_CODE,cb);u.hook_add(UC_HOOK_MEM_WRITE,write)
    common=[];cases=[];cap=10**19-1
    for case in range(600):
        k=case%300;off=0x344e8 if case<300 else 0x347dc
        old=[0,cap-1,0xffffffff,cap+1,0x123456789][k%5];args=[]
        init=[[0x030040b0,4,STATE],[STATE+8,4,old&0xffffffff],[STATE+12,4,old>>32],[STATE+0xaa,2,[65535,0,5][k%3]],[STATE+0x6e,2,5 if k%2 else 2],
              [0x02034f52,2,int(k%31==3)],[0x02034f54,2,int(k%37==5)],[0x02034f6c,4,65536 if k%17==3 else 0],[0x02035fa0,2,k%2],[0x02036050,2,[1,65535,21500,33333][k%4]]]
        bits=[1,2,4,8,16,32,40,17,9,63,48,0][(k//3)%12]
        for i in range(6):
            init.extend([[0x03004560+i,1,(bits>>i)&1],[0x0203513c+2*i,2,int(k%7!=i)]])
        calls=[];writes=set();before={};mutations={0x459fc:[[0x02035fa0,2,k%2],[0x02036050,2,34567],[STATE+0x6e,2,5]],0x389c:[[0x03004565,1,1]]} if k%9==2 else {}
        u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
        for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
        frames=[[]] if k<288 else [
            [[0x03004560+i,1,(e>>i)&1] for i in range(6)]+[[STATE+0xaa,2,v]] for e,v in
            [(1,65535),(2,0),(4,0),(8,3),(0,0),(16,65535),(32,1),(0,0),(8,65535),(4,0),(2,0),(1,0)]]
        for patches in frames:
            for a,n,v in patches:put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+off+1,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
        diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
        cases.append(dict(offset=off,args=args,init=init,mutations=mutations,frames=frames,calls=calls,diff=diff))
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name="directional_contacts",offset=0x344e8,length=0x2f4,sha256=hashlib.sha256(rom[0x344e8:0x347dc]).hexdigest()),dict(name="alternate_directional_contacts",offset=0x347dc,length=0x2d0,sha256=hashlib.sha256(rom[0x347dc:0x34aac]).hexdigest())],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
