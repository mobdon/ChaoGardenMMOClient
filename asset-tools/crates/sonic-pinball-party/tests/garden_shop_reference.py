"""Offline native A3VE shop parity. Runs authentic Thumb code, synthetic RAM and input.
Only calls crossing the listed boundaries are intercepted. The task allocator
and PRNG execute in the payload. The portable profile substitutes published
synthetic tables; the local profile reads all tables from the verified game.
"""
import argparse
import json
import os
import struct
from pathlib import Path

import reference_parity as parity

FIXTURE = Path(__file__).with_name("garden-shop-vectors.json")
P, I = 0x02000000, 0x03000000
S, G, K, OBJ = I+0x4aa0, I+0x4250, I+0x5310, I+0x5320
T, HAND, CHAO = OBJ+0x24*2, OBJ+0x24*3, OBJ+0x24*4
MENU = OBJ+0x24*5
END = I+0x7900
SHA = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
SPANS = {
    "decimalDigits": (0x15cc,0x163c), "priceRows": (0x7500,0x7688),
    "panel": (0x7688,0x7978), "firstPurchase": (0x7978,0x7b00),
    "secondPurchase": (0x7b00,0x7c4c), "thirdPurchase": (0x7c4c,0x7da0),
    "carryFood": (0x7da0,0x8010), "carrySecond": (0x8010,0x80e0),
    "carryThird": (0x80e0,0x8324), "restoreFood": (0x8324,0x83e4),
    "firstRow": (0x8b3c,0x8b7c), "secondRow": (0x8b7c,0x8bc4),
    "thirdRow": (0x8bc4,0x8c0c), "labelSetup": (0x8c0c,0x8c50),
    "labelFrame": (0x8c50,0x8c9c), "firstTransition": (0x8c9c,0x8cd4),
    "secondTransition": (0x8cd4,0x8d04), "thirdTransition": (0x8d04,0x8d3c),
    "foodFrame": (0x8d3c,0x8d90), "effectRows": (0x1269c,0x126cd),
    "rowOrder": (0x126cd,0x126d4), "prices": (0x126d4,0x1272c),
    "glyphTemplate": (0x1fe08,0x1fe30), "firstBanks": (0x20668,0x2066f),
    "thirdBanks": (0x2066f,0x20672), "dropMap": (0x216a0,0x216a0+22*20),
}
ARITY = {0xd04:3, 0x1534:0, 0x1398:2, 0x1254:2, 0xed4:1,
         0x1066c:1, 0x3e04:1, 0x83e4:1, 0x8dec:1, 0x85e0:1}

def recipes():
    base = [(I+0x150c,4,HAND),(I+0x1510,4,CHAO),(T+4,1,2),(T+0x1b,1,5),
            (T+0x1a,1,1),(T+0x19,1,3),(T+0xe,2,26),(HAND+0xa,2,48),
            (HAND+0xe,2,18),(CHAO+0xa,2,48),(CHAO+0xe,2,37),
            (CHAO+0x1e,1,1),(I+0x6194,4,999999),(I+0x159a,1,2),
            (K+4,2,1),(G+8,4,25),(I+0x1528,1,59),(S+0x5c,1,255),
            (S+0x5e,1,255),(S+0x59,1,255),(S+0x1c,1,255),
            (I+0x1530,1,255),(S+0xcf,1,255),(I+0x606,1,4),(I+0x607,1,4)]
    for i in range(70):
        base.append((I+0x5c0+i,1,i+2))
    # Parent 2 is already allocated. The fresh allocation chain feeds its
    # returned records into the seven first-category rows and optional entries.
    base.extend([(T+6,1,1),(OBJ+0x24+5,1,2)])
    variants=[]
    def case(name,entry,writes=(),steps=()):
        variants.append(dict(name=name,writes=list(writes),steps=list(steps) or [[entry,T]]))
    case("price-glyphs-and-panel",0,steps=[[0x7500,T],[0x7688,T]])
    for category,entry in enumerate([0x7978,0x7b00,0x7c4c]):
        for suffix,writes in [
            ("purchase",[]),("no-rings",[(I+0x6194,4,0)]),
            ("direction-held",[(K,2,0x80)]),("blink-hidden",[(K+4,2,0),(G+8,4,0)]),
            ("cursor-above",[(HAND+0xe,2,2)]),("cursor-below",[(HAND+0xe,2,19)]),
            ("slide-out",[(MENU+0xa,2,65535)]),("gone",[(MENU+0xa,2,65488)]),
            ("paused",[(G+15,1,16)]),("locked",[(I+0x1509,1,1)]),
            ("hand-busy",[(HAND+7,1,1)]),("transition",[(G+15,1,128)]),
        ]: case(f"category-{category}-{suffix}",entry,writes)
    for name,writes in [
        ("full",[(I+0x159a,1,8)]),("inconsistent-count",[(S+0x5c,1,2),(S+0x5e,1,3)]),
        ("last-slot",[(S+0x5c,1,2),(S+0x5e,1,3),(S+0x61,1,255)]),
        ("held-a",[(K+4,2,0)]),("outside-shop",[(HAND+0xa,2,52)])]:
        case("food-"+name,0x7978,writes)
    for id in range(7):
        for fullness in (0,200):
            case(f"contact-{id}-{fullness}",0x7da0,[(I+0x1528,1,0),(T+0x1a,1,id),(S+0x74,1,fullness)])
    for name,writes in [("drop",[(S+0x58,1,1)]),("clamp",[(HAND+0xe,2,151)]),
                        ("chao-busy",[(I+0x152f,1,1)]),("boundary-x",[(CHAO+0xa,2,59)]),
                        ("boundary-y",[(CHAO+0xe,2,55)]),("facing-right",[(CHAO+0xa,2,47)]),
                        ("facing-away",[(CHAO+0xa,2,91),(HAND+0xa,2,90),(S+0x74,1,255)]),
                        ("chao-inactive",[(CHAO+0x1e,1,0)]),("directions",[(K,2,32)]),
                        ("pointer-lock",[(I+0x1509,1,1)]),("flag80",[(G+15,1,128)])]:
        case(name,0x7da0,[(I+0x1528,1,0)]+writes)
    for entry in [0x8010,0x80e0]:
        for ident in range(3):
            for energy in (49,50):
                case(f"placement-{entry:x}-{ident}-{energy}",entry,[(T+0x1a,1,ident),(I+0x1528,1,0),(S+0x73,1,energy)])
    for id,entry in enumerate([0x8b3c,0x8b7c,0x8bc4]):
        case(f"init-label-{id}",entry,steps=[[entry,T],[0x8c50,OBJ+6*0x24]])
    case("label-closed",0x8c50,[(MENU+0xa,2,65488)])
    case("restore-food",0x8324,[(S+0x27,1,51),(S+0x28,1,200)])
    case("removed-externally",0x8d3c,[(S+0x5d,1,255)])
    for name,writes in [
        ("first",[(S+0x58,1,255)]),("cached",[(I+0x1530,1,8)]),
        ("second-disabled",[(I+0x1501,1,1)]),("third-one",[(S+0x31,1,1)]),
        ("third-two",[(S+0x31,1,1),(S+0x33,1,1)]),
        ("no-third",[(S+0x31,1,1),(S+0x33,1,1),(S+0x14,4,2)])]:
        case("open-"+name,0,[(K+4,2,512)]+writes,[[0x7500,T],[0x7688,T]])
    for seed in [0,1,2,7,15,31,59,127,295,999,10007,65535]:
        case("open-seed-"+str(seed),0,[(K+4,2,512),(I+0x13a8,4,seed)],[[0x7500,T],[0x7688,T]])
    for name,flags,key,x,enabled in [("pause",16,0,0,1),("transition",128,0,0,1),
            ("close",0,512,0,1),("slide-open",0,0,65532,1),("slide-shut",0,0,65532,0)]:
        case("panel-"+name,0x7688,[(G+15,1,flags),(K+4,2,key),(T+0xa,2,x),(T+0x1e,1,enabled),(S+0x14,4,5)])
    return base,variants

def synthetic():
    regions=[(0x1269c,bytes(1 if i%7==1 and (i//7)%2==0 else 255 if i%7==1 else 7 for i in range(49))),
             (0x126cd,bytes(range(6,-1,-1))),
             (0x126d4,b"".join(struct.pack("<I",x) for x in [0,7,50,101,999,10000,123456]+list(range(100,112))+[3,15,300])),
             (0x1fe08,bytes((i*3+17)%256 for i in range(40))),
             (0x20668,bytes(range(7,17))),
             (0x216a0,bytes((i%11==0) for i in range(440)))]
    return regions

def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,
                                  UC_ARM_REG_SP,UC_ARM_REG_PC,UC_ARM_REG_LR)
    rom=parity.bounded_read(Path(path),0x800000)
    parity.validate_rom(rom)
    payload,_=parity.decode(rom[0xd0970:],maximum=0x40000)
    assert parity.sha256(payload)==SHA
    base,cases=recipes()
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    cpu.mem_map(P,0x40000)
    cpu.mem_map(I,0x8000)
    calls=[]
    def external(m,a,size,user):
        off=a-P
        calls.append([off,[m.reg_read(r) for r in regs[:ARITY[off]]]])
        if off not in (0xd04,0x1534):
            m.reg_write(UC_ARM_REG_PC,m.reg_read(UC_ARM_REG_LR))
    for off in ARITY:
        cpu.hook_add(UC_HOOK_CODE,external,begin=P+off,end=P+off)
    def bios(m,n,user):
        pc=m.reg_read(UC_ARM_REG_PC)
        assert n==2 and pc-P in (0x11aea,0x11aee,0x11ae6)
        if pc-P==0x11ae6: # CpuSet used by allocator
            src,dst,n=(m.reg_read(r) for r in regs[:3])
            width=4 if n&(1<<26) else 2
            count=n&0x1fffff
            assert count<=32
            data=bytes(m.mem_read(src,width if n&(1<<24) else width*count))
            m.mem_write(dst,data*count if n&(1<<24) else data)
            return
        a,b=(int.from_bytes(m.reg_read(r).to_bytes(4,"little"),"little",signed=True) for r in regs[:2])
        q=int(a/b)
        m.reg_write(UC_ARM_REG_R0,q&0xffffffff)
        m.reg_write(UC_ARM_REG_R1,(a-q*b)&0xffffffff)
        m.reg_write(UC_ARM_REG_R3,abs(q))
    cpu.hook_add(UC_HOOK_INTR,bios)
    results={}
    for profile in ("synthetic","authentic"):
        cpu.mem_write(P,bytes(payload))
        if profile=="synthetic":
            for offset,data in synthetic():cpu.mem_write(P+offset,data)
        profile_outputs=[]
        for case in cases:
            cpu.mem_write(I,bytes(0x8000))
            for addr,width,value in [*base,*case["writes"]]:
                cpu.mem_write(addr,value.to_bytes(width,"little"))
            calls.clear()
            for off,*args in case["steps"]:
                for reg,value in zip(regs,args):cpu.reg_write(reg,value)
                cpu.reg_write(UC_ARM_REG_SP,I+0x7f00)
                cpu.reg_write(UC_ARM_REG_LR,END|1)
                cpu.emu_start(P+off+1,END,count=200000)
                assert cpu.reg_read(UC_ARM_REG_PC)==END,(case["name"],hex(cpu.reg_read(UC_ARM_REG_PC)))
            profile_outputs.append(dict(ram=parity.sha256(cpu.mem_read(I,0x7800)),calls=calls.copy()))
        results[profile]=profile_outputs
    return dict(romSha256=parity.sha256(rom),payloadSha256=SHA,sources={
        k:dict(space="garden",offset=a,length=b-a,sha256=parity.sha256(payload[a:b]))
        for k,(a,b) in SPANS.items()},syntheticTables=[[off,list(data)] for off,data in synthetic()],base=base,cases=cases,results=results)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",default=os.environ.get("SPP_ROM"))
    p.add_argument("--write",type=Path)
    a=p.parse_args()
    out=generate(a.rom)
    if a.write:a.write.write_text(json.dumps(out,separators=(",",":"))+"\n",encoding="utf8")
    elif json.loads(json.dumps(out))!=json.loads(FIXTURE.read_text(encoding="utf8")):raise SystemExit("Native shop vectors differ")
    print(f"{len(out['cases'])} scenarios in both profiles")


