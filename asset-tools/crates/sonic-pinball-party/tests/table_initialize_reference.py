"""Run native board display initialization with captured display commit and tile transfer."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x30fc:0,0x162c:3}
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
    common=[[0x02014540+i,1,72] for i in range(0,2048,57)]+[[0x02014d3f,1,17]]
    cases=[]
    for off in [0x1efc4,0x1eff8,0x5618,0x5784]:
        for k in range(8):
            args={0x1efc4:[],0x1eff8:[],0x5618:[0x02012000+k*4],0x5784:[[255,31,28,30,29,257,0,7][k],[0,1,4,2,3,5,32,257][k],0x18a+k]}[off]
            init=[[0x03000770+i,2,(0xa053+i*371+k*19)&65535] for i in range(0,80,2)]
            init += [[a,4,0xabf12c39] for a in [0x02031adc,0x02031ae0,0x02031ad4,0x02031ae4]]
            init += [[0x02001c50+i,1,0xaa] for i in range(0,76,3)]
            mutations={0x30fc:[[0x03000778,2,0x0500+k]]} if k%2 else {}
            calls=[];writes=set();before={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in common+init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            for reg,v in zip(regs,args):u.reg_write(reg,v)
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+off+1,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(offset=off,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    spans=[('board_display',0x1eff8,0x1f0ec),('scroll_reset',0x1efc4,0x1eff8),('palette_work_init',0x5618,0x5668),('palette_range',0x5784,0x57c0)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
