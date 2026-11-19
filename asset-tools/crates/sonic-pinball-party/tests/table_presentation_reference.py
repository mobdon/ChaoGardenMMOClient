"""A3VE message startup oracle. Resources are synthetic; a separate digest checks retail descriptors."""
import argparse,hashlib,json,struct
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
        off=a-BASE
        if off in (0x5784,0x162c,0xf84):
            calls.append([off,[u.reg_read(r) for r in regs[:0 if off==0xf84 else 3]]])
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]
    u.hook_add(UC_HOOK_CODE,cb)
    cases=[]
    common=[[0x080b061c,4,0x08100000],[0x080b0620,2,8],[0x080b0624,4,0x08100000],[0x080b0628,2,16]]
    for a,n,v in common:put(a,n,v)
    for a,n in [(0x08100000,65536+2048),(0x0855a5d8,2080)]:
        u.mem_write(a,bytes((i*73+19)&255 for i in range(n)))
    ranges=[(0x020329e0,0x26a0),(0x03000770,0xa0)]
    for k in range(72):
        mode=k%12;variant=k//12;desc=0x02010000;params=desc+128
        init=[[0x030040b0,4,STATE],[STATE+0x86,2,variant%2],[0x0300077a,2,0x1344],[0x03000772,2,65535],[0x030007c2,2,65535],
              [0x02033f34,2,int(variant==5)],[0x02032ae0,2,3],[0x02033f28,2,19],[0x02032ae8,2,123]]
        if 8<=mode<=10:init.extend([[desc,4,0x08100000],[desc+4,2,2],[desc+6,2,7],[desc+8,2,3],[desc+10,2,4],[desc+12,2,0 if variant==2 else 3],
                [desc+16,4,params],[desc+20,2,3],[desc+22,2,2],[desc+24,2,0x1ea],[params,2,135],[params+6,1,112]])
        else:
            chars=[32,65,48,57,176,185,33,92,93,64,95,77,255]
            init.extend([[desc+j,1,([1,2,32][variant%3] if j==0 else chars[(j+variant)%len(chars)])] for j in range(32)])
        u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
        for a,n,v in init:put(a,n,v)
        calls=[];args=[desc,mode+65536,0x10000+k]
        for r,v in zip(regs,args):u.reg_write(r,v)
        u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
        u.emu_start(BASE+0x3046d,SENTINEL,count=500000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
        h=hashlib.sha256()
        for a,n in ranges:h.update(u.mem_read(a,n))
        cases.append(dict(args=args,init=init,calls=calls,sha=h.hexdigest()))
    retail=[]
    u.mem_write(BASE,rom)
    for mode,desc in [(8,0x080afa4c),(8,0x080afa68),(8,0x080afa84),(8,0x080afa98),(8,0x080afaac),(8,0x080afac0),(8,0x080afad4),(10,0x080afaf4),(8,0x080afb18),(8,0x080afb34),(8,0x080cec94)]+[(8,0x080cecb0+20*i) for i in range(5)]+[(2,0x080b20ec)]:
        for en in (0,1):
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            init=[[0x030040b0,4,STATE],[STATE+0x86,2,en],[0x0300077a,2,0x1344],[0x02033f28,2,5]]
            for a,n,v in init:put(a,n,v)
            calls=[];args=[desc,mode,180]
            for r,v in zip(regs,args):u.reg_write(r,v)
            u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+0x3046d,SENTINEL,count=500000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            h=hashlib.sha256()
            for a,n in ranges:h.update(u.mem_read(a,n))
            retail.append(dict(args=args,init=init,calls=calls,sha=h.hexdigest()))
    spans=[('presentation_start',0x3046c,0x304a0),('presentation_prepare',0x2ff40,0x3046c),('presentation_queue_flush',0x2f9a0,0x2f9b4),('presentation_blank',0x2fb50,0x2fb98),('presentation_sequence',0x2fb98,0x2fbe8),('presentation_glyph',0x2fbe8,0x2fd60),('presentation_disable_display',0x2f8f8,0x2f920),('presentation_enable_display',0x2f920,0x2f92c),('presentation_copy',0x3720,0x374c)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],common=common,ranges=ranges,cases=cases,retail=retail)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
