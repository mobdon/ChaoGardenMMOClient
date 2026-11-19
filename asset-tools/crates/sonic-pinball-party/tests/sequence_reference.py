"""Original MPlay command leaves on synthetic state; no BIOS or helper hooks.

Pattern depth overflow calls a separate unfinished track-finish routine and is
tested as an explicit Rust error. These vectors do not establish playback.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES = [("goto",0x5E1D8,0x20),("pattern",0x5E1F8,0x1C),("return",0x5E214,0x14),
            ("repeat",0x5E228,0x30),("field1d",0x5E258,0xC),("tempo",0x5E264,0x14),
            ("field0a",0x5E278,0x14),("voice",0x5E28C,0x30),("field12",0x5E2BC,0x14),
            ("centered14",0x5E2D0,0x14),("centered0e",0x5E2E4,0x14),
            ("field0f",0x5E2F8,0x14),("field1b",0x5E30C,0xC)]


def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
    with open(path,"rb") as f:
        rom=f.read(0x800001)
    if len(rom)!=0x800000 or hashlib.sha256(rom).hexdigest()!=ROM_SHA256:
        raise ValueError("Exact local A3VE rev0 required")
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for address,length in [(0,0x10000),(0x08000000,0x800000),(0x02000000,0x40000),(0x03000000,0x8000)]:
        u.mem_map(address,length)
    u.mem_write(0x08000000,rom)
    rng=random.Random(0xA3E5E1D8)
    cases=[]
    for name,entry,length in ROUTINES:
        for i in range(40):
            player=bytearray(rng.randbytes(64))
            track=bytearray(rng.randbytes(80))
            cursor=0x1000 if i%7==0 else 0x02008000
            command=bytearray(rng.randbytes(8))
            command[0]=[0,1,127,128,255][i%5]
            track[2]=i%3 if name=="pattern" else i%4
            track[3]=[0,1,254,255][i%4]
            struct.pack_into("<I",track,0x40,cursor)
            voice_address=0x2000 if i%9==0 else 0x0200A000
            struct.pack_into("<I",player,0x30,voice_address)
            struct.pack_into("<H",player,0x1E,[0,1,255,256,65535][i%5])
            voice_seed=rng.randrange(256)
            voice=bytes((voice_seed+j*37)&255 for j in range(3072))
            u.mem_write(0x02001000,bytes(player))
            u.mem_write(0x02002000,bytes(track))
            u.mem_write(cursor,bytes(command))
            u.mem_write(voice_address,voice)
            u.reg_write(UC_ARM_REG_SP,0x03007D00)
            u.reg_write(UC_ARM_REG_LR,0x03007F01)
            u.reg_write(UC_ARM_REG_R0,0x02001000)
            u.reg_write(UC_ARM_REG_R1,0x02002000)
            u.emu_start(0x08000000+entry+1,0x03007F00,count=10000)
            if u.reg_read(UC_ARM_REG_PC)!=0x03007F00:
                raise RuntimeError("Native command instruction budget exceeded")
            cases.append(dict(entry=entry,cursor=cursor,command=bytes(command).hex(),voiceSeed=voice_seed,voiceAddress=voice_address,
                              player=bytes(player).hex(),track=bytes(track).hex(),
                              afterPlayer=bytes(u.mem_read(0x02001000,64)).hex(),afterTrack=bytes(u.mem_read(0x02002000,80)).hex()))
    spans=ROUTINES+[("protectedRead",0x5E1B0,0x1C),("nextByte",0x5E1CC,0xC)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,
                scope="Original native command leaves, no hooks; pattern overflow and whole sequencer/playback remain explicit separate work",
                routines=[dict(name=name,offset=entry,length=length,sha256=hashlib.sha256(rom[entry:entry+length]).hexdigest()) for name,entry,length in spans],cases=cases)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom",required=True)
    p.add_argument("--generate",action="store_true")
    a=p.parse_args()
    actual=generate(a.rom)
    target=Path(__file__).with_name("sequence-vectors.json")
    if a.generate:
        target.write_text(json.dumps(actual,separators=(",",":"))+"\n",encoding="utf8")
    elif json.loads(target.read_text(encoding="utf8"))!=actual:
        raise SystemExit("Native MPlay command vectors differ")
    print(json.dumps(dict(nativeCommands=len(actual["cases"]),wholeGamePorted=False)))
