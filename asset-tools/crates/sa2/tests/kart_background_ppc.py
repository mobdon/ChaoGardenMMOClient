"""Check `kart-course-background` against CartD and main.dol instructions.
Usage: python kart_background_ppc.py CLI EXE DATA_DLL CartD.rel PRIVATE_DIR   (SA2_MAIN_DOL set)

* The sea vertex builder (CartD +0x9ac..+0xb8c) runs under Unicorn for a set
  of frame counters; its colour blend call runs main.dol's 0x800334B0 in the
  shared harness (tests/dol_ppc.py). Every vertex colour must equal Rust's.
* The course-minimum update (+0x736c..+0x7384) runs over the native layout of
  every course and must give Rust's minimum_y.
"""
import json
import os
import struct
import subprocess
import sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402

cli, exe, dll, rel, work = map(Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=True)
for old in work.glob("*.json"):
    old.unlink()  # the CLI never overwrites an existing report
data = rel.read_bytes()
word = lambda at: struct.unpack_from(">I", data, at)[0]
base = {1: 0x81000000, 4: 0x83000000, 5: 0x82000000, 6: 0x84000000}
ENGINE, SCRATCH = 0x801CB000, 0x80FFF000
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
for section, addr in base.items():
    row = word(16) + 8 * section
    off, size = word(row) & ~1, word(row + 4)
    u.mem_map(addr, (size + 4095) & ~4095)
    if off:
        u.mem_write(addr, data[off:off + size])
u.mem_map(ENGINE, 0x2000)
u.mem_map(SCRATCH, 0x1000)
for mod, at in struct.iter_unpack(">II", data[word(40):word(40) + word(44)]):
    while True:
        delta, kind, sec, add = struct.unpack_from(">HBBI", data, at)
        at += 8
        if kind == 203:
            break
        if kind == 202:
            target, loc = sec, 0
            continue
        loc += delta
        if mod == 80 and sec in base:
            value = base[sec] + add
        elif mod == 0 and ENGINE <= add < ENGINE + 0x2000:
            value = add
        else:
            continue
        if target == 1 and kind in (4, 6):
            half = (value + 0x8000) >> 16 if kind == 6 else value & 0xFFFF
            u.mem_write(base[1] + loc, struct.pack(">H", half))
        elif target == 5 and kind == 1:
            u.mem_write(base[5] + loc, struct.pack(">I", value))
reg = lambda n: getattr(R, "UC_PPC_REG_" + str(n))
stack = 0x86000000
u.mem_map(stack, 0x10000)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
dol = Dol(os.environ["SA2_MAIN_DOL"]).machine()

def skip(machine, addr, size, user):
    machine.reg_write(R.UC_PPC_REG_PC, addr + 4)
u.hook_add(UC_HOOK_CODE, skip, begin=base[1] + 0x9C0, end=base[1] + 0x9C0)

def blend(machine, addr, size, user):
    t = struct.unpack(">d", struct.pack(">Q", machine.reg_read(R.UC_PPC_REG_FPR1)))[0]
    dol.call(0x800334B0, r3=machine.reg_read(reg(3)), r4=machine.reg_read(reg(4)), f1=t)
    machine.reg_write(reg(3), dol.gpr(3))
    machine.reg_write(R.UC_PPC_REG_PC, addr + 4)
u.hook_add(UC_HOOK_CODE, blend, begin=base[1] + 0xB64, end=base[1] + 0xB64)

frames = [0, 1, 2, 24, 25, 49, 50, 51, 99, 100, 300, 549, 550, 551, 12345, 2**31 + 7, 2**32 - 1]
frames_path = work / "frames.json"
frames_path.write_text(json.dumps(frames))
checked = 0
for name in ("101", "280", "beginner", "standard", "expert"):
    out = work / f"background-{name}.json"
    subprocess.run([str(cli), "kart-course-background", str(exe), str(out), str(dll), str(rel),
                    name, str(frames_path)], check=True)
    report = json.loads(out.read_text())
    if name == "101":
        for entry in report["sea"]:
            u.mem_write(ENGINE + 0xD88 + 0x7C, struct.pack(">I", entry["frame"]))
            u.reg_write(reg(1), stack + 0x8000)
            u.emu_start(base[1] + 0x9AC, base[1] + 0xB90, count=200000)
            assert u.reg_read(R.UC_PPC_REG_PC) == base[1] + 0xB90
            n = len(entry["colors"])
            got = [struct.unpack(">I", u.mem_read(SCRATCH + 0xE60 + 8 + 16 * i + 12, 4))[0]
                   for i in range(n)]
            assert got == entry["colors"], entry["frame"]
            checked += 1
    lay = work / f"layout-{name}.json"
    subprocess.run([str(cli), "kart-native-layout", str(exe), str(lay), str(dll), name], check=True)
    minimum = stack + 0x100
    u.mem_write(minimum, struct.pack(">f", 100000.0))
    entry = stack + 0x200
    for p in json.loads(lay.read_text())[1:]:
        u.mem_write(entry + 4, struct.pack(">fff", *p["position"]))
        u.reg_write(reg(1), stack + 0x8000)
        u.mem_write(stack + 0x8000 + 0x28, struct.pack(">I", minimum))
        u.reg_write(reg(28), entry)
        u.emu_start(base[1] + 0x736C, base[1] + 0x7388, count=100)
    native_min = struct.unpack(">f", u.mem_read(minimum, 4))[0]
    ours = struct.unpack(">f", struct.pack(">f", report["background"]["minimumY"]))[0]
    assert native_min == ours, (name, native_min, ours)
    print(name, "minimumY", ours)
print("sea colours match for", checked, "frames")
