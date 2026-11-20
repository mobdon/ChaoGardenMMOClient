"""Run native collision mask reset and pointer binding on synthetic pointer tables."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={}
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
    common=[[0x086e9338+4*i,4,0x08012004+i*0x208] for i in range(30)]
    if local_resources:common=[[a,n,int.from_bytes(rom[a-BASE:a-BASE+n],"little")] for a,n,_ in common]
    cases=[]
    for off in [0x2b824,0x2b8ac,0x2b920]:
        for k in range(8):
            p=STATE+0x800+(k%2)*0x280
            init=[[0x03003068,2,[0,1,2,3,4,1,2,3][k]],[0x030040b0,4,p],[p+6,1,0x77]]
            init += [[p+0x2a+i*2,2,0x1240+i] for i in range(32)]
            init += [[0x02032980+i*2,2,0x3344+i] for i in range(11)]
            args=[];calls=[];mutations={};writes=set();before={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in common+init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+off+1,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(offset=off,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    spans=[('selected_bind',0x2b824,0x2b8ac),('fixed_bind',0x2b8ac,0x2b920),('reset',0x2b920,0x2b990)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],resources=[dict(offset=o,length=32,sha256=hashlib.sha256(rom[o:o+32]).hexdigest()) for o in (0x6e9340,0x6e9360,0x6e9380)],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);p.add_argument("--local-resources",action="store_true",help="private verification only: use installed native pointer tables");a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes(),a.local_resources),separators=(',',':'))+'\n')
