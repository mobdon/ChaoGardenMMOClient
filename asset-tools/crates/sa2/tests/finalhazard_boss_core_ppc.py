"""Compare the Rust Finalhazard boss core object with installed PPC.

Usage: python finalhazard_boss_core_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs the boss core routines in Boss_Last2D.rel section 1 + 0x8328..0x8b0c
(update, construction, reset, stage set, hit query, clear, pattern lookup,
release) to their return. Engine imports and local calls are observed;
memory the code reads after a call is changed at that read. Every call,
every store and the returned r3 are compared."""
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
START, END = 0x8328, 0x8B0C
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, block=0x82000, collision=0x84000)

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
                               value=symbol(value & ((1 << 8 * size) - 1))))
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
    q, out = p / "request.json", p / f"{inp['path']}-{inp['routine']}-{n}.json"
    q.write_text(json.dumps(inp))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    report = json.loads(out.read_text())
    got = report["events"]
    if "returned" in report:
        assert report["returned"] == cpu.reg_read(R.UC_PPC_REG_3), (n, inp, report)
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
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(13)
T, E, B, C = REGIONS["task"], REGIONS["entity"], REGIONS["block"], REGIONS["collision"]
PAUSE = windows[(0, 6, 0x801CBD88)] + 0x37


def layout():
    cpu.mem_write(0x80000, bytes(0x6000))
    cpu.mem_write(T + 0x34, struct.pack(">I", E))
    cpu.mem_write(T + 0x40, struct.pack(">I", B))


def run(start, inp, n, regs):
    compare(start, dict(path="boss_core", **inp), n, regs)


cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, (pause, action) in enumerate(itertools.product((0, 1, -1), range(-1, 7))):
        events, changes, effects = [], {}, {}
        layout()
        pristine[(0, 6, 0x801CBD88)] = bytes(0x37) + struct.pack(">b", pause) + bytes(0xFC8)
        cpu.mem_write(E, struct.pack(">b", action))
        run(0x8328, dict(routine="update", task=T, pause=pause, action=action), n,
            [(R.UC_PPC_REG_3, T)])
        cases += 1
    for n, (parent, task, block, stage, cflags) in enumerate(itertools.product(
            (0, 0x7000), (0, T), (0, B), (0, 3, 0xFF), (0, 0xC, 0xF3, 0x30))):
        events, changes = [], {}
        layout()
        cpu.mem_write(C + 3, struct.pack(">B", cflags))
        effects = {0x8564: [("r3", task)], 0x85A0: [("r3", block)], 0x8638: [("r3", C)]}
        run(0x8528, dict(routine="construct", parent=parent, stage=stage, task=task, block=block,
                         collision_flags=cflags), n,
            [(R.UC_PPC_REG_3, parent), (R.UC_PPC_REG_4, stage)])
        cases += 1
    events, changes, effects = [], {}, {}
    layout()
    run(0x866C, dict(routine="reset", task=T), 0, [(R.UC_PPC_REG_3, T)])
    cases += 1
    for n, (value, bstage, action) in enumerate(itertools.product(
            (0, 2, -1, 5), (0, 2, -1), (2, 3, 1, 0, -1))):
        events = []
        layout()
        bslot, slot = rng.randrange(256), rng.randrange(256)
        cword = rng.randrange(1 << 32)
        cpu.mem_write(B, struct.pack(">bB", bstage, bslot))
        changes = {0x8714: [(E, ">b", action)], 0x8784: [(C + 4, ">I", cword)]}
        effects = {0x8704: [("r3", slot)], 0x8710: [("r3", slot)], 0x8780: [("r3", C)]}
        run(0x86B8, dict(routine="set_stage", task=T, value=value, block_stage=bstage,
                         block_slot=bslot, slot=slot, action=action, collision_word=cword), n,
            [(R.UC_PPC_REG_3, T), (R.UC_PPC_REG_4, value & 0xFFFFFFFF)])
        cases += 1
    for n, (task, flags) in enumerate(itertools.product((0, T), (0, 4, -1, 0x7FFB))):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(E + 4, struct.pack(">h", flags))
        run(0x87E4, dict(routine="hit_query", task=task, flags=flags), n, [(R.UC_PPC_REG_3, task)])
        changes = {0x885C: [(E + 4, ">h", flags)]}
        events = []
        run(0x8818, dict(routine="clear", task=task, flags=flags), n, [(R.UC_PPC_REG_3, task)])
        cases += 2
    for n, (bs, bl) in enumerate(itertools.product(range(0, 6), range(0, 8))):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(B, struct.pack(">bb", bs, bl))
        run(0x8880, dict(routine="pattern", task=T, block_stage=bs, block_slot=bl), n,
            [(R.UC_PPC_REG_3, T)])
        cases += 1
    for n, block in enumerate((0, 0x9100)):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(T + 0x38, struct.pack(">I", block))
        run(0x8AC8, dict(routine="release", block=block), n, [(R.UC_PPC_REG_3, T)])
        cases += 1
print(f"{cases} installed PPC boss core calls matched")