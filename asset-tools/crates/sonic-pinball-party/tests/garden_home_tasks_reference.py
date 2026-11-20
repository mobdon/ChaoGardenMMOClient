"""Execute the A3VE home object tasks on synthetic RAM in the native-reference harness."""
import argparse, json, math, os
from pathlib import Path
import reference_parity as parity

BASE=0x02000000
RAM=0x03000000
TASK=RAM+0x5320+36*4
PARENT=RAM+0x5320+36*6
SAVE=RAM+0x4aa0
FIXTURE=Path(__file__).with_name("garden-home-tasks-vectors.json")
SPAN={0x6a54:0x7240,0x7240:0x73a8,0x73a8:0x74c0,0x8a74:0x8ab8,0x8ab8:0x8b1c,
      0x3424:0x3534,0x35cc:0x3614,0x74c0:0x7504,0x89b0:0x89d4,0x8a04:0x8a2c,
      0x89d4:0x8a04,0x218c:0x21f4,0x88b0:0x88f8,0x83e4:0x84ec,0x84ec:0x85e0,
      0x3e04:0x3eb4,0x3eb4:0x3f68,0x8b1c:0x8b3c,0x292c:0x2eb8,0x3748:0x3c24,
      0x3614:0x36ec,0x36ec:0x3748,0x3c24:0x3d58,0x3d58:0x3e04,0x85e0:0x8648,
      0x8648:0x86b4,0x8d90:0x8dec}
def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,
                                  UC_ARM_REG_PC,UC_ARM_REG_LR,UC_ARM_REG_SP)
    rom=parity.bounded_read(Path(path),0x800000)
    parity.validate_rom(rom)
    p,_=parity.decode(rom[0xd0970:],maximum=0x40000)
    assert parity.sha256(p)=="33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    cpu.mem_map(BASE,0x40000);cpu.mem_map(RAM,0x8000);cpu.mem_write(BASE,bytes(p))
    regs=(UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3)
    calls=[]
    def hook(machine,addr,_,n):
        off=addr-BASE
        args=[machine.reg_read(r) for r in regs[:n]]
        if n==5:args.append(int.from_bytes(machine.mem_read(machine.reg_read(UC_ARM_REG_SP),4),"little"))
        calls.append([off,args])
        if off in (0x1066c,0x107a0,0x10804):machine.reg_write(UC_ARM_REG_PC,machine.reg_read(UC_ARM_REG_LR))
    for off,n in [(0xd04,3),(0x1398,2),(0x1254,2),(0xed4,1),(0x1698,2),(0x1066c,1),(0x107a0,0),(0x10804,2),(0x17b0,5)]:
        cpu.hook_add(UC_HOOK_CODE,hook,n,begin=BASE+off,end=BASE+off)
    def bios(machine,no,_):
        assert no==2
        pc=machine.reg_read(UC_ARM_REG_PC)-BASE
        def signed(n):return (n^0x80000000)-0x80000000
        a=signed(machine.reg_read(regs[0]));b=signed(machine.reg_read(regs[1]))
        if pc==0x11b02:machine.reg_write(regs[0],math.isqrt(a&0xffffffff));return
        assert pc in (0x11aea,0x11aee),hex(pc)
        q=int(a/b)
        for r,v in [(regs[0],q),(regs[1],a-q*b),(regs[3],abs(q))]:machine.reg_write(r,v&0xffffffff)
    cpu.hook_add(UC_HOOK_INTR,bios)
    default=[[RAM+0x152e,1,1],[RAM+0x1510,4,PARENT],[RAM+0x150c,4,PARENT],
            [TASK+4,1,4],[TASK+5,1,3],[TASK+6,1,5],[TASK+0x1b,1,6],[PARENT+4,1,6],
            [PARENT+10,2,104],[PARENT+14,2,113],[PARENT+0x18,1,109],[PARENT+0x1d,1,111],
            [PARENT+0x19,1,12],[TASK+0x19,1,4],[TASK+0x1a,1,1],[TASK+0x1d,1,80],
            [RAM+0x606,1,9],[RAM+0x5c0,1,8],[RAM+0x127a,1,1],[RAM+0x13a8,4,1],
            [SAVE+0x1f,1,88],[SAVE+0x20,1,104],[SAVE+0x33,1,64],[SAVE+0x34,1,159]]
    cases=[]
    def record(name,entry,patches,ticks=1):
        cpu.mem_write(RAM,bytes(0x8000))
        for a,w,v in default+patches:cpu.mem_write(a,v.to_bytes(w,"little"))
        trace=[]
        for tick in range(ticks):
            calls.clear();cpu.reg_write(UC_ARM_REG_SP,RAM+0x7f00)
            cpu.reg_write(UC_ARM_REG_R0,TASK);cpu.reg_write(UC_ARM_REG_LR,RAM+0x7001)
            cpu.emu_start(BASE+entry+1,RAM+0x7000,count=100000)
            assert cpu.reg_read(UC_ARM_REG_PC)==RAM+0x7000,(name,tick,hex(cpu.reg_read(UC_ARM_REG_PC)))
            trace.append([parity.sha256(cpu.mem_read(RAM,0x7000)),calls.copy()])
        cases.append(dict(name=name,entry=entry,patches=patches,trace=trace))
    for s in range(64):
        for j in range(6):
            record(f"body-{s}-{j}",0x6a54,[[PARENT+0x17,1,s],[PARENT+0x16,1,[0,3,6,9,11,2][j]],
                   [PARENT+0x19,1,[0,1,3,63,144,195][j]],[TASK+0x22,1,[0,8,17,11,9,1][j]],[TASK+0x23,1,j],
                   [PARENT+0x1a,1,j*42],[PARENT+0x22,2,0x8000 if j%2 else 23],
                   [PARENT+7,1,128 if j%2 else 0],[RAM+0x4258,4,[32,3,24,128,0,64][j]]],2)
    for s in (0x7240,0x73a8,0x8a74,0x8ab8,0x6a54):
        for j in range(16):
            record(f"part-{s:x}-{j}",s,[[PARENT+7,1,[0,4,8,16,32,64,128,1][j%8]],
                [PARENT+0x17,1,[0,57,61,23][j%4]],[PARENT+0x19,1,(j*13)%67],
                [PARENT+0x1e,1,j%2],[PARENT+0x14,2,474+j],[TASK+0x19,1,j],[TASK+0x1a,1,[4,10,179,0][j%4]],
                [TASK+7,1,j%2],[RAM+0x1506,1,j%2],[RAM+0x152e,1,int(j!=12)],
                [RAM+0x4258,4,j*10],[RAM+0x425f,1,16 if j==11 else (128 if j==13 else 0)]],2 if j!=12 else 1)
    for entry in SPAN:
        if entry in (0x6a54,0x7240,0x73a8,0x8a74,0x8ab8):continue
        for j in range(4):
            record(f"home-{entry:x}-{j}",entry,[[TASK+0x1a,1,j%3],[SAVE+16,4,1 if j<2 else 144],
                [RAM+0x425f,1,[0,0,16,128][j]],[RAM+0x5314,2,8],[TASK+0x19,1,j],
                [RAM+0x4258,4,[71,37,16,0][j]],[RAM+0x13a8,4,j+1]])
    for n in range(42):
        changes=[[RAM+0x6194,4,[0,123,99999,32,65343,1,20000][n%7]],[TASK+0x20,4,[345,1,99999][n%3]],
           [RAM+0x152f,1,[0,1,255][n%3]],[TASK+7,1,[0,1,1,255][n%4]],[TASK+0x1a,1,1],
           [SAVE+0x71,1,n],[RAM+0x4260,1,n%5],[RAM+0x4258,4,n],[PARENT+10,2,188 if n%2 else 182],
           [PARENT+14,2,n%10],[RAM+0x5314,2,n%2],[RAM+0x5310,2,16 if n%5==0 else 0]]
        for i in range(7):
            changes.extend([[SAVE+0x73+i,1,10+i],[SAVE+0x88+i,1,10+i],[RAM+0x155b+8*i,1,[9,99,0][n%3]],
                      [RAM+0x155a+8*i,1,9],[RAM+0x155c+8*i,1,1],[SAVE+0x6a+i,1,i+1]])
        record(f"hud-{n}",0x292c,changes,3)
    for j in range(16):
        for held in (0,1):
            record(f"move-{j}-{held}",0x3748,[[RAM+0x5310,2,j<<4],[TASK+7,1,held],
                   [TASK+8,4,(14 if j%2 else 211)<<16],[TASK+12,4,(141 if j%3 else 5)<<16],
                   [TASK+0x22,2,890],[TASK+0x20,2,630]],3)
    for k in range(8):
        for j in range(4):
            dx,_,dy,_=__import__("struct").unpack_from("bbbb",p,0x1fe68+4*k)
            record(f"click-{k}-{j}",0x3748,[[RAM+0x5314,2,1],[RAM+0x6190,1,1],[RAM+0x5d40,1,6],
                   [TASK+10,2,104+dx+8],[TASK+14,2,113+dy-8],[PARENT+0x1c,1,k],[PARENT+0x1e,1,3],
                   [PARENT+0x17,1,[0,23,34,16][j]],[PARENT+7,1,j*64],[PARENT+0x1a,1,j],
                   [RAM+0x152f,1,j%2],[SAVE+0x58,1,j%2]])
    for off in (0x3c24,0x3d58,0x8648,0x8d90,0x85e0,0x36ec,0x3614):
        for i in range(16):
            record(f"follow-{off:x}-{i}",off,[[TASK+0x19,1,i%8],[TASK+0x1a,1,[0,7,15,63,66,129,127,190][i%8]],
                   [TASK+0x1d,1,109],[TASK+14,2,60 if i%2 else 130],[TASK+0x14,2,402],[TASK+7,1,i%2],
                   [TASK+0x22,2,0xff80],[TASK+0x1e,1,254],[TASK+0x1f,1,23]],1)
    return dict(romSha256=parity.sha256(rom),payloadSha256=parity.sha256(p),space="garden",
       sources={f"{a:04x}":dict(offset=a,length=b-a,sha256=parity.sha256(p[a:b])) for a,b in SPAN.items()},
       defaults=default,cases=cases)
if __name__=="__main__":
    a=argparse.ArgumentParser(description=__doc__);a.add_argument("--rom",default=os.getenv("SPP_ROM"))
    a.add_argument("--write",type=Path);args=a.parse_args();d=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(d,separators=(",",":"))+"\n",encoding="utf8")
    else:assert d==json.loads(FIXTURE.read_text())
    print(json.dumps(dict(cases=len(d["cases"]),ticks=sum(len(x["trace"]) for x in d["cases"]))))
