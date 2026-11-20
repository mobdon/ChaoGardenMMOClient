"""Execute installed A3VE Garden Thumb on seeded RAM with a scripted JoyBus peer.
Registers are explicit bus inputs/latches (including the initial read of RECV).
This is an endpoint oracle, not cable timing, real flash or MMO acceptance.
Publish only hashes, calls and synthetic recipes; do not publish payload bytes.
"""
import argparse
import hashlib
import json
import os
import struct
from pathlib import Path

import reference_parity as parity
import spp_vectors  # noqa: E402

P,I,J=0x02000000,0x03000000,0x04000000
T=I+0x5368
IDLE=I+0x5b0
RXBUF=I+0x4270
TXBUF=I+0x4af8
PH=I+0x52fc
MODE=I+0x5308
ACTIVE=I+0x530c
RXF=I+0x52f8
TXF=I+0x5304
G=I+0x425f
KEY=I+0x5310
S=I+0x4aa0
CHAO=0x6f616843
END=I+0x7900
FIXTURE=spp_vectors.path("garden-transfer-vectors.json")
SPANS=[(0x480,0x590),(0x590,0x6b8),(0x6b8,0x988),(0x988,0xa40),(0xa40,0xaf0),
       (0xaf0,0xb0c),(0xb0c,0xb64),(0xb64,0xbb0),(0xbb0,0xbd0),(0x2364,0x2510),
       (0x2510,0x270c),(0x2eb8,0x338c),(0x338c,0x3424),(0x182c,0x1858),(0x3ac,0x43c)]
HOOKS={0x17b0:5,0xd04:3,0x1398:2,0xed4:1,0x1698:2,0x1066c:1,0x106e4:1,
       0x3ac:0,0x107a0:0,0x10660:0,0x11b04:0,0x10c70:0,0x11fe0:3,0x10cec:0}

def tables():
    ids=[0x35,0x36,0x37,0x38,0x39]
    yield 0x26f2c,bytes(ids)
    yield 0x1f2e0,b"".join(struct.pack("<h",(i*41)%513-256) for i in range(256))
    for idx in range(5):
        for lang in range(5):
            entry=0x26f04+4*(idx*2+lang) if lang<2 else 0x29a48+4*(idx*3+lang-2)
            at=0x32000+(idx*5+lang)*32
            yield entry,struct.pack("<I",P+at)
            yield at,bytes([4,2,0x5f,lang,idx+1,0xff,lang+1,0x5f,128,4,0xff,5,0x5f,128])

def recipes():
    base=[[G,1,0],[G+1,1,1],[T+4,1,2],[T+14,2,16],[T+25,1,0],[T+30,2,0],
          [ACTIVE,1,0],[MODE,1,0],[RXF,1,0],[TXF,1,0],[I+0x152c,1,0],
          [I+0x80,1,5],[KEY,2,0x102],[KEY+4,2,0],[IDLE-8,4,0x454f5341],[IDLE-4,4,0x45425347]]
    cases=[]
    def add(name,steps,w=()):cases.append(dict(name=name,writes=list(w),steps=steps))
    def call(off,*args):return [0,off,*args]
    def setv(addr,n,v):return [1,addr,n,v]
    def irq(f,v=0):return [2,f,v]
    ctl=call(0x2eb8,T)
    add("init", [call(0x480)])
    for a in (0,1):
        for timer in (0,9,10,255):
            for idle in (0,10,11,255):
                add(f"maint-{a}-{timer}-{idle}",[call(0x590)],[(ACTIVE,1,a),(IDLE+1,1,timer),(IDLE,1,idle)])
    for p in (0,1,2,3,4,5,6,7,8,9,10,255):
        for mask in (0,1,2,3,4,6,7):
            add(f"irq-{p}-{mask}",[irq(mask,CHAO)],[(PH,1,p),(IDLE+4,4,TXBUF if p==9 else RXBUF),(IDLE+8,2,12),(IDLE+10,2,0)])
    add("receive-handshake-and-retries", [call(0x480),irq(1),irq(4),irq(2,0x45425347),irq(0),irq(2,CHAO),call(0xaf0,5),irq(2,CHAO),
        irq(2,0x0200002a),irq(2,0x0200002a),irq(2,0x12340000),irq(4),irq(2,0x43210000),irq(2,0x78900100),irq(2,CHAO),ctl],
        [(G,1,0x40),(T+30,1,1)])
    add("send-retry-and-ack", [call(0xaf0,5),irq(2,0x4f414843),irq(2,0x02000022),irq(2,0x02000022),irq(2,0x26),
        irq(2,0x7070),irq(2,0x12340000),irq(4),irq(2,0xabcd0100),ctl],
        [(TXBUF,4,0xabcd1234),(ACTIVE,1,1),(G,1,0x40),(T+31,1,1)])
    add("receive-whole-opaque-record-through-flash", [call(0x480),irq(1),irq(4),irq(2,0x45425347),irq(0),irq(2,CHAO),
        ctl,call(0xaf0,5),irq(2,0x0004002a),[3,1024],irq(2,CHAO),ctl,ctl],
        [(G,1,0x40),(T+30,1,1),(TXBUF,1,255)])
    add("out-of-order-and-interrupted", [call(0xaf0,5),irq(2,0x0400002a),irq(2,0x11220000),irq(2,0x33440300),
        irq(2,CHAO),*[call(0x590)]*11,ctl],[(ACTIVE,1,1),(G,1,0x40),(T+30,1,1)])
    for lang in range(5):
        for mode in (0,1):
            add(f"localized-dialog-{lang}-{mode}",[ctl],[(ACTIVE,1,1),(G,1,0x40),(MODE,1,mode),(G+1,1,lang)])
        for wide in (0,1):
            add(f"direct-text-{lang}-{wide}",[call(0x2510 if wide else 0x2364,0x38 if wide else 0x35,I+0x780,I+0x781)],[(G+1,1,lang)])
    for name,w in [
        ("pause",[(G,1,128)]),("opening",[(ACTIVE,1,1),(T+14,2,65535)]),
        ("first-six-orbits",[(ACTIVE,1,1),(T+14,2,65488)]),
        ("finish-opening",[(ACTIVE,1,1),(T+14,2,15)]),("opening-block",[(ACTIVE,1,1),(T+14,2,15),(I+0x152c,1,1)]),
        ("lost-rx",[(RXF,1,1)]),("lost-tx",[(TXF,1,1)]),("prompt-wait",[(RXF,1,1),(T+25,1,1)]),
        ("lost-ack",[(TXF,1,1),(T+25,1,1),(KEY+4,2,1)]),
        ("active-ack",[(ACTIVE,1,1),(T+25,1,1),(KEY+4,2,1)]),
        ("active-wait",[(ACTIVE,1,1),(T+25,1,1)]),
        ("close-normal",[(T+30,1,1)]),("close-wide",[(T+31,1,1)]),
        ("hide-end",[(T+14,2,65489),(G,1,32)]),("hide-done",[(T+14,2,65488)]),
        ("busy-hide",[(I+0x152c,1,1)]),
        ("completed-send",[(G,1,64),(MODE,1,0),(ACTIVE,1,1),(TXF,1,2),(T+30,1,1)]),
        ("chao-departed",[(G,1,64),(ACTIVE,1,1),(RXF,1,2),(T+30,1,1),(I+0x5300,4,32),(RXBUF,1,0),(TXBUF,1,1)]),
    ]:add(name,[ctl],w)
    for angle in (0,126,127,249,255):
        add("orbit-"+str(angle),[call(0x338c,T)],[(G,1,32),(T+26,1,angle),(T+27,1,1)])
    add("orbit-removed",[call(0x338c,T)])
    return base,cases

def generate(path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR,UC_HOOK_MEM_READ,UC_HOOK_MEM_WRITE,UC_MEM_READ
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    rom=parity.bounded_read(Path(path),0x800000)
    parity.validate_rom(rom)
    payload,_=parity.decode(rom[0xd0970:],maximum=0x40000)
    assert parity.sha256(payload)==parity.GARDEN_SHA256
    base,cases=recipes()
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    outputs={}
    for profile in ("authentic","synthetic"):
        cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
        for addr,n in ((P,0x40000),(I,0x8000),(J,0x1000),(0x08000000,0x1000)):cpu.mem_map(addr,n)
        region=rom[0xaf] if profile=="authentic" else 0xbc
        cpu.mem_write(0x080000af,bytes([region]))
        cpu.mem_write(P,bytes(payload))
        if profile=="synthetic":
            for a,b in tables():cpu.mem_write(P+a,b)
        def boundary(m,a,size,user):
            off=a-P;n=HOOKS[off]
            args=[m.reg_read(r) for r in regs[:n]]
            if n>4:args.append(int.from_bytes(m.mem_read(m.reg_read(UC_ARM_REG_SP),4),"little"))
            calls.append([off,args])
            if off==0x3ac:return # real native CRC, header, rotation
            result=0
            if off==0xd04:result=I+0x5320+36*(5+sum(x[0]==off for x in calls))
            if off==0x11fe0:flash.append(parity.sha256(m.mem_read(args[1],args[2])))
            m.reg_write(UC_ARM_REG_R0,result);m.reg_write(UC_ARM_REG_PC,m.reg_read(UC_ARM_REG_LR))
        for off in HOOKS:cpu.hook_add(UC_HOOK_CODE,boundary,begin=P+off,end=P+off)
        def bios(m,num,_):
            assert num==2 and m.reg_read(UC_ARM_REG_PC)==P+0x11ae6
            src,dst,ctl=[m.reg_read(r) for r in regs[:3]]
            width=4 if ctl&(1<<26) else 2;n=ctl&0x1fffff
            assert n*width<=0x800 and not ctl&(1<<24)
            src&=~(width-1)
            b=bytes(m.mem_read(src,width*n))
            if src==P+0x6b8 and profile=="synthetic":b=bytes((x*11+1)&255 for x in range(len(b)))
            m.mem_write(dst,b)
        cpu.hook_add(UC_HOOK_INTR,bios)
        def io(m,access,a,n,v,_):
            read=access==UC_MEM_READ
            if read:v=int.from_bytes(m.mem_read(a,n),"little")
            bus.update(struct.pack("<BBII",int(not read),n,a,v))
        cpu.hook_add(UC_HOOK_MEM_READ|UC_HOOK_MEM_WRITE,io,begin=J,end=J+0xfff)
        results=[]
        for case in cases:
            bus=hashlib.sha256();calls=[];flash=[]
            cpu.mem_write(I,bytes((i*13+7)&255 for i in range(0x8000)))
            cpu.mem_write(J,bytes(0x1000));cpu.mem_write(P+0x3f800,bytes(0x800))
            def put(a,n,v):cpu.mem_write(a,v.to_bytes(n,"little"))
            for a,n,v in [*base,*case["writes"]]:put(a,n,v)
            def run(off,*args):
                for r,v in zip(regs,args):cpu.reg_write(r,v)
                cpu.reg_write(UC_ARM_REG_SP,I+0x7f00);cpu.reg_write(UC_ARM_REG_LR,END|1)
                cpu.emu_start(P+off+1,END,count=500000)
                assert cpu.reg_read(UC_ARM_REG_PC)==END,(case["name"],hex(cpu.reg_read(UC_ARM_REG_PC)))
            def irq(f,v):put(J+0x140,2,f);put(J+0x150,4,((region<<24)|0x425347) if v==0x45425347 else v);run(0x6b8)
            for kind,*s in case["steps"]:
                if kind==0:run(*s)
                elif kind==1:put(*s)
                elif kind==2:irq(*s)
                else:
                    for i in range(s[0]):irq(2,(((i*23+0x100)&65535)<<16)|((i&255)<<8)|(i>>8))
            results.append(dict(ram=parity.sha256(cpu.mem_read(I,0x7800)),map=parity.sha256(cpu.mem_read(P+0x3f800,0x800)),
                                mmio=bus.hexdigest(),calls=calls,flash=flash))
        outputs[profile]=results
    return dict(romSha256=parity.sha256(rom),payloadSha256=parity.sha256(payload),region=rom[0xaf],
                sources=[dict(space="garden",offset=a,length=b-a,sha256=parity.sha256(payload[a:b])) for a,b in SPANS],
                syntheticTables=[[o,list(b)] for o,b in tables()],base=base,cases=cases,results=outputs)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--rom",default=os.getenv("SPP_ROM"));p.add_argument("--write",type=Path)
    args=p.parse_args();data=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(data,separators=(",",":"))+"\n",encoding="utf8")
    elif json.loads(json.dumps(data))!=json.loads(FIXTURE.read_text(encoding="utf8")):raise SystemExit("transfer vectors differ")
    print(f"{len(data['cases'])} native scenarios in two profiles")
