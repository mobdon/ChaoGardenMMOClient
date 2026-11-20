"""Compare the Rust Finalhazard boss per-frame routines with installed PPC.

Usage: python finalhazard_boss_frame_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x638 (deleter), + 0x690 (push a point
out along a direction), + 0x744 (hit check) and + 0x920 (attachment points)
to their return. Engine imports and local calls are observed; stack
pointers are compared as "stack" and the vectors they hold as floats.
Every call and every store is compared in order.
"""
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
START, END = 0x638, 0x9D4
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
    q, out = p / "request.json", p / f"{inp['path']}-{n}.json"
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
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(3)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
rv = lambda: [f32(rng.uniform(-300, 300)) for _ in range(3)]
k4 = lambda o: struct.unpack_from(">f", data, section(4)[0] + o)[0]
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, block in enumerate((0, 0x9100)):
        events, changes, effects = [], {}, {}
        cpu.mem_write(0x80000, bytes(0x9000))
        cpu.mem_write(REGIONS["task"] + 0x38, struct.pack(">I", block))
        compare(0x638, dict(path="boss_delete", task=REGIONS["task"], block=block), n,
                [(R.UC_PPC_REG_3, REGIONS["task"])])
        cases += 1
    for n in range(200):
        events, changes = [], {}
        extra, dist, direction = f32(rng.uniform(-50, 200)), f32(rng.uniform(0, 900)), rv()
        effects = {0x6C8: [("f1", dist)], 0x6E4: [(0, direction)]}
        cpu.mem_write(0x80000, bytes(0x9000))
        cpu.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", extra))[0])
        compare(0x690, dict(path="boss_aim", **{"from": 0x81008, "to": 0x82014, "out": 0x83000},
                            extra=extra, distance=dist, direction=direction), n,
                [(R.UC_PPC_REG_3, 0x81008), (R.UC_PPC_REG_4, 0x82014), (R.UC_PPC_REG_5, 0x83000)])
        cases += 1
    health_edges = [k4(0xC0) + k4(0xAC) + d for d in (-1, 0, 0.5, 1)] + [100.0, -5.0]
    for n, (child_hit, action, health, hit, collider, kind) in enumerate(itertools.product(
            (1, 0, -1), (3, 0, 13), health_edges, (0, REGIONS["hit"]), (0, 2, -1), (1, 0, 2))):
        events = []
        cpu.mem_write(0x80000, bytes(0x9000))
        base = 0 if collider == 0 and n % 2 else REGIONS["colliders"] + 0x100
        sid = n % 2
        player = 0x6000 + n
        flags = rng.choice((0, 4, -1, 0x1230))
        point, offset = rv(), rv()
        cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
        cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", REGIONS["data"]))
        cpu.mem_write(REGIONS["data"] + 0x10, struct.pack(">I", REGIONS["state"]))
        cpu.mem_write(REGIONS["data"] + 0x28, struct.pack(">f", health))
        cpu.mem_write(REGIONS["state"] + 0x10, struct.pack(">I", 0x7000 + n))
        cpu.mem_write(REGIONS["entity"] + 0x2C, struct.pack(">I", REGIONS["collision"]))
        cpu.mem_write(REGIONS["collision"] + 0xC, struct.pack(">I", base))
        cpu.mem_write(REGIONS["hit"], struct.pack(">b", collider))
        changes = {0x784: [(REGIONS["entity"], ">b", action)],
                   0x7E0: [(REGIONS["entity"] + 4, ">h", flags)]}
        effects = {0x774: [("r3", child_hit)], 0x7F4: [("r3", hit)], 0x884: [("r3", kind)],
                   0x868: [(2, point)], 0x878: [(2, offset)]}
        stage_ptr = windows[(88, 6, 0x4A8)]
        players = windows[(0, 7, 0x801E7368)]
        for k, w in windows.items():
            cpu.mem_write(w, pristine[k])
        pristine[(88, 6, 0x4A8)] = struct.pack(">I", REGIONS["stage"]) + bytes(0xFFC)
        pristine[(0, 7, 0x801E7368)] = struct.pack(">II", player if sid == 0 else 0,
                                                   player if sid == 1 else 0) + bytes(0xFF8)
        cpu.mem_write(REGIONS["stage"], struct.pack(">b", sid))
        compare(0x744, dict(path="boss_hit", task=REGIONS["task"], child=0x7000 + n,
                            child_hit=child_hit, action=action, health=health,
                            entity_flags=flags, hit=hit, collider=collider, colliders=base,
                            player=player, point=point, offset=offset, hit_kind=kind), n,
                [(R.UC_PPC_REG_3, REGIONS["task"])])
        cases += 1
    events, changes, effects = [], {}, {}
    compare(0x920, dict(path="boss_points", task=REGIONS["task"]), 0,
            [(R.UC_PPC_REG_3, REGIONS["task"])])
    cases += 1
print(f"{cases} installed PPC boss frame routine calls matched")
