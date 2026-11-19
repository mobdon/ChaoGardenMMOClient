"""Execute the native score award and four-contact event, including its native message formatter."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x389c:1,0x3046c:3,0x45a18:0,0x43124:1,0x2f9b4:3,0x2f614:1}
def make(rom,local_resources=False):
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
    common=[[base+i,1,(rom[base-BASE+i] if local_resources else 33+i) if i<n else 0] for base,n in [(0x080b0984,3),(0x080b09a8,4)] for i in range(n+1)]
    for a,n,v in common:put(a,n,v)
    cases=[];cap=10**19-1;mask=2**64-1
    for k in range(152):
        off=0x30c58 if k<24 else 0x33d40
        old=[0,cap-1,cap,cap+1,mask,0xffffffff][k%6]
        amount=[0,1,2,0x100000000,mask,cap,cap-1][k%7]
        args=[amount&0xffffffff,amount>>32] if k<24 else []
        j=k-24; first=j%2==0
        init=[[0x030040b0,4,STATE],[STATE+8,4,old&0xffffffff],[STATE+12,4,old>>32],
              [STATE+0x9a,2,225 if j%19==1 else 100],[STATE+0xaa,2,65535 if j%13==1 else 1],[STATE+0x6e,2,3 if j%4==0 else 0],
              [0x02034f50,4,65536 if j%17==0 else 0],[0x02035ffa,2,65535],[0x02036050,2,(j*31)&65535],[0x02035fa0,2,int(j%5==0)],
              [0x02035138,2,j%5],[0x0203513a,2,(j//2)%4]]
        for i in range(4):
            init.extend([[0x03004560+i,1,(1 if i==(0 if first else 3) and j%11!=0 else 0) if i in (0,3) else i+1],[0x02035130+2*i,2,0 if (j%7==0 and i in (1,2)) else 1]])
        calls=[];writes=set();before={};mutations={0x3046c:[[0x02035fa0,2,1]],0x43124:[[0x02036050,2,65535]]} if j%9==0 else {}
        u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
        for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
        for reg,v in zip(regs,args):u.reg_write(reg,v)
        u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
        u.emu_start(BASE+off+1,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
        diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
        cases.append(dict(offset=off,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    spans=[('score_award',0x30c58,0x30ca4),('four_contact_event',0x33d40,0x34044),('progress_message',0x30d20,0x30df4)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);p.add_argument("--local-resources",action="store_true",help="private verification only: use installed native contact labels");a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes(),a.local_resources),separators=(',',':'))+'\n')
