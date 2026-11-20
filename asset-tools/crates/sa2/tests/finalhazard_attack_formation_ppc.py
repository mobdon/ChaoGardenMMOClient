"""Compare the Rust Finalhazard attack formation and hold with installed PPC.

Usage: python finalhazard_attack_formation_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x59b0 (formation spawn) and 0x5c04 (hold)
to their return. Engine imports and local calls are observed and fed the
request's results; every call (with stack vectors) and every store is
compared."""
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
START, END = 0x59B0, 0x5C4C
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, block=0x82000, out=0x83000)

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
cpu.mem_map(0, 0x400000)
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
            windows[key] = 0x200000 + 0x8000 * len(windows)
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
        if w <= v < w + 0x8000:
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
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(3, 9)]
    fl = [struct.unpack(">d", struct.pack(">Q", u.reg_read(getattr(R, f"UC_PPC_REG_FPR{k}"))))[0]
          for k in (1, 2, 3, 4)]
    f1 = fl
    vectors = {str(i): vec(u, r) for i, r in enumerate(regs[:3]) if 0x1000 <= r < 0x1FFFF4}
    if off in imports and 0x8010AC50 <= imports[off] <= 0x8010ACD0:
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
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
        elif reg == "seqf":
            u.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", value.pop(0)))[0])
        elif reg == "seq":
            u.reg_write(R.UC_PPC_REG_3, value.pop(0) & 0xFFFFFFFF)
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
    events.append(dict(site=site, store="module", offset=symbol(addr), size=size,
                       value=symbol(value & ((1 << 8 * size) - 1))))


def compare(start, inp, n, regs=()):
    for k, w in windows.items():
        cpu.mem_write(w, pristine[k])
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    for r, v in regs:
        cpu.reg_write(r, v)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + start, RETURN + 4, count=20000)
    q, out = p / "request.json", p / f"{inp['path']}-{n}.json"
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
            assert r["floats"] == e["f1"][:len(r["floats"])], (n, r, e)
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


rv = lambda: [struct.unpack('>f', struct.pack('>f', random.uniform(-300, 300)))[0] for _ in range(3)]
cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(59)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
T, E, B = REGIONS["task"], REGIONS["entity"], REGIONS["block"]


def layout():
    cpu.mem_write(0x80000, bytes(0x8000))
    cpu.mem_write(T + 0x34, struct.pack(">I", E))
    cpu.mem_write(T + 0x40, struct.pack(">I", B))


cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(400):
        events, changes = [], {}
        layout()
        count = rng.choice((1, 2, 4, 5, 9, 10, 16, 17, 30))
        root = f32(count ** 0.5)
        floor = float(int(root))
        sid = rng.choice((0, 1))
        flags = rng.choice((0, 0x100, -1, 0x0FF))
        trig = [f32(rng.uniform(-1, 1)) for _ in range(200)]
        pieces = [0 if rng.random() < 0.05 else 0x9000 + 16 * n + i for i in range(100)]
        cpu.mem_write(B, struct.pack(">h", count))
        cpu.mem_write(E + 4, struct.pack(">h", flags))
        pristine[(88, 6, 0x4A8)] = struct.pack(">I", 0x86000) + bytes(0xFFC)
        cpu.mem_write(0x86000, struct.pack(">b", sid))
        tq, pq = list(trig), list(pieces)
        effects = {0x5A00: [("f1", root)], 0x5A04: [("f1", floor)],
                   0x5AA8: [("seqf", tq)], 0x5ABC: [("seqf", tq)], 0x5B08: [("seqf", tq)],
                   0x5B1C: [("seqf", tq)], 0x5B3C: [("seq", pq)]}
        compare(0x59B0, dict(path="attack_formation", task=T, count=count, root=root, floor=floor,
                             stage_id=sid, trig=trig, pieces=pieces, flags=flags), n,
                [(R.UC_PPC_REG_3, T)])
        cases += 1
    for n, (flags, timer) in enumerate(itertools.product((0, 0x100, -1), (0, 1, 2, 0xFFFF))):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(E + 4, struct.pack(">hH", flags, timer))
        compare(0x5C04, dict(path="attack_hold", flags=flags, timer=timer), n, [(R.UC_PPC_REG_3, T)])
        cases += 1
print(f"{cases} installed PPC attack formation and hold calls matched")