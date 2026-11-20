"""Original A3VE sound-routing helpers, with explicit unported MPlay boundary.

The oracle executes local ROM instructions. Only MPlayStart/MPlayStop are
intercepted and recorded; no sequencer, mixer or playback parity is claimed.
Fixtures contain synthetic scalar state, native addresses and hashes, not ROM
instructions, waveforms or native saves. Requires Python unicorn separately.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import spp_vectors  # noqa: E402

ROM_SHA256="5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES={"request":(0x37D4,0x78),"force":(0x384C,0x50),"status":(0x38FC,0x24),"force-channel2":(0x38D4,0x14)}

def generate(path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_PC,UC_ARM_REG_SP,UC_ARM_REG_LR
    with open(path,"rb") as f: rom=f.read(0x800001)
    if len(rom)!=0x800000 or hashlib.sha256(rom).hexdigest()!=ROM_SHA256: raise ValueError("Exact local A3VE rev0 required")
    uc=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for base,length in [(0x08000000,0x800000),(0x02000000,0x40000),(0x03000000,0x8000)]: uc.mem_map(base,length)
    uc.mem_write(0x08000000,rom)
    players=list(struct.unpack_from("<4I",rom,0x6DD41C))
    headers=list(struct.unpack_from("<135I",rom,0x6DD42C))
    calls=[]
    def hook(cpu,address,size,data):
        if address in (0x0805F170,0x0805F254):
            calls.append(["start" if address==0x0805F170 else "stop",cpu.reg_read(UC_ARM_REG_R0),cpu.reg_read(UC_ARM_REG_R1) if address==0x0805F170 else 0])
            cpu.reg_write(UC_ARM_REG_PC,cpu.reg_read(UC_ARM_REG_LR))
    uc.hook_add(UC_HOOK_CODE,hook)
    rng=random.Random(0xA3E5384C)
    specs=[]
    for routine in ROUTINES:
        for channel in [0,1,2,3,4,255,256,0x123401,0xFFFFFFFF]:
            for song in [0,1,134,135,65536,0xFFFFFFFF]: specs.append((routine,channel,song))
    specs.extend((rng.choice(list(ROUTINES)),rng.getrandbits(32),rng.getrandbits(32)) for _ in range(96))
    specs.extend((routine,channel,song) for routine in ["request","force"] for channel in range(4) for song in [1,2,3,53,134])
    cases=[]
    for index,(routine,channel,song) in enumerate(specs):
        current=[rng.randrange(135) for _ in range(4)]
        status=[rng.choice([0,1,0x8000,0xFFFF]) for _ in range(4)]
        if channel<4 and index%2==0: current[channel]=song&0xFFFF
        uc.mem_write(0x030007C8,struct.pack("<4H",*current))
        for p,value in zip(players,status): uc.mem_write(p+4,struct.pack("<H",value))
        calls.clear()
        uc.reg_write(UC_ARM_REG_SP,0x03007D00)
        uc.reg_write(UC_ARM_REG_LR,0x03007F01)
        uc.reg_write(UC_ARM_REG_R0,song if routine=="force-channel2" else channel)
        uc.reg_write(UC_ARM_REG_R1,song)
        uc.emu_start(0x08000000+ROUTINES[routine][0]+1,0x03007F00,count=10000)
        if uc.reg_read(UC_ARM_REG_PC)!=0x03007F00: raise RuntimeError("Native sound helper instruction budget exceeded")
        if len(calls)>1: raise RuntimeError("Unexpected native sound effect count")
        case={"routine":routine,"channel":channel,"song":song,"current":current,"status":status}
        if routine=="status": case["result"]=uc.reg_read(UC_ARM_REG_R0)
        else:
            case["after"]=list(struct.unpack("<4H",uc.mem_read(0x030007C8,8)))
            case["call"]=calls[0] if calls else None
            if routine=="force-channel2": case["returnValue"]=uc.reg_read(UC_ARM_REG_R0)
        cases.append(case)
    return {"schemaVersion":1,"romSha256":ROM_SHA256,"boundary":"MPlayStart/MPlayStop intercepted; playback not executed","routines":[{"name":name,"offset":off,"length":length,"sha256":hashlib.sha256(rom[off:off+length]).hexdigest()} for name,(off,length) in ROUTINES.items()],"players":players,"headers":headers,"cases":cases}

if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--rom",required=True);p.add_argument("--generate",action="store_true");a=p.parse_args()
    actual=generate(a.rom);fixture=spp_vectors.path("sound-vectors.json")
    if a.generate: fixture.write_text(json.dumps(actual,separators=(",",":"))+"\n",encoding="utf8")
    elif json.loads(fixture.read_text(encoding="utf8"))!=actual: raise SystemExit("Native sound routing vectors differ")
    print(json.dumps({"cases":len(actual["cases"]),"sourceSha256":ROM_SHA256,"boundary":actual["boundary"]}))
