"""Compare the Rust super-form effects draw with installed PPC instructions.

Usage: python finalhazard_super_effects_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xd60c to its return with the module data
windows holding the file's bytes (including the fade colour source table
and the relocated trail pointers). Every call, argument (module addresses
as "88:SECTION+OFFSET"), float argument and store is compared in order.
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
START, END = 0xD60C, 0xDC90
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
REGIONS = []
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
                           floats=[struct.unpack(">d", struct.pack(">Q", u.reg_read(getattr(R, f"UC_PPC_REG_FPR{k}"))))[0] for k in (1, 2, 3)],
                           vectors={str(i + 3): floats(u, regs[i], 3) for i in range(3)},
                           blocks={str(i + 3): floats(u, regs[i], 6) for i in range(2)}))
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x1000 <= addr < STACK + 0x100:
        return
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, address=addr, size=size,
                       value=value & ((1 << (8 * size)) - 1)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)


GLOBAL = windows[(0, 6, 0x801CBD88)]
MOTIONS = windows[(0, 7, 0x801E6288)]
WORKPTR = windows[(88, 6, 0x4A8)]
# Relocated pointers the trail reads from module data (section 5).
for holder, target in ((0x3E07C, 0x3E064), (0x3E928, 0x3E910)):
    windows[(88, 5, target)] = 0x30000 + 0x1000 * len([k for k in windows if k[0] == 88 and k[2] in (0x3E064, 0x3E910)])
    cpu.mem_write(windows[(88, 5, holder)] + 4, struct.pack(">I", windows[(88, 5, target)]))
AURA = windows[(88, 5, 0x3C9A8)] + 0xE


def symbol(v):
    for (mod, s, add), w in windows.items():
        if mod == 88 and w <= v < w + 0x1000:
            return f"88:{s}+{add + v - w:#x}"
    return v


TASK, WORK = 0x86000, 0x86400
SPIN = (0xBF4, 0xC14, 0xC04, 0xC24, 0xBFC, 0xC1C, 0xC0C, 0xC2C)


def run():
    global events
    events = []
    for base in (ENT, PL, TASK, WORK):
        cpu.mem_write(base, bytes(0x400))
    for i, off in enumerate(SPIN):
        cpu.mem_write(MOTIONS + off, struct.pack(">I", 0xC0000000 + i))
    cpu.mem_write(TASK + 0x34, struct.pack(">I", ENT))
    cpu.mem_write(TASK + 0x40, struct.pack(">I", PL))
    cpu.mem_write(WORKPTR, struct.pack(">I", WORK))
    cpu.mem_write(WORK, struct.pack(">b", inp["active_player"]))
    cpu.mem_write(GLOBAL + 0x42, struct.pack(">bb", inp["global_42"], inp["global_43"]))
    cpu.mem_write(GLOBAL + 0x7C, struct.pack(">I", inp["frame"]))
    cpu.mem_write(ENT + 4, struct.pack(">h", inp["entity_flags"]))
    cpu.mem_write(ENT + 8, struct.pack(">3i", *inp["rotation"]))
    cpu.mem_write(PL, struct.pack(">bb", inp["slot"], inp["character"]))
    cpu.mem_write(PL + 0x178, struct.pack(">h", inp["pose"]))
    cpu.mem_write(PL + 0x35C, struct.pack(">b", inp["counter_35c"]))
    cpu.mem_write(PL + 0x370, struct.pack(">I", inp["word_370"]))
    cpu.mem_write(AURA, struct.pack(">H", inp["aura_halfword"]))
    cpu.reg_write(R.UC_PPC_REG_3, TASK)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=20000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases, seen = 0, set()
fade_reloads = {}
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(1500):
        slot = rng.choice((0, 1))
        frame = rng.choice((0, 14400, 14401, 14470, 14480, 20000, 32400, 32401, 50000,
                            rng.randrange(1 << 32)))
        inp = dict(path="super_effects", entity=ENT, player=PL,
                   active_player=rng.choice((slot, slot, 1 - slot)),
                   global_42=rng.choice((0, 0, 1)), global_43=rng.choice((0, 0, 1)),
                   frame=frame, slot=slot, character=rng.choice((9, 10, 10)),
                   pose=rng.choice((8, 9, 10, 11, 100, 89, 90, 92, 94, 96, 98, 0)),
                   counter_35c=rng.choice((0, 3, 5, -1)), word_370=rng.randrange(1 << 32),
                   entity_flags=rng.choice((0, 0x400, -1, 0x3FF)),
                   rotation=[rng.randrange(-0x10000, 0x10000) for _ in range(3)],
                   aura_halfword=rng.randrange(1 << 16),
                   spin_models=[0xC0000000 + i + (0 if frame & 1 else 4) for i in range(4)])
        # The fade loop rewrites module colours; restore the file bytes each case.
        for (mod, s, add), w in list(windows.items()):
            if mod == 88 and add in (0x3D450,):
                start, size = section(s)
                cpu.mem_write(w, data[start + add:start + min(size, add + 0x1000)])
        native = run()
        q = p / "request.json"
        out = p / f"{n}.json"
        q.write_text(json.dumps(inp))
        res = subprocess.run([exe, "finalhazard-placement-clock", module, out, q],
                             capture_output=True, text=True)
        assert res.returncode == 0, (n, res.stderr[-500:])
        got = json.loads(out.read_text())["events"]
        assert [e["site"] for e in got] == [e["site"] for e in native], (n, [hex(e["site"]) for e in got], [hex(e["site"]) for e in native])
        for r, e in zip(got, native):
            if "store" in r:
                assert (symbol(e["address"]), e["size"], e["value"]) == (r["offset"], r["size"], r["value"]), (n, r, e)
                continue
            assert r["call"] == e["call"], (n, r, e)
            args = [symbol(v) if isinstance(symbol(v), str) else v & 0xFFFFFFFF
                    for v in e["args"][:len(r["args"])]]
            assert r["args"] == args, (n, r, args)
            if "floats" in r:
                assert r["floats"] == e["floats"], (n, r, e)
        seen.update(e["site"] for e in native)
        cases += 1
required = {0xD700, 0xD7D0, 0xD870, 0xD898, 0xD988, 0xD9A8, 0xDA5C, 0xDA90, 0xDB20, 0xDBBC,
            0xDBF0, 0xDC34, 0xDC44, 0xDC58, 0xDC68, 0xDC70}
assert required <= seen, sorted(hex(s) for s in required - seen)
print(f"{cases} installed PPC super-form effect draws matched")