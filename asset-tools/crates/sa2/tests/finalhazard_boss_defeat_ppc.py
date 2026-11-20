"""Compare the Rust Finalhazard boss defeat (action 13) with installed PPC.

Usage: python finalhazard_boss_defeat_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x1844 to its return; the beam parameter
block passed to 0x3e58 is read off the stack and compared too. Engine imports and local calls are observed; memory the
code reads again after a call is changed at that read. Every call (with
float arguments) and every store is compared in order.
"""
import itertools
import json
import random
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
START, END = 0x1844, 0x1BB8
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, data=0x82000, state=0x83000, motion=0x84000,
               stage=0x85000)

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


def symbol(v):
    if CODE <= v < CODE + 0x20000:
        return f"88:1+{v - CODE:#x}"
    for (mod, s, add), w in windows.items():
        if w <= v < w + 0x1000:
            return f"88:{s}+{add + v - w:#x}" if mod == 88 else add + v - w
    return v & 0xFFFFFFFF


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
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs, f1=f1))
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs, f1=f1))
        if target == 0x3E58:
            a, b, x, y, z = struct.unpack(">BBxxIII", u.mem_read(regs[1], 0x10))
            events[-1]["block"] = [a, b, symbol(x), symbol(y), symbol(z)]
    if target in results:
        u.reg_write(R.UC_PPC_REG_3, results[target] & 0xFFFFFFFF)
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x100 <= addr < STACK + 0x100:
        return
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=name,
                               offset=addr - base, size=size, value=value & ((1 << 8 * size) - 1)))
            return
    raise AssertionError(hex(addr))


def layout(step, child, handles=(0, 0)):
    cpu.mem_write(0x80000, bytes(0x6000))
    cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
    cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", REGIONS["data"]))
    cpu.mem_write(REGIONS["data"] + 0xC, struct.pack(">I", REGIONS["motion"]))
    cpu.mem_write(REGIONS["data"] + 0x10, struct.pack(">I", REGIONS["state"]))
    cpu.mem_write(REGIONS["state"] + 0x10, struct.pack(">III", child, *handles))
    cpu.mem_write(REGIONS["entity"] + 1, struct.pack(">b", step))


def compare(inp, n):
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_3, REGIONS["task"])
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4,
                  count=10000)
    q, out = p / "request.json", p / f"{n}.json"
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
        if "block" in r:
            assert r["block"] == e["block"], (n, r, e)
        if "floats" in r:
            assert r["floats"] == [e["f1"]], (n, r, e)


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
k4 = lambda o: struct.unpack_from(">f", data, section(4)[0] + o)[0]
edges = sorted({k4(o) for o in (0xF8, 0xFC, 0x100, 0x104, 0x108, 0x10C, 0x110, 0x114)})
frames = sorted({e + d for e in edges for d in (-0.5, 0.0, 0.5)} | {-1.0, edges[-1] + 50})
COUNTER = windows[(0, 6, 0x801CBD88)] + 0x7C
rng = random.Random(7)
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(1500):
        events = []
        step = rng.choice((0, 0, 1, 1, 1, 2, -1))
        handles = [rng.choice((0, 0x9100 + n)), rng.choice((0, 0x9200 + n))]
        child = 0x7000 + n
        layout(step, child, handles)
        cpu.mem_write(windows[(88, 6, 0x4A8)], struct.pack(">I", REGIONS["stage"]))
        sid = rng.choice((3, -100))
        cpu.mem_write(REGIONS["stage"], struct.pack(">b", sid))
        sa = [rng.randrange(256), rng.randrange(256)]
        fr = [rng.choice(frames) for _ in range(3)]
        g = [rng.choice((0, 45, 90, 30, 10, 60, 450, 900, 7, 0xFFFFFFFF, 0xFFFFFFF0, 4294967290,
                         rng.randrange(1 << 32))) for _ in range(3)]
        flags = rng.choice((0, 4, -1, 0x1234))
        done = rng.choice((0, 1))
        changes = {0x18FC: [(REGIONS["entity"] + 4, ">h", flags)],
                   0x1910: [(REGIONS["entity"] + 1, ">B", sa[0])],
                   0x191C: [(REGIONS["motion"] + 0x1C, ">f", fr[0])],
                   0x193C: [(COUNTER, ">I", g[0])],
                   0x1AA0: [(COUNTER, ">I", g[1])],
                   0x1AC4: [(REGIONS["motion"] + 0x1C, ">f", fr[1])],
                   0x1AE4: [(COUNTER, ">I", g[2])],
                   0x1B08: [(REGIONS["motion"] + 0x1C, ">f", fr[2])],
                   0x1B74: [(REGIONS["entity"] + 1, ">B", sa[1])]}
        results = {0x8002CB2C: done, 0x800C3974: 0x5A5A0000 + n}
        compare(dict(path="boss_defeat", task=REGIONS["task"], motion=REGIONS["motion"],
                     step=step, step_after=sa, handles=handles, child=child,
                     entity_flags=flags, motion_frame=fr, frame_count=g, motion_done=done,
                     clock=0x5A5A0000 + n, stage_id=sid), n)
        cases += 1
print(f"{cases} installed PPC boss defeat frames matched")