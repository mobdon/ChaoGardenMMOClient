"""Run CartD's billboard picture choice (+0x71b8..+0x72a4) and compare it with Rust.
Usage: python kart_billboards_ppc.py CLI EXE DATA_DLL CartD.rel PRIVATE_DIR
Engine rand() (both call sites) returns a fixed LCG sequence; the same returns
are then given to `kart-course-billboards`, which must consume all of them.
"""
import json
from gsne8p import steam_rel
import struct
import subprocess
import sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

cli, exe, dll, rel, work = map(Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=True)
data = rel.read_bytes()
word = lambda at: struct.unpack_from(">I", data, at)[0]
base = {1: 0x81000000, 4: 0x83000000, 5: 0x82000000}
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
for section, addr in base.items():
    row = word(16) + 8 * section
    off, size = word(row) & ~1, word(row + 4)
    u.mem_map(addr, (size + 4095) & ~4095)
    u.mem_write(addr, data[off:off + size])
for mod, at in struct.iter_unpack(">II", data[word(40):word(40) + word(44)]):
    while True:
        delta, kind, sec, add = steam_rel(struct.unpack_from(">HBBI", data, at), mod)
        at += 8
        if kind == 203:
            break
        if kind == 202:
            target, loc = sec, 0
            continue
        loc += delta
        if mod != 80 or sec not in base:
            continue
        value = base[sec] + add
        if target == 1 and kind in (4, 6):
            half = (value + 0x8000) >> 16 if kind == 6 else value & 0xFFFF
            u.mem_write(base[1] + loc, struct.pack(">H", half))
reg = lambda n: getattr(R, "UC_PPC_REG_" + str(n))
ram, stack = 0x85000000, 0x86000000
u.mem_map(ram, 0x10000)
u.mem_map(stack, 0x10000)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
state = {"seed": 0x2468ACE, "returns": []}
def rand(machine, addr, size, user):
    state["seed"] = (state["seed"] * 1103515245 + 12345) & 0xFFFFFFFF
    value = (state["seed"] >> 16) & 0x7FFF
    state["returns"].append(value)
    machine.reg_write(reg(3), value)
    machine.reg_write(R.UC_PPC_REG_PC, addr + 4)
for site in (0x71F8, 0x7230):
    u.hook_add(UC_HOOK_CODE, rand, begin=base[1] + site, end=base[1] + site)

courses = work / "courses.json"
for old in work.glob("*.json"):
    old.unlink()  # the CLI never overwrites an existing report
subprocess.run([str(cli), "kart-courses", str(exe), str(courses)], check=True)
s4 = lambda at: base[4] + at
for course in json.loads(courses.read_text()):
    if course["name"] not in ("101", "280"):
        continue
    ids = course["piece_ids"]
    state["returns"] = []
    native = []
    u.mem_write(ram, bytes(ids))
    for i in range(len(ids)):
        u.mem_write(ram + 0x1000, b"\xAA" * 0x30)
        u.mem_write(stack + 0x8000 + 0x30, struct.pack(">i", len(ids) - 0x14))
        for n, v in {1: stack + 0x8000, 14: ram, 15: s4(0x2E4), 18: s4(0x2DC), 19: s4(0x2E0),
                     20: s4(0x2D8), 22: base[5] + 0xF9EDC, 24: s4(0x2D4), 25: 0x43300000,
                     26: i, 28: ram + 0x1000}.items():
            u.reg_write(reg(n), v)
        u.emu_start(base[1] + 0x71B8, base[1] + 0x72A4, count=4096)
        assert u.reg_read(R.UC_PPC_REG_PC) == base[1] + 0x72A4
        words = struct.unpack(">4i", u.mem_read(ram + 0x1020, 16))
        native.append(None if i + 20 >= len(ids) else list(words))
        if i + 20 >= len(ids):
            assert u.mem_read(ram + 0x1020, 16) == b"\xAA" * 16
    rands = work / f"rand-{course['name']}.json"
    rands.write_text(json.dumps(state["returns"]))
    out = work / f"billboards-{course['name']}.json"
    subprocess.run([str(cli), "kart-course-billboards", str(exe), str(out), str(dll), str(rel),
                    course["name"], str(rands)], check=True)
    report = json.loads(out.read_text())
    assert report["randCalls"] == len(state["returns"])
    ours = [o["pictures"] for o in report["occurrences"]]
    assert ours == native, course["name"]
    shown = sum(1 for p in native if p for v in p if v)
    print(course["name"], len(ids), "rand", len(state["returns"]), "shown", shown)
