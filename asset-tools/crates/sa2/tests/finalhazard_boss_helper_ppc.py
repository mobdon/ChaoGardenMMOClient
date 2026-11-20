"""Compare the Rust Finalhazard boss helpers with installed PPC instructions.

Usage: python finalhazard_boss_helper_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs each helper in Boss_Last2D.rel section 1 + 0x9d4..0xe2c to its return,
with the helpers it calls running natively too. Every store and the
returned r3 are compared.
"""
import itertools
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: (word(word(16) + 8 * n) & ~1, word(word(16) + 8 * n + 4))
code = section(1)[0]
START, END = 0x9D4, 0xE2C
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, data=0x81000, state=0x82000, entity=0x83000, other=0x84000,
               task_entity=0x85000)

relocs = {}
imp_off, imp_size = struct.unpack_from(">II", data, 0x28)
for i in range(imp_off, imp_off + imp_size, 8):
    mod, off = struct.unpack_from(">II", data, i)
    pos, sec = 0, None
    while True:
        o, t, s, add = struct.unpack_from(">HBBI", data, off)
        off += 8
        if t == 203:
            break
        pos += o
        if t == 202:
            sec, pos = s, 0
        elif t != 201 and sec == 1 and START <= pos < END:
            relocs[pos] = (t, mod, s, add)

cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(CODE + START, data[code + START:code + END])
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if t == 10:
        imports[at] = add
        continue
    if mod == 88 and s == 1:
        dest = CODE + add
    else:
        key = (mod, s, add)
        if key not in windows:
            windows[key] = 0x40000 + 0x1000 * len(windows)
            if mod == 88 and section(s)[0]:
                cpu.mem_write(windows[key], data[section(s)[0] + add:][:0x1000])
        dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))
BOSS = windows[(88, 6, 0)]


def code_hook(u, pc, size, _):
    if pc == RETURN:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 == 18 and ins & 1 and pc - CODE in imports:
        raise AssertionError(f"engine call at {pc - CODE:#x}")


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x200 <= addr < STACK + 0x100:
        return
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=name,
                               offset=addr - base, size=size, value=value & ((1 << 8 * size) - 1)))
            return
    raise AssertionError(hex(addr))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    product = itertools.product(
        (0x9D4, 0x9F4, 0xA40, 0xA88, 0xC60, 0xC98, 0xD04, 0xD40, 0xDC4),
        (0, REGIONS["task"]), (0, REGIONS["data"]), (0, REGIONS["state"]),
        (0, 13, 3, -1), (0, 4, 0x7FFB, -1), (0, 1, 0x1FF, 0xFFFFFFFE))
    n = 0
    for routine, boss, dat, state, action, flags, s0c in product:
        if routine == 0xD40 and (boss == 0 or dat == 0 or state == 0):
            continue
        for s04, d28, arg in ((n % 7, 1.0 + (n % 4) + 0.75 * (n % 2), (3, 13, 5, -2)[n % 4]),):
            n += 1
            events = []
            cpu.mem_write(0x80000, bytes(0x6000))
            cpu.mem_write(BOSS, struct.pack(">I", boss))
            cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
            cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", dat))
            cpu.mem_write(REGIONS["data"] + 0x10, struct.pack(">I", state))
            cpu.mem_write(REGIONS["data"] + 0x28, struct.pack(">f", d28))
            cpu.mem_write(REGIONS["state"] + 4, struct.pack(">B", s04 if s04 < 6 else 0xFF))
            cpu.mem_write(REGIONS["state"] + 0xC, struct.pack(">I", s0c))
            cpu.mem_write(REGIONS["state"] + 0x10, struct.pack(">I", 0x7700 + n))
            cpu.mem_write(REGIONS["entity"], struct.pack(">b", action))
            cpu.mem_write(REGIONS["entity"] + 4, struct.pack(">h", flags))
            cpu.mem_write(REGIONS["other"] + 0x34, struct.pack(">I", REGIONS["task_entity"]))
            cpu.reg_write(R.UC_PPC_REG_1, STACK)
            cpu.reg_write(R.UC_PPC_REG_3, REGIONS["other"] if routine == 0xDC4 else REGIONS["task"])
            cpu.reg_write(R.UC_PPC_REG_4, arg & 0xFFFFFFFF)
            cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
            cpu.emu_start(CODE + routine, RETURN + 4, count=10000)
            returned = cpu.reg_read(R.UC_PPC_REG_3)
            inp = dict(path="boss_helper", routine=routine, boss=boss, data=dat, state=state,
                       child=0x7700 + n if dat and state else 0, action=action, flags=flags, state_0c=s0c,
                       state_04=s04 if s04 < 6 else 0xFF, data_28=d28, argument=arg)
            q, out = p / "request.json", p / f"{n}.json"
            q.write_text(json.dumps(inp))
            subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                           stdout=subprocess.DEVNULL)
            report = json.loads(out.read_text())
            got = report["events"]
            assert got == events, (inp, got, events)
            if routine != 0xDC4:
                assert report["returned"] == returned, (inp, report["returned"], returned)
            cases += 1
print(f"{cases} installed PPC boss helper calls matched")
