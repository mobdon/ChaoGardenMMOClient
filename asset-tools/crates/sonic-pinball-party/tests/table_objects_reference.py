"""Run native table object placement, capturing six arguments at its object queue boundary."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_LR,UC_ARM_REG_SP,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x2614:4}
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,v.to_bytes(n,'little'))
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    def cb(u,a,n,_):
        off=a-BASE
        if off in WIDTH:
            calls.append([off,[u.reg_read(r) for r in regs[:WIDTH[off]]]+[int.from_bytes(u.mem_read(u.reg_read(UC_ARM_REG_SP)+j,4),"little") for j in (0,4)]])
            for a,n,v in mutations.get(off,[]):put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if STATE<=a<0x03007000:writes.update(range(a,a+n))
    u.hook_add(UC_HOOK_CODE,cb);u.hook_add(UC_HOOK_MEM_WRITE,write)
    common=[[0x086ddf44,2,0x2134],[0x086ddf48,2,0x7162]]
    for i in range(9):
        p=0x08720000+0x200*i
        common.append([0x0806a43c+i*4,4,p])
        for slot in [0,1,2,4,5,6,40,45,49,50,79,80,83,87,96,97,99,114]:
            common.append([p+4*slot,4,p+0x10000+4*slot])
    for a,n,v in common:put(a,n,v)
    cases=[]
    for id in range(9):
        for k,yy in enumerate([-32768,-14,-13,0,32,33,65,71,73,79,87,91,95,104,126,135,136,166,185,209,240,241,407,408]):
            init=[[0x03002948,2,id],[0x030040b0,4,STATE],[STATE+16,2,(0x2418*k)&65535],[STATE+18,2,yy&65535],[0x0203524c,2,(0x5101*k)&65535],
                  [STATE+0x90,4,[227,149,222,148,234,235][k%6]*256],[STATE+0x94,4,[148,54,226,55,288,289][k%6]*256],[STATE+40,2,k%2],[0x020329d4,2,k%3],
                  [0x020326ec,4,(-17*k)&0xffffffff],[0x02032708,4,(1042*k)&0xffffffff],[0x0203250c,2,k%3],[0x02031af0,4,[0,11,10,-10,-11,123][k%6]&0xffffffff],[0x02031aec,4,k%3]]
            mutations={0x2614:[[STATE+18,2,135],[0x03002948,2,5]]} if k==3 else {}
            args=[];calls=[];writes=set();before={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+0x1fc31,SENTINEL,count=200000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(offset=0x1fc30,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name="table_object_placement",offset=0x1fc30,length=0x8f8,sha256=hashlib.sha256(rom[0x1fc30:0x20528]).hexdigest())],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
