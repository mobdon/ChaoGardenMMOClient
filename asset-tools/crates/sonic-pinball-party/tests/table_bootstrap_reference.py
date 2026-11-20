"""Run ABB4's five native table bootstrap entries including real callback stack registration."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={i:0 for i in [0x2560,0x1eff8,0x27ddc,0x20f7c,0x3a098,0x2b824,0x29a40,0x2a778,0x2dff8,0x39ea0,0x26214,0x1f0ec,0x1efc4]}
WIDTH.update({0x1dc4:1,0x22964:1,0x162c:3})
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
    common=[[0x0806a5a8+i*2,2,55+i*71] for i in range(12)]
    for a,n,v in common:put(a,n,v)
    cases=[]
    for off in [0xa668,0xa808,0xa9f0,0xaa20,0xa85c,0x2f68,0x2f74,0x2f92c]:
        for k in range(12):
            args={0x2f68:[],0x2f92c:[],0x2f74:[0x080abc01,0x12345678]}.get(off,[0x020199a0])
            init=[[0x03003068,2,k%6],[0x030040b0,4,STATE],[STATE+0x86,2,1],[STATE+0x88,2,k%2],[0x02031c44,2,k%3],[0x03002948,2,2],
                  [0x03002060,2,[0,3,9,10][k%4]],[0x03000778,2,0x1344],[0x0300077a,2,0x1f44],[0x02033f28,2,8],[0x02032ae0,2,12],
                  [0x02033f2c,2,2],[0x02034f44,2,15],[0x020199a0,4,1],[0x020319a8,4,1],[0x020319c0,4,31],[0x020328bc,4,1]]
            for i in range(0,2048,101):init.append([0x02004db8+i,1,72])
            mutations={}
            if k==7:mutations={0x1eff8:[[0x03003068,2,3],[0x0806a5b4,2,999]],0x1f0ec:[[0x03003068,2,2]],0x162c:[[0x0300077a,2,0x0800]]}
            if k==9:mutations={0x29a40:[[0x030040b0,4,STATE+0x400]],0x26214:[[0x03003068,2,2]]}
            calls=[];writes=set();before={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in common+init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            for reg,v in zip(regs,args):u.reg_write(reg,v)
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+off+1,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(offset=off,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    spans=[('boot_full',0xa668,0xa808),('boot_full_alias',0xa808,0xa814),('boot_variant4',0xa85c,0xa9f0),('boot_variant2',0xa9f0,0xaa20),('boot_variant3',0xaa20,0xab78),('aux_stack_clear',0x2f68,0x2f74),('aux_stack_push',0x2f74,0x2fa8),('boot_message_reset',0x2f92c,0x2f968)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],resources=[dict(name='bootstrap_dimensions',offset=0x6a5a8,length=24,sha256=hashlib.sha256(rom[0x6a5a8:0x6a5c0]).hexdigest())],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
