"""Execute the A3VE Garden puzzle callbacks with synthetic task and board state."""
import argparse
import json
import os
from pathlib import Path

import reference_parity as parity
import garden_shop_reference as shop

P,I,T,K,G = shop.P,shop.I,shop.T,shop.K,shop.G
CHILD=shop.OBJ+36*5
MODE,BOARD=I+0x1ae6,I+0x1ae8
FIXTURE=Path(__file__).with_name("garden-puzzle-objects-vectors.json")
SPANS={a:b for a,b in [(0xd8a4,0xd9a8),(0xd9a8,0xda04),(0xda04,0xda58),(0xda58,0xdac0),
    (0xdac0,0xdb28),(0xdb28,0xdba8),(0xdba8,0xde24),(0xde24,0xdfbc),(0xdfbc,0xe02c),
    (0xe1f0,0xe3b4),(0xe3b4,0xe430),(0xe6d8,0xe888),(0xe888,0xe974),(0xf2dc,0xf404),
    (0xf688,0xf6bc),(0xf6bc,0xf72c),(0xf7dc,0xf814),(0xf85c,0xf888),(0xf888,0xf8d0),
    (0x2e117,0x2e11b),(0x2e0c0,0x2e108)]}
ARITY={0xd04:3,0xed4:1,0x1254:2,0x1398:2,0x1698:2,0x1066c:1,0x107a0:0,0x107d8:0,0x17b0:5}

SPANS.update({0xe02c:0xe08c,0xe08c:0xe134,0xe134:0xe1f0,0xe430:0xe6d8,
    0xf194:0xf218,0xf218:0xf2dc,0xf404:0xf518,0xf518:0xf624,0xf624:0xf688,
    0xf72c:0xf778,0xf778:0xf7dc,0xf814:0xf85c,0xf8d0:0xf8f8,0xf8f8:0xf928})
RETURNS=[0xea5c,0xed90,0xf010,0xef50,0xf0f0]
ARITY.update({0xeb08:2,0xeef8:0,0x16d8:0,0x12e0:0,0x1420:0,0xe28:0,
              0x163c:2,0xd504:0,**{off:2 if off==0xea5c else 0 for off in RETURNS}})

SPANS.update({0xeb08:0xeb9c,0xeb9c:0xecc8,0xecc8:0xed90,0xed90:0xeef8,0xeef8:0xef50,
              0xef50:0xf010,0xf010:0xf0f0,0xf0f0:0xf194,0x17194:0x171d4})
RULES=set(RETURNS)|{0xeb08,0xeef8}
RULE_RESULTS={0xecc8,0xed90,0xef50,0xf010,0xf0f0}

def recipes():
    base=[(T+4,1,2),(T+0x1b,1,5),(T+0x19,1,6),(T+0x1a,1,1),
          (T+0x1c,1,30),(T+0x1f,1,255),(T+0xa,2,104),(CHILD+4,1,5),
          (CHILD+0x1a,1,20),(CHILD+0xa,2,96),(CHILD+0x19,1,0),
          (I+0x606,1,4),(I+0x607,1,4),(T+6,1,1),(shop.OBJ+36+5,1,2),
          (MODE-2,1,18),(G+8,4,0x567c),(G+4,4,0x1234),(I+0x1ad1,1,1)]
    base.extend((I+0x5c0+i,1,i+2) for i in range(70))
    cases=[]
    def case(name,off,writes=(),steps=None):
        cases.append(dict(name=name,writes=list(writes),steps=steps if steps is not None else [[off,T]]))
    for count in (0,1,2,255):case("fade-"+str(count),0xf688,[(T+0x1a,1,count),(G+15,1,0xa3)])
    for off in (0xf6bc,0xdfbc,0xe3b4):
        for level in (0,1,2,3):case(f"setup-{off:x}-{level}",off,[(I+0x1ad1,1,level)])
    for flags,select,timer,pressed,held in [(0,0,0,8,8),(1,1,1,8,8),(16,1,0,8,8),(0,1,2,8,0),
        (128,1,1,1,1),(128,0,1,1,1),(128,1,0,2,2),(128,0,1,128,0),(128,1,1,64,0),
        (128,0,2,9,0),(128,1,0,0,2)]:
        case(f"pause-{flags}-{select}-{timer}-{pressed}",0xd8a4,[(G+15,1,flags),(T+0x19,1,select),(T+0x1a,1,timer),(K+4,2,pressed),(K,2,held)])
    for off in [0xd9a8,0xda04,0xda58,0xdac0,0xf7dc]:
        for flags,sel,lang in [(0,0,0),(128,0,0),(128,1,0),(128,0,1),(128,1,1)]:
            case(f"sprite-{off:x}-{flags}-{sel}-{lang}",off,[(G+15,1,flags),(CHILD+0x19,1,sel),(I+0x1ad0,1,lang)])
    for n in (0,1,10,99,100,999,1000,65535):
        case(f"digits-{n}",0,[(I+0x1ad2,2,n)],[[0xdb28],[0xdb28]])
    for state in [0,1,2,3,4,5,6,7,255]:
        for pressed,held in [(0,0),(3,0),(1,32),(0,16),(0,32)]:
            case(f"cursor-{state}-{pressed}-{held}",0xdba8,
                 [(T+0x16,1,state),(K+4,2,pressed),(K,2,held),(I+0x2a06,1,42)])
    for mode,flags in [(64,0),(128,0),(132,0),(64,16),(0,128)]:
        case(f"end-result-{mode}-{flags}",0xdba8,[(MODE,1,mode),(G+15,1,flags)])
    for col,held,n in [(1,32,1),(10,16,1),(1,32,2),(10,16,2),(2,32,1),(9,16,1)]:
        case(f"cursor-edge-{col}-{held}-{n}",0xdba8,[(T+0x16,1,3),(T+0x19,1,col),(T+0x1a,1,n),(K,2,held)])
    for state in range(8):
        for n,anim,special in [(0,0,0),(31,4,0),(6,2,0),(20,0,0),(7,1,1),(15,2,0)]:
            case(f"cursor-animation-{state}-{n}",0xde24,
                 [(CHILD+0x16,1,state),(T+0x1a,1,n),(T+0x19,1,anim),(I+0x29f8,1,special),(CHILD+7,1,1)])
    for n,timer,limit,idx,mode,pop,other in [(0,1,3,255,0,4,0),(0,1,3,2,0,4,0),
        (0,1,3,2,0,0,1),(0,1,3,2,0,0,0),(0,1,3,2,4,1,1),(14,1,3,2,0,1,0),
        (2,1,3,2,2,1,0),(0,2,3,2,0,1,0),(0,0,3,2,0,1,0)]:
        case(f"row-{n}-{timer}-{idx}-{mode}-{pop}-{other}",0xe1f0,[(I+0x29f0,2,n),(T+0x19,1,timer),
            (T+0x1a,1,limit),(T+0x1f,1,idx),(MODE,1,mode),(MODE-2,1,pop),(MODE-1,1,other),
            (BOARD+37,1,0x50),(I+0x1df0,1,77),(I+0x1df9,1,19)])
    case("bottom-occupied",0xe1f0,[(BOARD+118,1,2)])
    for off in (0xe6d8,0xe888):
        start=I+0x1b70 if off==0xe6d8 else BOARD
        tags=[0,1,2,0x10,0x18,0x50,0xa0,0xf0,0xff,0x31]
        changes=[(start+13+12*r+c,1,tags[(r+c)%10]) for r in range(10) for c in range(10)]
        case("tiles-"+hex(off),0,changes+[(I+0x2a30,1,2)],[[off]])
    for stop in [0,1,4,9,255]:
        case("detach-"+str(stop),0,[(BOARD+109,1,0x50),(BOARD+97,1,1),(BOARD+85,1,0x20),
            (BOARD+98,1,0xa0),(BOARD+86,1,0xf0)],[[0xf2dc,stop]])
    for i,denom in [(-1,20),(0,20),(19,20),(30,50),(-12,1)]:
        case(f"meter-{i}-{denom}",0,[(CHILD+0x1f,1,i%256),(CHILD+0x1a,1,denom)],[[0xf85c,T],[0xf888,T]])
    for off in (0xe134,0xe08c,0xe02c,0xf814):
        for flags,tag in [(0,0),(0,1),(128,0),(2,1)]:
            case(f"projectile-{off:x}-{flags}-{tag}",off,[(G+15,1,flags),(MODE,1,1),(T+0xe,2,155),
                 (T+0x19,1,5),(CHILD+0x19,1,5),(CHILD+7,1,tag),(BOARD+113,1,tag)])
    for off in (0xf218,0xf194):
        for n,y,dx in [(0,136,0),(0,145,12),(0,167,0),(0,168,0),(1,180,0),(6,0,3)]:
            case(f"particle-{off:x}-{n}-{y}",off,[(T+0x1a,1,n),(T+0xe,2,y),(T+0x19,1,3),
                 (I+0x29f6,2,104+dx),(I+0x1ad2,2,65535)])
    for state in range(7):
        for n,old,result in [(0,0,0),(1,1,0),(17,1,1),(1,0,1),(16,1,0),(19,1,0),(31,1,1),(7,1,0)]:
            case(f"resolve-{state}-{n}-{old}-{result}",0xe430,
                 [(T+0x16,1,state),(T+0x1a,1,n),(T+7,1,old),(T+0xe,2,4),(T+0xa,2,3),
                  (T+0x19,1,2)]+[(I+0x7000+i,1,result) for i in range(5)])
    for idx in [1,2,3,4]:
        case(f"match-branch-{idx}",0xe430,[(T+0x16,1,3 if idx in [2,3] else 4 if idx==4 else 5),
            (T+0x1a,1,0),(I+0x7000+idx,1,1)])
    for off in (0xf404,0xf518):
        for state,value,keys in [(0,0,0),(0,20,0),(1,7,0),(2,15,0),(3,0,0),(4,179,0),(4,0,8)]:
            case(f"banner-{off:x}-{state}-{value}-{keys}",off,[(T+0x16,1,state),(T+0xe,2,value),
                 (K+4,2,keys),(T+0x1a,1,0)])
        case(f"banner-sound-{off:x}",off)
    for round_ in range(4):
        case("win-"+str(round_),0,[(I+0x1ad1,1,round_)],[[0xf778,T]])
    case("loss",0xf72c)
    for amount,add,next_,n in [(40,5,0,15),(99998,5,0,15),(0xffffffff,2,0,15),
                               (1,1,P+0xf8f9,15),(1,1,0,0)]:
        case(f"return-{amount}-{next_}-{n}",0,[(I+0x6194,4,amount),(I+0x1ad2,2,add),
             (G+4,4,next_),(G+12,2,n)],[[0xf624]])
    for round_ in (0,2,3,255):
        case("advance-round-"+str(round_),0,[(I+0x1ad1,1,round_)],[[0xf8d0,T],[0xf8f8]])

    def rules(name,writes,steps):
        case(name,0,writes,steps)
        cases[-1]["rules"]=True
    for n in (0,1,8,15,16,31,127,128,255):
        for tag in (1,2):rules(f"palette-{n}-{tag}",[],[[0xeb08,tag,n]])
    for width in (1,8,9,10,11,20,90):
        cells=[(BOARD+12*(1+i//10)+1+i%10,1,1) for i in range(width)]
        roots=[(I+0x1bf4,1,1),(I+0x1bf8,1,1),(I+0x1bf9,1,1),(I+0x1bfa,1,1)]
        rules("group-"+str(width),cells+roots,[[0xed90],[0xeef8],[0xef50]])
    for reg in (0,4,12,65532):
        cells=[(BOARD+13,1,1),(BOARD+37,1,2),(BOARD+49,1,16),(BOARD+122,1,0xf0)]
        rules("settle-"+str(reg),cells+[(I+0x29f2,2,reg)],
              [[0xeef8],[0xef50],[0xf0f0],[0xf010],[0xf0f0],[0xf0f0],[0xf0f0],[0xef50]])
    # A placed ball closes a bracket; another variant completes a ten-cell
    # group and the match / collapse / recheck controller runs across frames.
    for cluster in (False,True):
        writes=[(T+0x16,1,0),(T+0x19,1,1),(T+0xe,2,4),(T+0xa,2,3),
                (BOARD+12*4+4,1,2),(BOARD+12*4+5,1,1)]
        if cluster:writes.extend((BOARD+12*3+i,1,1) for i in range(1,11))
        rules("shot-resolution-chain-"+str(cluster),writes,[[0xe430,T]]*240)
        cases[-1]["stopWhenRemoved"]=T
    for seed in range(12):
        writes=[]
        for row in range(1,10):
            for col in range(1,11):
                value=(row*17+col*31+seed*7+row*col)%13
                writes.append((BOARD+12*row+col,1,[0,0x50,16,2,1,1,1,1,1,1,1,1,1][value]))
        writes.extend([(I+0x1bf4,1,3),(I+0x1bf8,1,2),(I+0x1bf9,1,2),(I+0x1bfa,1,1),
                       (I+0x1bfb,1,4),(I+0x1bfc,1,5),(I+0x1bfd,1,1),
                       (I+0x1bfe,1,6),(I+0x1bff,1,7),(I+0x1c00,1,2)])
        rules("mixed-board-"+str(seed),writes,[[0xed90],[0xeef8],[0xf010],[0xef50]])

    return base,cases

def generate(rom_path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,
                                   UC_ARM_REG_PC,UC_ARM_REG_LR,UC_ARM_REG_SP)
    rom=parity.bounded_read(Path(rom_path),0x800000)
    parity.validate_rom(rom)
    payload,_=parity.decode(rom[0xd0970:],maximum=0x40000)
    assert parity.sha256(payload)==shop.SHA
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    cpu.mem_map(P,0x40000)
    cpu.mem_map(I,0x8000)
    cpu.mem_map(0x04000000,0x1000)
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    log=[]
    def service(m,pc,size,user):
        off=pc-P
        if case.get("rules") and off in RULES:return
        args=[m.reg_read(r) for r in regs[:ARITY[off]]]
        if ARITY[off]>4:args.append(int.from_bytes(m.mem_read(m.reg_read(UC_ARM_REG_SP),4),"little"))
        log.append([off,args])
        if off in RETURNS:m.reg_write(regs[0],int.from_bytes(m.mem_read(I+0x7000+RETURNS.index(off),1),"little"))
        if off!=0xd04:m.reg_write(UC_ARM_REG_PC,m.reg_read(UC_ARM_REG_LR))
    for off in ARITY:cpu.hook_add(UC_HOOK_CODE,service,begin=P+off,end=P+off)
    def bios(m,n,user):
        pc=m.reg_read(UC_ARM_REG_PC)-P
        a,b,ctrl=[m.reg_read(r) for r in regs[:3]]
        if pc==0x11ae2:
            assert n==2 and ctrl==0x1000140
            m.mem_write(b,bytes(m.mem_read(a,4))*320)
        elif pc==0x11ae6:
            assert n==2 and ctrl==0x1000280
            m.mem_write(b,bytes(m.mem_read(a,2))*640)
        elif pc==0x11aea:
            a=int.from_bytes(a.to_bytes(4,"little"),"little",signed=True)
            q=int(a/b)
            m.reg_write(regs[0],q&0xffffffff)
            m.reg_write(regs[1],(a-q*b)&0xffffffff)
            m.reg_write(regs[3],abs(q))
        else:raise AssertionError(hex(pc))
    cpu.hook_add(UC_HOOK_INTR,bios)
    base,cases=recipes()
    tables=[[0x2e117,[3,9,2,5]],[0x2e0c0,list(range(21,93))]]
    tables.append([0x17194,[(i*17+63)%256 for i in range(64)]])
    outputs={}
    for profile in ("synthetic","authentic"):
        cpu.mem_write(P,bytes(payload))
        if profile=="synthetic":
            for off,data in tables:cpu.mem_write(P+off,bytes(data))
        expected=[]
        for case in cases:
            cpu.mem_write(0x04000054,bytes(2))
            cpu.mem_write(I,bytes(0x8000))
            for a,width,v in [*base,*case["writes"]]:cpu.mem_write(a,v.to_bytes(width,"little"))
            log.clear()
            results=[]
            executed=0
            for off,*args in case["steps"]:
                executed+=1
                for reg,v in zip(regs,args):cpu.reg_write(reg,v)
                cpu.reg_write(UC_ARM_REG_SP,I+0x7f00)
                cpu.reg_write(UC_ARM_REG_LR,shop.END|1)
                cpu.emu_start(P+off+1,shop.END,count=200000)
                assert cpu.reg_read(UC_ARM_REG_PC)==shop.END,case["name"]
                if off in RULE_RESULTS:results.append(cpu.reg_read(regs[0]))

                if "stopWhenRemoved" in case and [0xed4,[case["stopWhenRemoved"]]] in log:break

            if "stopWhenRemoved" in case:assert [0xed4,[case["stopWhenRemoved"]]] in log,case["name"]
            expected.append(dict(executed=executed,returns=results,ram=parity.sha256(cpu.mem_read(I,0x7800)),calls=log.copy(),brightness=int.from_bytes(cpu.mem_read(0x04000054,2),"little")))
        outputs[profile]=expected
    return dict(romSha256=parity.sha256(rom),payloadSha256=shop.SHA,sources=[
        dict(space="garden",offset=a,length=b-a,sha256=parity.sha256(payload[a:b])) for a,b in SPANS.items()],
        syntheticTables=tables,base=base,cases=cases,results=outputs)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",default=os.environ.get("SPP_ROM"))
    p.add_argument("--write",type=Path)
    a=p.parse_args()
    out=generate(a.rom)
    if a.write:a.write.write_text(json.dumps(out,separators=(",",":"))+"\n",encoding="utf8")
    elif json.loads(json.dumps(out))!=json.loads(FIXTURE.read_text(encoding="utf8")):raise SystemExit("Puzzle vectors differ")
    print(f"{len(out['cases'])} scenarios in both profiles")



















