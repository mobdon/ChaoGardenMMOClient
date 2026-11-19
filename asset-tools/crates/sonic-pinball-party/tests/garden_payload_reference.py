"""Offline native A3VE Tiny Garden board oracle. The only player inputs are synthetic.

Executes D504, its D7CC successor, the real 1534 PRNG, E974
and EA5C. BIOS Div/CpuSet/CpuFastSet and the explicitly listed dependencies are bounded
externally. Per-case RAM hashes cover adjacent untouched bytes as well.
"""
import argparse
import json
import os
import struct
from pathlib import Path

import reference_parity as parity

FIXTURE = Path(__file__).with_name("garden-payload-vectors.json")
EWRAM = 0x02000000
IWRAM = 0x03000000
BOARD = IWRAM + 0x1ae8
SNAPSHOT = IWRAM + 0x1300
SIZE = 0x1734
RETURN = IWRAM + 0x7000
PAYLOAD_SHA = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
SOURCES = {
    "resetEntryAndIrq": (0, 0xc4),
    "thumbMain": (0x168, 0x19c),
    "boardSetupPrefix": (0xd504, 0xd6c4),
    "setupTailAndLiterals": (0xd6c4, 0xd7cc),
    "frameCallback": (0xd7cc, 0xd8a4),
    "initializeObjects": (0xc90,0xd04),
    "allocateObject": (0xd04,0xdb0),
    "initializeOam": (0x1290,0x12e0),
    "uploadOam": (0x12e0,0x12f8),
    "displayWriteQueue": (0x167c,0x1718),
    "biosCopy": (0x11ae0, 0x11ae8),
    "rng": (0x1534, 0x1554),
    "biosDiv": (0x11ae8, 0x11aec),
    "roundCountsAndTags": (0x2e108, 0x2e117),
    "directions": (0x2e11b, 0x2e129),
    "flipRay": (0xe974, 0xea5c),
    "flipSevenDirections": (0xea5c, 0xeb08),
}

def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R3,
                                   UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP)
    rom = parity.bounded_read(Path(path), 0x800000)
    parity.validate_rom(rom)
    payload, _ = parity.decode(rom[0xd0970:], maximum=0x40000)
    assert len(payload) == 188716 and parity.sha256(payload) == PAYLOAD_SHA
    cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
    cpu.mem_map(EWRAM, 0x40000)
    cpu.mem_map(IWRAM, 0x8000)
    cpu.mem_write(EWRAM, bytes(payload))
    cpu.mem_map(0x05000000, 0x1000)
    cpu.mem_map(0x06000000, 0x18000)
    cpu.mem_map(0x04000000, 0x1000)
    cpu.mem_map(0x07000000, 0x1000)
    concrete = False
    calls = []
    allocated = []
    arity = {0xc90:0, 0xd04:3, 0x167c:2, 0x16d8:0, 0x12e0:0, 0x1420:0,
             0x1374:0, 0xeac:0, 0x117c:0, 0x1088:0, 0xdb28:0, 0x1698:2,
             0x10804:2, 0xe6d8:0}

    def presentation(machine, address, _size, _user):
        from unicorn.arm_const import UC_ARM_REG_R2
        if concrete and address == EWRAM+0x1420:
            machine.emu_stop()
            return
        if concrete and address-EWRAM in (0xc90,0xd04,0x167c,0x1698,0x16d8,0x12e0):
            return
        count = arity[address - EWRAM]
        args = [machine.reg_read(reg) for reg in (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2)[:count]]
        calls.append([address,args])
        if address == EWRAM + 0xd04:
            ptr = IWRAM + 0x5400 + len(allocated) * 0x24
            allocated.append(ptr)
            machine.reg_write(UC_ARM_REG_R0, ptr)
        machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    def bios(machine, interrupt, _user):
        from unicorn.arm_const import UC_ARM_REG_R2
        pc = machine.reg_read(UC_ARM_REG_PC) - EWRAM
        assert interrupt == 2
        a, b = (machine.reg_read(reg) for reg in (UC_ARM_REG_R0, UC_ARM_REG_R1))
        if pc in (0x11ae2, 0x11ae6):
            ctrl = machine.reg_read(UC_ARM_REG_R2)
            width = 4 if pc == 0x11ae2 else 2
            count = ctrl & 0x1fffff
            assert count <= 1024 and (width == 2 or count % 8 == 0)
            if ctrl & (1 << 24):
                data = bytes(machine.mem_read(a,width)) * count
            else:
                data = bytes(machine.mem_read(a,width*count))
            machine.mem_write(b,data)
            return
        assert pc == 0x11aea and 0 < b < 128 and 0 < a < 65536
        machine.reg_write(UC_ARM_REG_R0, a // b)
        machine.reg_write(UC_ARM_REG_R1, a % b)
        machine.reg_write(UC_ARM_REG_R3, a // b)

    cpu.hook_add(UC_HOOK_INTR, bios)
    for offset in arity:
        pc = EWRAM + offset
        cpu.hook_add(UC_HOOK_CODE, presentation, begin=pc, end=pc)
    def reset():
        calls.clear()
        allocated.clear()
        cpu.mem_write(IWRAM, bytes(0x8000))
        cpu.mem_write(SNAPSHOT, bytes([0xa5]) * SIZE)
        cpu.reg_write(UC_ARM_REG_SP, IWRAM + 0x7f00)
        cpu.reg_write(UC_ARM_REG_LR, RETURN | 1)

    def run(start, end):
        try:
            cpu.emu_start(EWRAM + start + 1, end, count=200000)
        except Exception as error:
            raise RuntimeError(f"native PC {cpu.reg_read(UC_ARM_REG_PC):08x}, calls {calls}") from error
        assert cpu.reg_read(UC_ARM_REG_PC) == end
        return parity.sha256(cpu.mem_read(SNAPSHOT, SIZE))

    setup = []
    for synthetic in (False, True):
        if synthetic:
            cpu.mem_write(EWRAM + 0x2e108, bytes([1, 2, 3] * 4 + [0xd3, 0xe6, 0xa8]))
        for level in range(4):
            for seed in ([0x1badcafe] if synthetic else [0, 1, 0xffffffff, 0x54696368]):
                reset()
                cpu.mem_write(IWRAM + 0x13a8, struct.pack("<I", seed))
                cpu.mem_write(IWRAM + 0x1ad1, bytes([level]))
                digest = run(0xd504, EWRAM + 0xd6c4)
                setup.append(dict(round=level, seed=seed, synthetic=synthetic, ramSha256=digest))
    cpu.mem_write(EWRAM + 0x2e108, bytes(payload[0x2e108:0x2e117]))

    rays = []
    directions = [(1,-1), (0,-1), (-1,-1), (-1,0), (-1,1), (0,1), (1,1)]
    def vector(name, origin, cells):
        reset()
        cpu.mem_write(BOARD, bytes(132))
        for row, col, value in cells:
            cpu.mem_write(BOARD + row * 12 + col, bytes([value]))
        cpu.reg_write(UC_ARM_REG_R0, origin[0])
        cpu.reg_write(UC_ARM_REG_R1, origin[1])
        digest = run(0xea5c, RETURN)
        rays.append(dict(name=name, origin=origin, cells=cells, ramSha256=digest,
                         flipped=cpu.reg_read(UC_ARM_REG_R0), calls=calls.copy()))

    for i, (dy, dx) in enumerate(directions + [(1,0)]):
        vector("direction-" + str(i), [5,5], [[5,5,2], [5+dy,5+dx,0x31],
            [5+2*dy,5+2*dx,3], [5+3*dy,5+3*dx,0xa2]])
    vector("adjacent", [5,5], [[5,5,1], [5,6,0xa1]])
    vector("open", [5,5], [[5,5,2], [5,6,1], [5,7,0xf0], [5,8,2]])
    vector("origin-high-nibble", [5,5], [[5,5,0xa2], [5,6,1], [5,7,2]])
    vector("long-edge", [1,1], [[1,1,1]] + [[1,x,2] for x in range(2,10)] + [[1,10,1]])
    vector("all-seven", [5,5], [[5,5,1]] + [[5+n*dy,5+n*dx,3-n]
            for dy,dx in directions for n in (1,2)])
    vector("empty-origin", [1,10], [])
    vector("several-unclosed", [3,3], [[3,3,1],[3,2,2],[3,1,2],[3,4,2],[2,3,3]])
    vector("mixed-specials", [2,9], [[2,9,2],[3,8,0xa3],[4,7,0x54],[5,6,0xf2]])
    vector("f0-wall", [2,4], [[2,4,2],[1,4,0xf0],[0,4,2]])
    vector("bottom-inward", [10,10], [[10,10,2],[9,9,0xf1],[8,8,2]])

    # The join below is the real callback handoff. External services are intercepted,
    # including the object allocator. The frame tests separately exercise dirty
    # buffer copies and both exit branches.
    def extra_memory():
        cpu.mem_write(0x05000000,bytes([0x77])*0x400)
        cpu.mem_write(0x0600c000,bytes([0x88])*0x1000)

    def full_digest():
        return parity.sha256(b"".join(bytes(cpu.mem_read(addr,n)) for addr,n in
            [(IWRAM,0x7000), (0x05000000,0x400), (0x0600c000,0x1000)]))

    frames = []
    for flags in (0,1,2,4,7):
        for exit_kind in (0,1,2):
            reset()
            extra_memory()
            cpu.mem_write(IWRAM+0x4250,struct.pack("<IIIHBB",EWRAM+0xd7cd, int(exit_kind==2),0,0x1234,0,int(exit_kind!=0)*2))
            for i,addr in enumerate((0x2a0c,0x29f4,0x29f5)):
                cpu.mem_write(IWRAM+addr,bytes([(flags>>i)&1]))
            for addr,size,mult in [(0x2a10,32,3),(0x1ff0,0x500,5),(0x24f0,0x500,7)]:
                cpu.mem_write(IWRAM+addr,bytes((n*mult+1)&255 for n in range(size)))
            run(0xd7cc, RETURN)
            frames.append(dict(flags=flags,exitKind=exit_kind,sha256=full_digest(),calls=calls.copy()))

    reset()
    extra_memory()
    cpu.mem_write(IWRAM+0x1ad1,bytes([1]))
    cpu.mem_write(IWRAM+0x13a8,struct.pack("<I",0x1badcafe))
    run(0xd504,RETURN)
    callback = struct.unpack("<I",cpu.mem_read(IWRAM+0x4250,4))[0]
    assert callback == EWRAM + 0xd7cd
    cpu.reg_write(UC_ARM_REG_LR, RETURN|1)
    run(callback-EWRAM-1,RETURN)
    composed = dict(round=1,seed=0x1badcafe,sha256=full_digest(),calls=calls.copy())
    # Same native branch with real object pool, register queue and OAM copy.
    # The ROM's object/frame services execute directly through the exact first
    # remaining Rust boundary. No inner function interception occurs here.
    concrete=True
    def hardware_reset():
        reset()
        extra_memory()
        cpu.mem_write(0x04000000, bytes([0x66])*0x400)
        cpu.mem_write(0x07000000, bytes([0x77])*0x400)
    def hardware_digest():
        return parity.sha256(b"".join(bytes(cpu.mem_read(addr,n)) for addr,n in
            [(IWRAM,0x7000), (0x04000000,0x400), (0x05000000,0x400),
            (0x0600c000,0x1000), (0x07000000,0x400)]))
    def native_call(offset,args):
        from unicorn.arm_const import UC_ARM_REG_R2
        for r,v in zip((UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2),args):
            cpu.reg_write(r,v)
        cpu.reg_write(UC_ARM_REG_LR,RETURN|1)
        run(offset,RETURN)
        return cpu.reg_read(UC_ARM_REG_R0)
    concrete_cases=[]
    for level in range(4):
        hardware_reset()
        seed=(0x54321*level + 19)
        cpu.mem_write(IWRAM+0x1ad1,bytes([level]))
        cpu.mem_write(IWRAM+0x13a8,struct.pack("<I",seed))
        run(0xd504,RETURN)
        callback=struct.unpack("<I",cpu.mem_read(IWRAM+0x4250,4))[0]
        assert callback==EWRAM+0xd7cd and not calls
        at_setup=hardware_digest()
        cpu.reg_write(UC_ARM_REG_LR,RETURN|1)
        run(callback-EWRAM-1,EWRAM+0x1420)
        assert not calls
        concrete_cases.append(dict(round=level,seed=seed,afterSetup=at_setup,
                                     firstFrameBoundary=EWRAM+0x1420,atBoundary=hardware_digest()))
    helper_cases=[]
    for before,used,head in ((0,0,0),(1,1,1),(257,2,69),(0,70,0)):
        hardware_reset()
        native_call(0xc90,[])
        cpu.mem_write(IWRAM+0x606,bytes([used,head]))
        result=native_call(0xd04,[0x0200abcd,IWRAM+0x5320,before])
        helper_cases.append(dict(before=before,used=used,head=head,result=result,sha256=hardware_digest()))
    hardware_reset()
    queue_calls=[[0x167c,0x12,0x12345]]+[[0x1698,((i%3)*2),0xbc00+i] for i in range(10)]
    queue_results=[native_call(off,[reg,value]) for off,reg,value in queue_calls]
    queue_results[0] = None  # 167C returns through R0; no native result.
    native_call(0x16d8,[])
    queue_case=dict(calls=queue_calls,results=queue_results,sha256=hardware_digest())
    return dict(romSha256=parity.sha256(rom), payloadSha256=PAYLOAD_SHA,
                sources={k: dict(space="garden", offset=a, length=b-a,
                sha256=parity.sha256(payload[a:b])) for k,(a,b) in SOURCES.items()},
                setupCases=setup, flipCases=rays, frameCases=frames, setupIntoFrame=composed,
                concreteSetup=concrete_cases, objectCases=helper_cases, queueCase=queue_case)

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    p.add_argument("--write", type=Path)
    args = p.parse_args()
    data = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(data, separators=(",",":")) + "\n", encoding="utf8")
    elif data != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise SystemExit("Native Garden board vectors differ")
    print(json.dumps({"setupCases": len(data["setupCases"]), "flipCases": len(data["flipCases"])}))
