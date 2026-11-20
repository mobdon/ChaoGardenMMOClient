"""Compare the Rust hit ring drop with installed PPC instructions.

Usage: python finalhazard_ring_drop_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x19470 to its return. The engine ring
count, random number and task allocations return the request's values;
each allocated ring gets its own emulated entity. Every call (code and
module addresses as "88:SECTION+OFFSET") and every store is compared.
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
START, END = 0x19470, 0x196F0
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
SAVE, RESTORE = {0x8010AC84, 0x8010AC78}, {0x8010ACD0, 0x8010ACC4}
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
        if target == 0x8010D940:
            ret_value[0] = inp["random"]
        elif target == 0x80062A24:
            ret_value[0] = inp["rings"]
        elif target == 0x80071BA4:
            i = counters["tasks"]
            counters["tasks"] += 1
            ok = i < len(inp["allocated"]) and inp["allocated"][i]
            ret_value[0] = TASKS[i] if ok else 0
        else:
            ret_value[0] = 0
        events.append(dict(site=off, call=target, args=regs,
                           floats=[struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]],
                           vectors={str(i + 3): floats(u, regs[i], 3) for i in range(3)},
                           blocks={str(i + 3): floats(u, regs[i], 6) for i in range(2)}))
        u.reg_write(R.UC_PPC_REG_3, ret_value[0] & 0xFFFFFFFF)
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x1000 <= addr < STACK + 0x100:
        return
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, address=addr, size=size,
                       value=value & ((1 << (8 * size)) - 1)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)


P1T = windows[(0, 7, 0x801E7368)]
P2T = windows[(0, 7, 0x801E7328)]
TASKS = [0xA0000 + 0x400 * i for i in range(24)]
ENTS = [0xA8000 + 0x400 * i for i in range(24)]
ret_value = [0]
counters = {}


def symbol(v):
    if CODE <= v < CODE + 0x20000:
        return f"88:1+{v - CODE:#x}"
    for (mod, s, add), w in windows.items():
        if mod == 88 and w <= v < w + 0x1000:
            return f"88:{s}+{add + v - w:#x}"
    return v & 0xFFFFFFFF


def run():
    global events
    events = []
    counters["tasks"] = 0
    for base in (ENT, PL, *TASKS, *ENTS):
        cpu.mem_write(base, bytes(0x100))
    for t, e in zip(TASKS, ENTS):
        cpu.mem_write(t + 0x34, struct.pack(">I", e))
    for tbl, obj in ((P1T, ENT), (P2T, PL)):
        cpu.mem_write(tbl + 4 * inp["slot"], struct.pack(">I", obj))
    cpu.mem_write(ENT + 0x14, struct.pack(">3f", *inp["position"]))
    cpu.mem_write(PL + 1, struct.pack(">b", inp["character"]))
    cpu.reg_write(R.UC_PPC_REG_3, inp["slot"])
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=20000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases, seen = 0, set()
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(1500):
        rings = rng.choice((0, 0, 1, 2, 7, 19, 20, 21, 50, 999, -3, 0x10005))
        inp = dict(path="ring_drop", slot=rng.choice((0, 1)), rings=rings,
                   character=rng.choice((9, 10, 6, 7, 0)),
                   position=[f32(rng.uniform(-1500, 1500)) for _ in range(3)],
                   random=rng.choice((0, 32767, rng.randrange(32768))),
                   allocated=[rng.random() < 0.85 for _ in range(20)])
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
                ent = ENTS[r["index"]]
                assert (e["address"] - ent, e["size"], e["value"]) == (r["offset"], r["size"], r["value"]), (n, r, e)
                continue
            assert r["call"] == e["call"], (n, r, e)
            assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
        seen.update(e["site"] for e in native)
        cases += 1
assert {0x19528, 0x1952C, 0x19544, 0x19598, 0x19600, 0x19628} <= seen
print(f"{cases} installed PPC ring drops matched")