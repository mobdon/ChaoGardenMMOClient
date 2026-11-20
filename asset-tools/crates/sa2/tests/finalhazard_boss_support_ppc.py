"""Compare the Rust Finalhazard boss support routines with installed PPC.

Usage: python finalhazard_boss_support_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x1e04 (draw), 0x1ed8, 0x1f94, 0x1fc8,
0x2078 and 0x210c to their return. Engine imports are observed; parameter
blocks passed on the stack are read off it and compared word by word."""
import itertools
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
START, END = 0x1E04, 0x2138
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, data=0x82000, state=0x83000, collision=0x84000,
               colliders=0x85000, hit=0x86000, stage=0x87000, points=0x88000)

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
pristine = {k: bytes(cpu.mem_read(w, 0x1000)) for k, w in windows.items()}


def symbol(v):
    if STACK - 0x200 <= v < STACK:
        return "stack"
    if CODE <= v < CODE + 0x20000:
        return f"88:1+{v - CODE:#x}"
    for (mod, s, add), w in windows.items():
        if w <= v < w + 0x1000:
            return f"88:{s}+{add + v - w:#x}" if mod == 88 else add + v - w
    return v & 0xFFFFFFFF


def vec(u, addr):
    return list(struct.unpack(">3f", u.mem_read(addr, 12)))


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    for addr, fmt, value in changes.get(off, ()):
        u.mem_write(addr, struct.pack(fmt, value))
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(3, 8)]
    f1 = struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]
    vectors = {str(i): vec(u, r) for i, r in enumerate(regs[:3]) if 0x1000 <= r < 0x1FFFF4}
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs, f1=f1, vectors=vectors))
        size = {0x800CB438: 6, 0x800C82DC: 12}.get(target)
        if size:
            events[-1]["block"] = [symbol(w) for w in struct.unpack(f">{size}I", u.mem_read(regs[1], 4 * size))]
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs, f1=f1, vectors=vectors))
    for reg, value in effects.get(off, ()):
        if reg == "f1":
            u.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", value))[0])
        elif reg == "r3":
            u.reg_write(R.UC_PPC_REG_3, value & 0xFFFFFFFF)
        else:
            u.mem_write(regs[reg], struct.pack(">3f", *value))
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x200 <= addr < STACK + 0x100:
        return
    site = u.reg_read(R.UC_PPC_REG_PC) - CODE
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            events.append(dict(site=site, store=name, offset=addr - base, size=size,
                               value=value & ((1 << 8 * size) - 1)))
            return
    events.append(dict(site=site, store="module", offset=symbol(addr), size=size, value=value))


def compare(start, inp, n, regs=()):
    for k, w in windows.items():
        cpu.mem_write(w, pristine[k])
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    for r, v in regs:
        cpu.reg_write(r, v)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + start, RETURN + 4, count=20000)
    q, out = p / "request.json", p / f"{inp['path']}-{inp.get('routine')}-{n}.json"
    q.write_text(json.dumps(inp))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    got = json.loads(out.read_text())["events"]
    assert [e["site"] for e in got] == [e["site"] for e in events], (n, inp, got, events)
    for r, e in zip(got, events):
        if "store" in r:
            assert (r["store"], r["offset"], r["size"], r["value"]) == (
                e["store"], e["offset"], e["size"], e["value"]), (n, r, e)
            continue
        assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
        assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
        if "floats" in r:
            assert r["floats"] == [e["f1"]], (n, r, e)
        if "block" in r:
            assert r["block"] == e["block"], (n, r, e)
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
# The screen effect table at 5:0x2dd4 holds three relocated pointers.
table = windows[(88, 5, 0x2DD4)]
words = b""
for i in range(3):
    o = 0x2DD4 + 4 * i
    rel = next(add for mod, sec, at, add in [(88, 5, o, a) for a in (0x2DC8, 0x1878, 0x2CC4)][i:i + 1])
    key = (88, 5, rel)
    if key not in windows:
        windows[key] = 0x40000 + 0x1000 * len(windows)
        pristine[key] = bytes(0x1000)
    words += struct.pack(">I", windows[key])
pristine[(88, 5, 0x2DD4)] = words + pristine[(88, 5, 0x2DD4)][12:]
REGIONS["target"] = 0x89000
rng = random.Random(9)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
k4 = lambda o: struct.unpack_from(">f", data, section(4)[0] + o)[0]
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(40):
        events, changes, effects = [], {}, {}
        rot = [rng.randrange(1 << 32) for _ in range(3)]
        byte = rng.choice((0, 5, -3))
        cpu.mem_write(0x80000, bytes(0xA000))
        cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
        cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", REGIONS["data"]))
        cpu.mem_write(REGIONS["data"] + 0xC, struct.pack(">I", 0x7700 + n))
        cpu.mem_write(REGIONS["entity"] + 8, struct.pack(">III", *rot))
        pristine[(0, 9, 0x803AD526)] = struct.pack(">b", byte) + bytes(0xFFF)
        compare(0x1E04, dict(path="boss_support", routine="draw", entity=REGIONS["entity"],
                             rotation=rot, motion=0x7700 + n, byte=byte), n,
                [(R.UC_PPC_REG_3, REGIONS["task"])])
        cases += 1
    for n, (index, flag, count) in enumerate(itertools.product((0, 1, 2), (0, 1, 2, -1), (0, 13, 14, -1))):
        events, changes = [], {}
        f1 = f32(rng.uniform(-10, 10))
        effects = {0x1F58: [("r3", count)], 0x1F68: [("r3", 0x5150 + n)]}
        cpu.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", f1))[0])
        compare(0x1ED8, dict(path="boss_support", routine="screen_effect", index=index,
                             flag=flag, f1=f1, count=count, clock=0x5150 + n), n,
                [(R.UC_PPC_REG_3, index & 0xFFFFFFFF), (R.UC_PPC_REG_4, flag & 0xFFFFFFFF)])
        cases += 1
    events, changes, effects = [], {}, {0x1FA4: [("r3", 0x4242)]}
    compare(0x1F94, dict(path="boss_support", routine="screen_reset", clock=0x4242), 0)
    cases += 1
    edge = k4(0x118)
    for n in range(400):
        events, changes = [], {}
        z = f32(rng.choice((edge, -edge, edge * 0.5, -edge * 0.99, 0.0, -0.0, edge * 2, -edge * 3,
                            rng.uniform(-3 * edge, 3 * edge))))
        src = [f32(rng.uniform(-500, 500)), f32(rng.uniform(-500, 500)), z]
        flag = rng.choice((0, 1, -1))
        effects = {0x200C: [("f1", abs(-z))]}
        cpu.mem_write(0x80000, bytes(0xA000))
        cpu.mem_write(REGIONS["entity"], struct.pack(">3f", *src))
        compare(0x1FC8, dict(path="boss_support", routine="follow", source=src, flag=flag), n,
                [(R.UC_PPC_REG_3, REGIONS["entity"]), (R.UC_PPC_REG_4, REGIONS["target"]),
                 (R.UC_PPC_REG_5, flag & 0xFFFFFFFF)])
        cases += 1
    for n, first in enumerate((0, 1, 0x80001234, 0xFFFFFFFF)):
        events, changes, effects = [], {}, {}
        compare(0x2078, dict(path="boss_support", routine="effect_block", first=first), n,
                [(R.UC_PPC_REG_3, first)])
        cases += 1
    for n, kind in enumerate((0, 3, -1, 127)):
        events, changes, effects = [], {}, {}
        compare(0x210C, dict(path="boss_support", routine="effect_kind", kind=kind), n,
                [(R.UC_PPC_REG_3, kind & 0xFFFFFFFF)])
        cases += 1
print(f"{cases} installed PPC boss support calls matched")