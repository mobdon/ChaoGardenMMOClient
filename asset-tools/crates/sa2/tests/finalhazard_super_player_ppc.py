"""Compare the Rust super-form player update with installed PPC instructions.

Usage: python finalhazard_super_player_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xad8c to its return with the player,
entity, collision and effect objects in emulated memory. Engine imports and
the local movement handlers are observed and answered; the local fabs at
+0x2130 runs natively. Every call, its arguments and every non-stack store
are compared in order.
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

relocs, data_relocs = {}, {}
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
        elif t != 201:
            data_relocs[(sec, pos)] = (t, mod, s, add)

CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
RANGES = [(0x2130, 0x2138), (0xAD8C, 0xBAD0)]
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
for a, b in RANGES:
    cpu.mem_write(CODE + a, data[code + a:code + b])
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not any(a <= at < b for a, b in RANGES):
        continue
    if t == 10:
        imports[at] = add
        continue
    assert t in (4, 6), (hex(at), t)
    key = (mod, s, add)
    if key not in windows:
        windows[key] = 0x40000 + 0x1000 * len(windows)
        start, size = section(s) if mod == 88 else (0, 0)
        if start:
            cpu.mem_write(windows[key], data[start + add:start + min(size, add + 0x1000)])
    dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))
assert 0x40000 + 0x1000 * len(windows) <= 0x80000
# The action jump table (section 5 + 0x3fd50) holds relocated code addresses.
JUMPS = windows[(88, 5, 0x3FD50)]
for i in range(68):
    t, mod, s, add = data_relocs[(5, 0x3FD50 + 4 * i)]
    assert (t, mod, s) == (1, 88, 1)
    cpu.mem_write(JUMPS + 4 * i, struct.pack(">I", CODE + add))
GLOBAL = windows[(0, 6, 0x801CBD88)]

TASK, ENT, D2, PL, COL, INNER, FX = (0x80000 + 0x1000 * i for i in range(7))
cpu.mem_write(TASK + 0x34, struct.pack(">III", ENT, D2, 0) + struct.pack(">I", PL))
cpu.mem_write(ENT + 0x2C, struct.pack(">I", COL))
cpu.mem_write(COL + 0xC, struct.pack(">I", INNER))
REGIONS = [("entity", ENT), ("collision", COL), ("inner", INNER), ("player", PL), ("fx", FX)]
SAVE, RESTORE = {0x8010AC50}, {0x8010AC9C}
saved = []
f2d = lambda f: struct.unpack(">Q", struct.pack(">d", f))[0]
d2f = lambda q: struct.unpack(">d", struct.pack(">Q", q))[0]


def vec(u, p):
    try:
        return list(struct.unpack(">3f", u.mem_read(p, 12)))
    except Exception:
        return None


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = lambda *ns: [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in ns]
    fprs = [d2f(u.reg_read(getattr(R, f"UC_PPC_REG_FPR{n}"))) for n in (1, 2, 3)]
    r3, r4, r5, r6 = regs(3, 4, 5, 6)
    if off in imports:
        target = imports[off]
        if target in SAVE:
            saved.append(regs(*range(14, 32)))
        elif target in RESTORE:
            for n, v in zip(range(14, 32), saved.pop()):
                u.reg_write(getattr(R, f"UC_PPC_REG_{n}"), v)
        else:
            events.append(dict(site=off, call=target, args=[r3, r4, r5, r6], floats=fprs,
                               vectors=[vec(u, r3), vec(u, r4)]))
            ret, fret = 0, None
            if target == 0x80114EAC:
                fret = inputs["lengths"][counters["lengths"]]
                counters["lengths"] += 1
            elif target == 0x8010D940:
                ret = inputs["randoms"][counters["randoms"]]
                counters["randoms"] += 1
            elif target in (0x80112050, 0x80112068):
                fret = inputs["trig"][counters["trig"]]
                counters["trig"] += 1
            elif target == 0x8002ABF4:
                fret = inputs["approach"]
            elif target == 0x8004FDA0:
                ret = inputs["defeat_query"]
            u.reg_write(R.UC_PPC_REG_3, ret & 0xFFFFFFFF)
            if fret is not None:
                u.reg_write(R.UC_PPC_REG_FPR1, f2d(fret))
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
    li = ins & 0x3FFFFFC
    if li & 0x2000000:
        li -= 0x4000000
    target = off + li
    if target == 0x2130:  # local fabs runs natively
        return
    events.append(dict(site=off, local=target, args=[r3, r4, r5, r6], floats=fprs,
                       vectors=[vec(u, r3), vec(u, r4)]))
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
f32 = lambda v: struct.unpack(">f", struct.pack(">f", v))[0]


def run(inp):
    global events, inputs, counters
    events, inputs = [], inp
    counters = dict(lengths=0, randoms=0, trig=0)
    for base in (ENT, COL, INNER, PL, FX):
        cpu.mem_write(base, bytes(0x1000))
    cpu.mem_write(ENT + 0x2C, struct.pack(">I", COL))
    cpu.mem_write(COL + 0xC, struct.pack(">I", INNER))
    cpu.mem_write(GLOBAL + 0x37, struct.pack(">b", inp["enabled_mask"]))
    cpu.mem_write(ENT, struct.pack(">b", inp["action"]))
    cpu.mem_write(ENT + 4, struct.pack(">h", inp["entity_flags"]))
    cpu.mem_write(COL + 4, struct.pack(">H", inp["collision_flags"]))
    cpu.mem_write(INNER + 3, bytes([inp["inner_03"]]))
    cpu.mem_write(INNER + 4, struct.pack(">I", inp["inner_04"]))
    cpu.mem_write(INNER + 0x34, struct.pack(">I", inp["inner_34"]))
    cpu.mem_write(PL, struct.pack(">bb", inp["slot"], inp["character"]))
    cpu.mem_write(PL + 0x10, struct.pack(">h", inp["flags_10"]))
    for off, key in ((0x68, "f68"), (0xEC, "fec"), (0x134, "f134"), (0x184, "f184")):
        cpu.mem_write(PL + off, struct.pack(">f", inp[key]))
    cpu.mem_write(PL + 0x176, struct.pack(">hh", inp["anim_176"], inp["anim_178"]))
    cpu.mem_write(PL + 0x35C, struct.pack(">bxh", inp["counter_35c"], inp["timer_35e"]))
    for off, key in ((0x218, "v218"), (0x230, "v230"), (0x360, "v360")):
        cpu.mem_write(PL + off, struct.pack(">3f", *inp[key]))
    fx = inp["fx"]
    cpu.mem_write(PL + 0x36C, struct.pack(">I", FX if fx else 0))
    if fx:
        cpu.mem_write(FX + 4, struct.pack(">f", fx["f04"]))
        cpu.mem_write(FX + 0xC, struct.pack(">i", fx["i0c"]))
    cpu.reg_write(R.UC_PPC_REG_3, TASK)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + 0xAD8C, RETURN + 4, count=200000)
    assert not saved, saved
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases = 0
seen = set()
actions = (0, 15, 53, 57, 58, 59, 60, 61, 62, 63, 64, 67, 1, 30, 54, 65, -3, 100)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(1500):
        slot = rng.choice((0, 1))
        inp = dict(
            path="super_player",
            addresses=dict(task=TASK, entity=ENT, data2=D2, player=PL, collision=COL,
                           inner=INNER, fx=FX),
            enabled_mask=rng.choice((0, 0, 0, 1, 2, 3)),
            action=rng.choice(actions),
            entity_flags=rng.choice((0, 0x400, -1, 0x3501, 0x7FFF)),
            collision_flags=rng.choice((0, 0x1F, 0xFFFF, 0x1234)),
            inner_03=rng.randrange(256), inner_04=rng.choice((0, 0x4000, 0xFFFFFFFF, 0x12345678)),
            inner_34=rng.choice((0, 0x10, 0xFFFFFFFF)),
            slot=slot, character=rng.choice((9, 10, 9, 10, 0)),
            flags_10=rng.choice((0, -0x8000, 0x7FFF, -1)),
            f68=f32(rng.uniform(-3, 3)), fec=f32(rng.choice((0.5, 2.0, 100.0))),
            f134=f32(rng.uniform(0, 20)), f184=f32(rng.uniform(-5, 5)),
            anim_176=rng.choice((0, 104, 7)),
            anim_178=rng.choice((0, 54, 85, 99, 100, 102, 103, 104, 112, 113, 114, 50)),
            counter_35c=rng.choice((0, 4, 5, 6, -2)), timer_35e=rng.choice((0, 39, 40, 41, 100)),
            v218=[f32(rng.uniform(-50, 50)) for _ in range(3)],
            v230=[f32(rng.uniform(-50, 50)) for _ in range(3)],
            v360=[f32(rng.uniform(-50, 50)) for _ in range(3)],
            fx=None if rng.random() < 0.25 else dict(f04=f32(rng.uniform(-3, 3)),
                                                     i0c=rng.choice((0, 0x4000, -5, 0x7FFF0000))),
            lengths=[f32(rng.choice((0.0, 0.3, 0.5, 1.5, 2.0, 3.0, 9.0))) for _ in range(6)],
            approach=f32(rng.uniform(-3, 3)),
            randoms=[rng.choice((0, 32767, rng.randrange(32768))) for _ in range(120)],
            trig=[f32(rng.uniform(-1, 1)) for _ in range(80)],
            defeat_query=rng.choice((0, 0, 1)))
        native = run(inp)
        q = p / "request.json"
        out = p / f"{n}.json"
        q.write_text(json.dumps(inp))
        res = subprocess.run([exe, "finalhazard-placement-clock", module, out, q],
                             capture_output=True, text=True)
        assert res.returncode == 0, (n, res.stderr[-500:])
        got = json.loads(out.read_text())["events"]
        assert len(got) == len(native), (n, [e["site"] for e in got], [e["site"] for e in native])
        for r, e in zip(got, native):
            if "store" in r:
                assert r == e, (n, r, e)
                continue
            for key in ("site", "call", "local"):
                assert r.get(key) == e.get(key), (n, key, r, e)
            for key in ("args", "floats", "vectors"):
                if key in r:
                    want = e[key][:len(r[key])]
                    if key == "args":
                        want = [v & 0xFFFFFFFF for v in want]
                    assert r[key] == want, (n, key, r, e)
        seen.update(e["site"] for e in native)
        cases += 1
required = {0xAE1C, 0xAE78, 0xAEE0, 0xAEF0, 0xAF24, 0xAF94, 0xAF84, 0xAFE8, 0xB094, 0xB0C4,
            0xB104, 0xB0F0, 0xB180, 0xB1A8, 0xB210, 0xB278, 0xB2E0, 0xB364, 0xB398, 0xB3AC,
            0xB410, 0xB6AC, 0xB6C8, 0xB720, 0xB808, 0xB87C, 0xB8A8, 0xB93C, 0xB9BC, 0xB9E8,
            0xBA0C, 0xBA7C, 0xBA6C}
assert required <= seen, sorted(hex(s) for s in required - seen)
print(f"{cases} installed PPC super-player updates matched")
