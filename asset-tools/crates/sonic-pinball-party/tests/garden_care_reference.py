"""Run the A3VE embedded Chao frame and movement instructions on synthetic state.

All 64 switch states, the initializer and the selector execute their original
instructions. BIOS division uses signed truncation; song-start records its ID.
Task allocation, dialog drawing and queue writes execute with native instructions.
Only synthetic inputs, result hashes, call IDs and source-span identities are emitted.
"""
import argparse
import json
import os
import struct
import math
import math_reference
from pathlib import Path

import reference_parity as parity

FIXTURE=Path(__file__).with_name("garden-care-vectors.json")
BASE=0x02000000
RAM=0x03000000
TASK=RAM+0x5400
SAVE=RAM+0x4aa0
TIMERS=RAM+0x1518
FLAGS=RAM+0x425f
END=RAM+0x7000
PAYLOAD_SHA="33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
PORTED={0x46d4,0x46e0,0x4756,0x47ac,0x4808,0x482a,0x484a,0x487c,0x48ac,
        0x48ba,0x48e0,0x4910,0x4a9c,0x4b44,0x4b52,0x4c94,0x4e40,0x4e68,
        0x4e90,0x4e9e,0x5676,0x5698,0x56d0,0x5708,0x5868,0x58a0,0x58dc,0x5b08,
        0x493c,0x4a68,0x4ac0,0x4aca,0x5350,0x545c,0x4b90,0x4bc6,0x4d94,0x4ee4,
        0x508a,0x50e8,0x51c4,0x51f4,0x52e4,0x5540,0x55f0,0x5740,0x5774,0x5908,
        0x5a3c,0x5b3e,0x5bf8,0x5d6e}
SOURCES={"frameTimersAndSwitch":(0x4418,0x45d4),"stateTargets":(0x45d4,0x46d4),
         "firstActions":(0x46d4,0x493c),"timerAction":(0x4a9c,0x4ac0),
         "repeatedAction":(0x4b44,0x4b90),"walkAction":(0x4c94,0x4d94),"timedActions":(0x4e40,0x4ee4),
         "directionalTimers":(0x5676,0x5740),"heightAndMapActions":(0x5860,0x5908),
         "resetAndSelect":(0x5b08,0x5b3e),"frameWriteback":(0x5e10,0x5e58),
         "movement":(0x5e58,0x5efc),"selection":(0x5efc,0x6554),
         "directionChoice":(0x6554,0x65ec),"rng":(0x1534,0x1588),
         "trig":(0x182c,0x185c),"trigTable":(0x1f2e0,0x1f4e0),
         "directionAngles":(0x20672,0x2067e),"foodEffects":(0x1269c,0x126cd),
         "applyFood":(0x493c,0x4a9c),"careCooldown":(0x4ac0,0x4b44),
         "approachContact":(0x5350,0x5540),"allocateTask":(0xd04,0xdb0),
         "allActions":(0x46d4,0x5e10),"dialogBuilder":(0x270c,0x292c),
         "initializeChao":(0x41e0,0x4418),"immediateCallback":(0x1249c,0x1249e)}
def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3,
        UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP)
    rom=parity.bounded_read(Path(path),0x800000)
    parity.validate_rom(rom)
    payload,_=parity.decode(rom[0xd0970:],maximum=0x40000)
    assert parity.sha256(payload)==PAYLOAD_SHA
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    cpu.mem_map(BASE,0x40000);cpu.mem_map(RAM,0x8000)
    cpu.mem_write(BASE,bytes(payload))
    targets=[addr-BASE for addr in struct.unpack_from("<64I",payload,0x45d4)]
    stop=set(targets)-PORTED
    assert not stop, "every native action target must be implemented"
    calls=[]
    stops=[]
    def boundary(machine,addr,size,user):
        stops.append(addr-BASE)
        machine.emu_stop()
    for o in stop:cpu.hook_add(UC_HOOK_CODE,boundary,begin=BASE+o,end=BASE+o)
    def song(machine,addr,size,user):
        calls.append([addr-BASE,[machine.reg_read(UC_ARM_REG_R0)]])
        machine.reg_write(UC_ARM_REG_PC,machine.reg_read(UC_ARM_REG_LR))
    cpu.hook_add(UC_HOOK_CODE,song,begin=BASE+0x1066c,end=BASE+0x1066c)
    def allocation(machine,addr,size,user):
        calls.append([addr-BASE,[machine.reg_read(r) for r in
                      (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2)]])
    cpu.hook_add(UC_HOOK_CODE,allocation,begin=BASE+0xd04,end=BASE+0xd04)
    def queue(machine,addr,size,user):
        regs=(UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3)
        calls.append([addr-BASE,[machine.reg_read(r) for r in regs]+[
            int.from_bytes(machine.mem_read(machine.reg_read(UC_ARM_REG_SP),4),"little")]])
    cpu.hook_add(UC_HOOK_CODE,queue,begin=BASE+0x17b0,end=BASE+0x17b0)
    def div(machine,intno,user):
        assert intno==2
        pc=machine.reg_read(UC_ARM_REG_PC)-BASE
        def signed(x):return (x^0x80000000)-0x80000000
        a=signed(machine.reg_read(UC_ARM_REG_R0)); b=signed(machine.reg_read(UC_ARM_REG_R1))
        if pc==0x11ade:
            machine.reg_write(UC_ARM_REG_R0,math_reference.atan2_reference(a,b));return
        if pc==0x11b02:
            machine.reg_write(UC_ARM_REG_R0,math.isqrt(a&0xffffffff));return
        assert pc in (0x11aea,0x11aee),hex(pc)
        q=int(a/b);r=a-q*b
        for reg,v in [(UC_ARM_REG_R0,q),(UC_ARM_REG_R1,r),(UC_ARM_REG_R3,abs(q))]:
            machine.reg_write(reg,v&0xffffffff)
    cpu.hook_add(UC_HOOK_INTR,div)
    default=[[TASK+8,4,88<<16|0x1000],[TASK+12,4,104<<16|0x2000],
        [TASK+0x20,2,64],[TASK+0x22,2,192],[TASK+0x16,1,4],[TASK+0x19,1,2],[TASK+0x1a,1,1],
        [TASK+0x1f,1,1],[TASK+0x14,2,2],[SAVE+0x73,1,10],[SAVE+0x74,1,109],
        [SAVE+0x75,1,199],[SAVE+0x76,1,200],[SAVE+0xd0,4,30],[RAM+0x159b,1,4],
        [RAM+0x1510,4,TASK],[RAM+0x1514,4,RAM+0x5560],[RAM+0x5c0,4,0x08070504],
        [RAM+0x5c4,4,0x0c0b0a09],[TASK+4,1,6],
        [RAM+0x1940,4,0x12121212],[RAM+0x1944,4,0x56565656],
        [RAM+0x13a8,4,1]]+[[TIMERS+2*i,2,1] for i in range(8)]
    cases=[]
    def record(label,offset,patches,ticks=1):
        cpu.mem_write(RAM,bytes(0x8000))
        cpu.mem_write(BASE+0x3f800,bytes(0x800))
        for addr,width,value in default+patches:
            cpu.mem_write(addr,value.to_bytes(width,"little"))
        trace=[]
        for tick in range(ticks):
            calls.clear();stops.clear()
            cpu.reg_write(UC_ARM_REG_SP,RAM+0x7f00)
            cpu.reg_write(UC_ARM_REG_R0,TASK);cpu.reg_write(UC_ARM_REG_LR,END|1)
            cpu.emu_start(BASE+offset+1,END,count=200000)
            pc=cpu.reg_read(UC_ARM_REG_PC)
            assert (pc==END and not stops) or (len(stops)==1 and pc==BASE+stops[0]),(label,tick,hex(pc))
            trace.append(dict(sha256=parity.sha256(cpu.mem_read(RAM,0x7000)),tiles=parity.sha256(cpu.mem_read(BASE+0x3f800,0x800)),
                    calls=calls.copy(),stop=stops[0] if stops else None))
            if stops:break
        cases.append(dict(name=label,entry=offset,patches=patches,trace=trace))
    for s in range(65):
        for flags in (0,32):
            record(f"action-{s}-flags-{flags}",0x4418,[[TASK+0x17,1,s],[FLAGS,1,flags]])
    for i,v in enumerate([0,109,110,189,190,200,255]):
        for count in (0,3,4,255):
            record(f"needs-{v}-{count}",0x4418,[[SAVE+0x74,1,v],[RAM+0x159b,1,count],
                   [SAVE+0x73,1,i%2],[SAVE+0x75,1,200+i],[SAVE+0x76,1,198+i]])
    for flag in (2,16,128,0x92,0xb2):
        record("mask-"+str(flag),0x4418,[[FLAGS,1,flag]])
    for s in (0,1,2,3,4,5,7,8,9,10,11,12,15,18,19,22,24,28,29,43,45,46,48,53,54,55,59):
        record("sequence-"+str(s),0x4418,[[TASK+0x17,1,s],[TASK+0x14,2,1],
                [TASK+0x1a,1,127],[TASK+0x19,1,8]],5)
    record("paused-high-position",0x4418,[[FLAGS,1,32],[SAVE+0x58,1,255]],4)
    record("paused-low-position",0x4418,[[FLAGS,1,32],[SAVE+0x58,1,255],[TASK+14,2,60]],4)
    record("zero-timers",0x4418,[[TIMERS+2*i,2,0]for i in range(8)])
    for item in range(7):
        for scenario,(need,progress,level,total) in enumerate([
                (0,0,0,0),(176,95,20,32),(199,99,98,9998),(93,200,99,9999)]):
            p=[[TASK+0x17,1,13],[TASK+0x1b,1,item],[SAVE+0x73,1,need],[SAVE+0x74,1,need]]
            for i in range(5):
                p.extend([[SAVE+0x78+i,1,progress],[SAVE+0x80+i,1,i+1],[SAVE+0x88+i,1,level],
                          [SAVE+0x90+2*i,2,(total%256)*256+total//256]])
            record(f"food-{item}-{scenario}",0x4418,p,2)
    for s in (14,15,16,17):
        for v in (195,250):
            for cd in (0,2):
                record(f"cooldown-{s}-{v}-{cd}",0x4418,[[TASK+0x17,1,s],[TASK+0x14,2,1],
                       [SAVE+0x73,1,v],[TIMERS+14,2,cd]],2)
    for s in (36,37):
        for absent in (False,True):
            for facing in (0,128):
                obj=RAM+0x5320+0x24*3
                record(f"approach-{s}-{absent}-{facing}",0x4418,[[TASK+0x17,1,s],[TASK+0x14,2,1],
                       [TASK+0x1a,1,facing],[RAM+0x1940,1,int(not absent)],[RAM+0x1508,1,3],
                       [obj+0x19,1,2],[obj+0x1a,1,4],[RAM+0x159a,1,1],[SAVE+0x5c,1,4],
                       [RAM+0x1514,4,0 if absent else RAM+0x5560],
                       [RAM+0x556a,2,70 if facing else 100]],4)
    for state in (0,22,23):
        for energy,need in ((0,0),(1,0),(1,200)):
            for seed in (1,17,0xffffffff):
                record(f"selection-{state}-{energy}-{need}-{seed}",0x5efc,[[TASK+0x17,1,state],
                    [SAVE+0xd0,4,energy],[SAVE+0x75,1,need],[RAM+0x13a8,4,seed]])
    for angle in range(12):
        record(f"heading-{angle}",0x6570,[[TASK+0x16,1,angle],[RAM+0x1507,1,angle%2]])
    for i,(x,y) in enumerate(((0,0),(19,31),(20,32),(156,152),(157,153),(0x7fff,0x8000))):
        record("move-"+str(i),0x5e58,[[TASK+8,4,x<<16|0xffff],[TASK+12,4,y<<16],
            [TASK+0x22,2,0x8000],[TASK+0x20,2,0x7fff],[TASK+0x16,1,i*2]],8)
    # Exercise the weighted selector and both view-cone tests across native
    # inputs. Reuse the first seven bytes of synthetic position bits only.
    for i in range(144):
        seed=(i*0xb5297a4d+0x68e31da4)&0xffffffff
        patches=[[TASK+0x17,1,22],[RAM+0x13a8,4,seed],[TASK+0x16,1,i%12],
            [SAVE+0x75,1,i%200],[SAVE+0x74,1,(i*37)%201],[SAVE+0x73,1,(i*17)%201],
            [SAVE+0x76,1,(i*23)%201],[SAVE+0x92,2,0xb400 if i%2 else 0],
            [SAVE+0xcd,1,i%3],[SAVE+0xce,1,i%2],[SAVE+0xcf,1,i%4],
            [SAVE+0x21,1,88+i%10],[SAVE+0x22,1,100+i%8],
            [SAVE+0x35,1,80+i%20 if i%4 else 0],[SAVE+0x36,1,101+i%5]]
        record("weighted-selector-"+str(i),0x5efc,patches)
    # Choose one original nonzero collision tile in the walkable bounds.
    blocked=next((x,y) for y in range(32,153,8) for x in range(20,157,8)
                 if payload[0x216a0+(y//8)*22+(x//8)])
    record("walking-obstacle",0x4418,[[TASK+0x17,1,22],[TASK+10,2,blocked[0]],[TASK+14,2,blocked[1]]])
    for lang in range(5):
        for state in (30,32,34,39,41):
            for i in range(3):
                p=[[TASK+0x17,1,state],[RAM+0x4260,1,lang],[TASK+10,2,88 if state==32 else (32+i*48)],
                   [TASK+14,2,87 if state==32 else (74+i*26)],[TASK+0x19,1,121+i*2],
                   [RAM+0x13a8,4,(0x8914+i*0x21523)&0xffffffff],[RAM+0x5314,2,1]]
                if state==30 and i:
                    p += [[SAVE+0xc0+k,1,255] for k in range(12)]
                    p += [[SAVE+0xcb,1,0xdf if i==1 else 255],[SAVE+0xcc,1,93 if i==1 else 94]]
                record(f"dialog-{lang}-{state}-{i}",0x4418,p,3)
    for s in (20,21,23,31,33,34,35,38,40,42):
        for i in range(4):
            record(f"contact-expiry-{s}-{i}",0x4418,[[TASK+0x17,1,s],[TASK+0x19,1,i%2],
                   [TASK+0x1a,1,1 if s!=23 else 29],[TASK+0x14,2,1 if i!=2 else 90],
                   [TASK+7,1,128 if i%2 else 0],[RAM+0x5314,2,1],[SAVE+0x62,1,i+1],
                   [TASK+10,2,0xfff1 if s==34 else 88],[SAVE+0x75,1,i*40],
                   [RAM+0x13a8,4,1+i*4]],3)
    for i in range(96):
        s=[51,52,56,57,58,61,62,63][i%8];j=i//8
        x,y=(blocked if j%3==0 else ((15,155) if j%3==1 else (89,112)))
        height=[0,0xe800,0xf700,0x1000][j%4]
        record(f"water-{s}-{j}",0x4418,[[TASK+0x17,1,s],[TASK+0x1a,1,[0,1,2,60][j%4]],
               [TASK+0x14,2,1],[TASK+0x1f,1,([0,0xe8,0xf7,16][j%4])],[RAM+0x1502,2,height],
               [RAM+0x1504,2,[0,0xff00,256][j%3]],[RAM+0x4258,4,j%4],
               [TASK+10,2,x],[TASK+14,2,y],[TASK+0x22,2,0xf003 if j%2 else 0x2001],
               [TASK+0x20,2,0xf003 if j%3 else 0x2001],[SAVE+0x90,2,0x6400 if j%2 else 0],
               [FLAGS,1,32 if j==10 else 0]],4)
    for i in range(18):
        pos=blocked if i%3==0 else ((0,0) if i%3==1 else (40,68))
        p=[[SAVE+0x58,1,255 if i==0 else 1],[SAVE+0x72,1,[0,7,12][i%3]],
           [SAVE+0x71,1,i*2],[SAVE+0x1d,1,pos[0]],[SAVE+0x1e,1,pos[1]],
           [RAM+0x152d,1,[0,21,51,58,13,22][i%6]]]
        p+=[[SAVE+0xa0+k,1,(k*17+i)%256] for k in range(32)]
        record(f"chao-init-{i}",0x41e0,p)
    return dict(romSha256=parity.sha256(rom),payloadSha256=PAYLOAD_SHA,
          sources={k:dict(space="garden",offset=a,length=b-a,sha256=parity.sha256(payload[a:b]))
                   for k,(a,b) in SOURCES.items()},defaults=default,cases=cases)
if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",default=os.environ.get("SPP_ROM"));p.add_argument("--write",type=Path)
    a=p.parse_args();data=generate(a.rom)
    if a.write:a.write.write_text(json.dumps(data,separators=(",",":"))+"\n",encoding="utf8")
    else:assert data==json.loads(FIXTURE.read_text())
    print(json.dumps({"cases":len(data["cases"]),"ticks":sum(len(c["trace"]) for c in data["cases"])}))
