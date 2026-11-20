"""Compare the Rust Finalhazard attack object update with installed PPC.

Usage: python finalhazard_attack_update_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x6158 (update, through its jump table),
0x5f34, 0x60a8, 0x5fe4, 0x612c and 0x6264 to their return. Engine imports
and local calls are observed; every call and every store is compared."""
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
START, END = 0x5F34, 0x62A0
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, block=0x82000, owner=0x83000, owner_block=0x84000, owner_state=0x85000, stage=0x86000)

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
    if off in imports and imports[off] in (0x8010AC84, 0x8010ACD0):
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
            assert r["floats"] == e["f1"][:len(r["floats"])], (n, r, e)
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


rv = lambda: [struct.unpack('>f', struct.pack('>f', random.uniform(-300, 300)))[0] for _ in range(3)]
cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(47)
T, E, B = REGIONS["task"], REGIONS["entity"], REGIONS["block"]
table = []
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
        elif t == 1 and sec == 5 and 0x3FFC <= pos < 0x3FFC + 36:
            table.append((pos, add))
pristine[(88, 5, 0x3FFC)] = b"".join(struct.pack(">I", CODE + a) for _, a in sorted(table)) + bytes(0x1000 - 36)


def layout():
    cpu.mem_write(0x80000, bytes(0x8000))
    cpu.mem_write(T + 0x34, struct.pack(">I", E))
    cpu.mem_write(T + 0x40, struct.pack(">I", B))


def run(start, inp, n):
    compare(start, dict(path="attack_update", **inp), n, [(R.UC_PPC_REG_3, T)])


cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, (pause, freeze, bstate, lock, action) in enumerate(itertools.product(
            (0, 1), (0, -1), (0, 1, -1), (0, 1), range(-1, 10))):
        events, changes = [], {}
        layout()
        pristine[(0, 6, 0x801CBD88)] = bytes(9) + struct.pack(">b", freeze) + bytes(0x2D) + struct.pack(">b", pause) + bytes(0xFC8)
        pristine[(88, 6, 0x4A8)] = struct.pack(">I", REGIONS["stage"]) + bytes(0xFFC)
        cpu.mem_write(REGIONS["stage"] + 1, struct.pack(">b", lock))
        cpu.mem_write(E, struct.pack(">b", action))
        effects = {0x619C: [("r3", bstate)]}
        run(0x6158, dict(routine="update", task=T, pause=pause, freeze=freeze, boss_state=bstate,
                         stage_lock=lock, action=action), n)
        cases += 1
    for n in range(300):
        events, changes = [], {}
        layout()
        g = rng.choice((0, 5, 10, 3, 0xFFFFFFFF, 0xFFFFFFFB, rng.randrange(1 << 32)))
        spawned = [rng.choice((0, 0x9100 + n)), rng.choice((0, 0x9200 + n))]
        pristine[(0, 6, 0x801CBD88)] = bytes(0x7C) + struct.pack(">I", g) + bytes(0xF80)
        effects = {0x5F94: [("seq", list(spawned))]}
        run(0x5F34, dict(routine="spawn", task=T, frame_count=g, spawned=spawned), n)
        effects = {0x60DC: [("seq", list(spawned))]}
        events = []
        run(0x60A8, dict(routine="burst", task=T, spawned=spawned), n)
        cases += 2
    for n in range(300):
        events, changes, effects = [], {}, {}
        layout()
        step = rng.choice((0, 0, 1, 2, -1))
        count = rng.choice((0, 1, 3, 8, -2))
        slots = [rng.choice((0, 0x9300 + 16 * n + i)) for i in range(max(count, 0))]
        child = rng.choice((0, 0x7000))
        after = rng.randrange(256)
        cpu.mem_write(E + 1, struct.pack(">b", step))
        cpu.mem_write(B, struct.pack(">h", count))
        cpu.mem_write(T + 0xC, struct.pack(">I", child))
        pristine[(88, 6, 0x274)] = b"".join(struct.pack(">I", w) for w in slots) + bytes(0x1000 - 4 * len(slots))
        changes = {0x6068: [(E + 1, ">B", after)]}
        run(0x5FE4, dict(routine="sweep", step=step, count=count, slots=slots, step_after=after,
                         child=child), n)
        cases += 1
    for n, timer in enumerate((0, 1, 2, 0xFFFF)):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(E + 6, struct.pack(">H", timer))
        run(0x612C, dict(routine="rest", timer=timer), n)
        cases += 1
    O, OB, OS = REGIONS["owner"], REGIONS["owner_block"], REGIONS["owner_state"]
    for n, (owner, ob, os_, link) in enumerate(itertools.product((0, O), (0, OB), (0, OS), (T, 0x1234))):
        events, changes, effects = [], {}, {}
        layout()
        cpu.mem_write(T + 8, struct.pack(">I", owner))
        cpu.mem_write(O + 0x40, struct.pack(">I", ob))
        cpu.mem_write(OB + 0x10, struct.pack(">I", os_))
        cpu.mem_write(OS + 0x14, struct.pack(">I", link))
        run(0x6264, dict(routine="delete", task=T, owner=owner, owner_block=ob, owner_state=os_,
                         link=link), n)
        cases += 1
print(f"{cases} installed PPC attack update calls matched")