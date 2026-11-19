"""Native A3VE background message animation oracle, with synthetic and local-only retail resources."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,v.to_bytes(n,'little'))
    def cb(u,a,n,_):
        if a==BASE+0x5784:
            calls.append([0x5784,[u.reg_read(r) for r in [UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]]])
            if mutate:put(0x02033f38,2,0);put(0x02034f40,2,0)
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    u.hook_add(UC_HOOK_CODE,cb)
    ranges=[(0x02032ae0,0xc40),(0x02033f28,0x100),(0x02034f40,4),(0x03000770,0x80)]
    pattern=[(0x02032b00,3072),(0x02033f40,192)]
    fake=[(0x0855a458,0x1050),(0x0876b000,0x2500)]
    for a,n in fake:u.mem_write(a,bytes((i*37+13)&255 for i in range(n)))
    def one(mode,n,k,retail=False,steps=1):
        nonlocal calls,mutate
        u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
        for a,l in pattern:u.mem_write(a,bytes((i*43+19)&255 for i in range(l)))
        text=0x02010000;desc=0x02010200;p=0x02010300
        init=[[0x02033f38,2,n],[0x02034f40,2,(n+1)&65535],[0x02032af0,4,text],[0x02033f30,4,desc],[desc+16,4,p],[0x030040b0,4,STATE],
              [STATE+0x86,2,k%2],[p,2,65505],[p+2,2,501],[p+4,2,[0,31,65533,120][k%4]],
              [p+6,2,65],[p+8,2,65506],[p+10,2,[12,32,5,0][k%4]]]
        chars=[48,65,33,255,32,57,92,64,177,49,66,34,0,185,175,94,93]
        init.extend([[text+i,1,chars[(i+k)%len(chars)]] for i in range(61)])
        if retail and mode>=8:init.append([0x02033f30,4,0x080afaf4 if mode==10 else 0x080afa84])
        for a,l,v in init:put(a,l,v)
        mutate=mode==10 and k%4==1 and not retail;calls=[]
        for _ in range(steps):
            u.reg_write(UC_ARM_REG_R0,mode+65536);u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+0x304a1,SENTINEL,count=250000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
        h=hashlib.sha256()
        for a,l in ranges:h.update(u.mem_read(a,l))
        return dict(mode=mode,steps=steps,init=init,calls=calls,mutate=mutate,sha=h.hexdigest())
    mutate=False;calls=[]
    cases=[one(mode,n,k) for mode in range(12) for k,n in enumerate([0,1,2,14,15,16,19,20,29,30,31,118,119,120,65535])]
    u.mem_write(BASE,rom)
    retail=[one(7,0,k,True,121) for k in range(4)]+[one(mode,0,k,True,41) for mode in [8,9,10] for k in [0,1]]
    a,b=0x304a0,0x30a0c
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name='message_frame',offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest())],ranges=ranges,cases=cases,retail=retail)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
