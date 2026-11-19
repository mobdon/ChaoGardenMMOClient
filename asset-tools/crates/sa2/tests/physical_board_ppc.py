"""Optional instruction oracle for the original PPC ObjectBoardExec in PC stg13D.rel.

Set SA2_SET_DIR to the installed gd_PC and install unicorn==2.1.4 for Python.
cargo test --test physical_board_trace compares_installed_ppc -- --ignored
No executable bytes are stored, launched as host code or written back.
"""
import hashlib
import os
from pathlib import Path
import struct
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn.ppc_const import UC_PPC_REG_1, UC_PPC_REG_3, UC_PPC_REG_LR, UC_PPC_REG_MSR, UC_PPC_REG_PC

TEXT = 0x81000000
START, END = 0x3B4, 0x4F0
RETURN = TEXT + 0x800
TASK, WORK, MOTION = 0x100000, 0x100100, 0x100200
PLAYER_TABLES = (0x801E7368, 0x801E7348, 0x801E7328)
DETACH, CHECK_RANGE = 0x800DE11C, 0x8006538C
SHA = "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b"


def code_from_rel(rel):
    assert hashlib.sha256(rel).hexdigest() == SHA
    word = lambda off: struct.unpack_from(">I",rel,off)[0]
    text_off = word(word(0x10) + 8) & ~1
    assert text_off == 0xe8
    code = bytearray(rel[text_off + START:text_off + END])
    targets = []
    imports = word(0x28)
    for module, at in struct.iter_unpack(">II",rel[imports:imports+word(0x2c)]):
        if module != 0:
            continue
        section, offset = None, 0
        while True:
            delta, kind, sec, value = struct.unpack_from(">HBBI",rel,at)
            at += 8
            if kind == 203:
                break
            if kind == 202:
                section, offset = sec, 0
                continue
            offset += delta
            if section != 1 or not START <= offset < END:
                continue
            loc=offset-START
            if kind in (6,4):   # R_PPC_ADDR16_HA, R_PPC_ADDR16_LO
                half = (value+0x8000)>>16 if kind==6 else value
                struct.pack_into(">H",code,loc,half & 0xffff)
            elif kind == 10:    # R_PPC_REL24 preserves the original branch/link bits
                insn = struct.unpack_from(">I",code,loc)[0]
                distance = value-(TEXT+offset)
                assert distance % 4 == 0 and -(1<<25)<=distance<(1<<25)
                struct.pack_into(">I",code,loc,(insn & 0xfc000003)|(distance & 0x03fffffc))
            else:
                raise ValueError(f"unexpected ObjectBoardExec relocation {kind}")
            targets.append(value)
    assert targets == [p for p in PLAYER_TABLES for _ in range(2)]+[DETACH,CHECK_RANGE]
    return bytes(code)


def machine(code):
    u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
    for address,size in [(TEXT,0x1000),(TASK,0x8000),(0x801e7000,0x1000),
                         (DETACH & ~0xfff,0x1000),(CHECK_RANGE & ~0xfff,0x1000)]:
        u.mem_map(address,size)
    u.mem_write(TEXT+START,code)
    for callee in [DETACH,CHECK_RANGE]:
        u.mem_write(callee,struct.pack(">I",0x4e800020))  # return from external stub
    u.reg_write(UC_PPC_REG_MSR,u.reg_read(UC_PPC_REG_MSR)|0x2000)  # enable floating point
    return u


def run(code):
    actions = [0,0x78,0x79,0x90,0x91,-1,0x79,0x80,0x90,0,0x80]
    modes = [0,0,3,2,3,5,3,-1,4,0,2]
    for ang_x in [0,1,-2,-1]:
        u=machine(code); slot=ang_x & 1; calls=[]
        def put(addr,fmt,*values): u.mem_write(addr,struct.pack(">"+fmt,*values))
        def get(addr,fmt): return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
        def observe(uc,pc,size,_):
            if pc in (DETACH,CHECK_RANGE):
                assert uc.reg_read(UC_PPC_REG_3)==TASK and get(WORK,"B")== (2,)
                calls.append(pc)
        for callee in (DETACH,CHECK_RANGE):
            u.hook_add(UC_HOOK_CODE,observe,begin=callee,end=callee)
        put(TASK+0x34,"II",WORK,MOTION)
        put(WORK,"BB",0,slot); put(WORK+8,"iii",ang_x,-32768,-1)
        put(WORK+0x14,"fff",-1.0,2.5,-3.0)
        for frame,(action,motion_mode) in enumerate(zip(actions,modes)):
            for s in (0,1):
                i=frame+s*20
                p=TASK+0x1000+s*0x1000; m=p+0x200; pw=p+0x400
                put(PLAYER_TABLES[0]+4*s,"I",0 if frame in (0,5) else p)
                put(PLAYER_TABLES[1]+4*s,"I",m);put(PLAYER_TABLES[2]+4*s,"I",pw)
                put(p+8,"iii",-i,40000-i,i*1234);put(p+0x14,"fff",i+0.5,-i,5)
                put(m,"fff",i*0.25,-2,3);put(pw+0x174,"h",motion_mode)
                put(pw+0x178,"h",action);put(pw+0x184,"f",i+0.75)
            calls.clear()
            u.reg_write(UC_PPC_REG_1,TASK+0x7800)
            u.reg_write(UC_PPC_REG_3,TASK);u.reg_write(UC_PPC_REG_LR,RETURN)
            u.emu_start(TEXT+START,RETURN,count=200)
            assert u.reg_read(UC_PPC_REG_PC)==RETURN and calls in ([],[DETACH,CHECK_RANGE])
            mode,slot_read=get(WORK,"BB")
            print(slot_read,mode,int(bool(calls)),*get(WORK+0x14,"III"),*get(WORK+8,"iii"),
                  *get(MOTION,"III"),*get(MOTION+0x18,"iI"))


if __name__ == "__main__":
    run(code_from_rel((Path(os.environ["SA2_SET_DIR"])/"stg13D.rel").read_bytes()))
