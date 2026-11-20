"""Compare the Rust super-form collision placement with installed PPC instructions.

Usage: python finalhazard_super_collision_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xc598 to its return. The engine call
0x8003e778 rewrites its vector argument with the request's `rotated` value.
Every call, pointed-to vector and non-stack store is compared in order.
"""
import json
from pathlib import Path
import random
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
        elif t != 201 and sec == 1:
            relocs[pos] = (t, mod, s, add)

CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
START, END = 0xC598, 0xC67C
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(CODE + START, data[code + START:code + END])
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not START <= at < END:
        continue
    if t == 10:
        imports[at] = add
        continue
    key = (mod, s, add)
    if key not in windows:
        windows[key] = 0x40000 + 0x1000 * len(windows)
        start, size = section(s) if mod == 88 else (0, 0)
        if start:
            cpu.mem_write(windows[key], data[start + add:start + min(size, add + 0x1000)])
    dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))

ENT, D2, PL, COL, INNER = 0x80000, 0x81000, 0x82000, 0x83000, 0x84000
REGIONS = [("entity", ENT), ("data2", D2), ("player", PL), ("inner", INNER)]
SAVE, RESTORE = {0x8010AC84}, {0x8010ACD0}
saved = []
f2d = lambda f: struct.unpack(">Q", struct.pack(">d", f))[0]
d2f = lambda q: struct.unpack(">d", struct.pack(">Q", q))[0]
f32 = lambda v: struct.unpack(">f", struct.pack(">f", v))[0]


def floats(u, p, n):
    try:
        return list(struct.unpack(f">{n}f", u.mem_read(p, 4 * n)))
    except Exception:
        return None


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    if off not in imports:
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    assert ins >> 26 == 18 and ins & 1, hex(off)
    target = imports[off]
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    if target in SAVE:
        saved.append([u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(14, 32)])
    elif target in RESTORE:
        for n, v in zip(range(14, 32), saved.pop()):
            u.reg_write(getattr(R, f"UC_PPC_REG_{n}"), v)
    else:
        events.append(dict(site=off, call=target, args=regs,
                           vectors={str(i + 3): floats(u, regs[i], 3) for i in range(3)},
                           blocks={str(i + 3): floats(u, regs[i], 6) for i in range(2)}))
        if target == 0x8003E778:  # engine rotates the offset in place
            u.mem_write(regs[1], struct.pack(">3f", *inp["rotated"]))
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x1000 <= addr < STACK + 0x100:
        return
    for name, base in REGIONS:
        if base <= addr < base + 0x1000:
            events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=name,
                               offset=addr - base, size=size,
                               value=value & ((1 << (8 * size)) - 1)))
            return
    raise AssertionError(f"unexpected store at {addr:#x}")


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)


def run():
    global events
    events = []
    for base in (ENT, D2, PL, COL, INNER):
        cpu.mem_write(base, bytes(0x1000))
    cpu.mem_write(ENT, struct.pack(">b", inp["action"]))
    cpu.mem_write(ENT + 8, struct.pack(">3i", 11, 22, 33))
    cpu.mem_write(ENT + 0x14, struct.pack(">3f", *inp["position"]))
    cpu.mem_write(ENT + 0x2C, struct.pack(">I", COL))
    cpu.mem_write(COL + 0xC, struct.pack(">I", INNER))
    cpu.mem_write(D2 + 0x18, struct.pack(">3I", 7, 8, 9))
    cpu.mem_write(PL + 0x140, struct.pack(">f", inp["height"]))
    cpu.reg_write(R.UC_PPC_REG_3, ENT)
    cpu.reg_write(R.UC_PPC_REG_4, D2)
    cpu.reg_write(R.UC_PPC_REG_5, PL)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=10000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases = 0
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(500):
        inp = dict(path="super_collision", entity=ENT, data2=D2, player=PL,
                   action=rng.choice((67, 59, 0, 57, -1)),
                   position=[f32(rng.uniform(-1500, 1500)) for _ in range(3)],
                   height=f32(rng.uniform(-10, 20)),
                   rotated=[f32(rng.uniform(-20, 20)) for _ in range(3)])
        native = run()
        q = p / "request.json"
        out = p / f"{n}.json"
        q.write_text(json.dumps(inp))
        res = subprocess.run([exe, "finalhazard-placement-clock", module, out, q],
                             capture_output=True, text=True)
        assert res.returncode == 0, (n, res.stderr[-500:])
        got = json.loads(out.read_text())["events"]
        assert [e["site"] for e in got] == [e["site"] for e in native], (n, got, native)
        for r, e in zip(got, native):
            if "store" in r:
                assert r == e, (n, r, e)
                continue
            assert r["call"] == e["call"]
            assert r["args"] == [v & 0xFFFFFFFF for v in e["args"][:len(r["args"])]], (n, r, e)
            for reg, value in r.get("vectors", {}).items():
                assert value == e["vectors"][reg], (n, reg, r, e)
        cases += 1
print(f"{cases} installed PPC super-collision placements matched")