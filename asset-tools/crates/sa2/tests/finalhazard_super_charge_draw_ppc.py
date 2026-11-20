"""Compare the Rust action-67 charge-effect draw with installed PPC instructions.

Usage: python finalhazard_super_charge_draw_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xc67c to its return with filled engine
motion/model tables and a player animation table. The request's motion and
model words are the entries the documented slots select. Every call and
its arguments are compared; engine table addresses are translated back.
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
START, END = 0xC67C, 0xC77C
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
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6, 7)]
    if target in SAVE:
        saved.append([u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(14, 32)])
    elif target in RESTORE:
        for n, v in zip(range(14, 32), saved.pop()):
            u.reg_write(getattr(R, f"UC_PPC_REG_{n}"), v)
    else:
        events.append(dict(site=off, call=target, args=regs,
                           floats=[struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]],
                           vectors={str(i + 3): floats(u, regs[i], 3) for i in range(3)},
                           blocks={str(i + 3): floats(u, regs[i], 6) for i in range(2)}))
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


GLOBAL = windows[(0, 6, 0x801CBD88)]
MOTIONS = windows[(0, 7, 0x801E6288)]
MODELS = windows[(0, 7, 0x801E5928)]
ENGINE = {w: add for (mod, s, add), w in windows.items() if mod == 0}
ATAB = 0x85000


def run():
    global events
    events = []
    for base in (ENT, PL, ATAB, MOTIONS, MODELS):
        cpu.mem_write(base, bytes(0x1000))
    for i in range(0x1000 // 8):
        cpu.mem_write(MOTIONS + 8 * i + 4, struct.pack(">I", 0xA0000000 + i))
        cpu.mem_write(MODELS + 8 * i + 4, struct.pack(">I", 0xB0000000 + i))
    cpu.mem_write(GLOBAL + 0x7C, struct.pack(">I", inp["frame"]))
    cpu.mem_write(ENT + 8, struct.pack(">3i", *inp["rotation"]))
    cpu.mem_write(PL, struct.pack(">b", inp["slot"]))
    cpu.mem_write(PL + 0x178, struct.pack(">h", anim))
    cpu.mem_write(PL + 0x184, struct.pack(">f", inp["f184"]))
    cpu.mem_write(PL + 0x198, struct.pack(">I", ATAB))
    cpu.mem_write(PL + 0x370, struct.pack(">I", inp["word_370"]))
    cpu.mem_write(ATAB + 16 * anim, struct.pack(">hh", *inp["pose"]))
    cpu.mem_write(ATAB + 16 * inp["pose"][0], struct.pack(">h", inp["base_pose"]))
    for r, v in zip((3, 4, 5), (ENT, PL, PL)):
        cpu.reg_write(getattr(R, f"UC_PPC_REG_{r}"), v)
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
        anim = rng.randrange(0, 100)
        pose0 = rng.choice([x for x in range(0, 100) if x != anim])
        pose1, base = rng.randrange(0, 400), rng.randrange(0, 400)
        inp = dict(path="super_charge_draw", entity=ENT, frame=rng.randrange(1 << 32),
                   rotation=[rng.randrange(-0x10000, 0x10000) for _ in range(3)],
                   slot=rng.choice((0, 1)), pose=[pose0, pose1], base_pose=base,
                   motion=0xA0000000 + pose1, model=0xB0000000 + base,
                   word_370=rng.randrange(1 << 32), f184=f32(rng.uniform(-5, 5)))
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
            assert r["call"] == e["call"]
            args = [ENGINE.get(v, v) & 0xFFFFFFFF for v in e["args"][:len(r["args"])]]
            assert r["args"] == args, (n, r, e)
            if "floats" in r:
                assert r["floats"] == e["floats"], (n, r, e)
        cases += 1
print(f"{cases} installed PPC charge-effect draws matched")