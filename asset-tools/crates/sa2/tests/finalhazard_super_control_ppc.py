"""Compare the Rust super-form action selection with installed PPC instructions.

Usage: python finalhazard_super_control_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xbad0 to its return. Engine queries and
the local checks 0xdf44, 0xe120 and 0xe200 return the request's values;
0xe1ac is observed. Every call, local call, argument and non-stack store
is compared in order.
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
START, END = 0xBAD0, 0xC4FC
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
JUMPS = windows[(88, 5, 0x3FE60)]
for i in range(68):
    t, mod, s, add = data_relocs[(5, 0x3FE60 + 4 * i)]
    cpu.mem_write(JUMPS + 4 * i, struct.pack(">I", CODE + add))
BUTTONS = [windows[(0, 9, a)] for a in (0x803AD498, 0x803AD494, 0x803AD488, 0x803AD484)]
PAD = windows[(0, 7, 0x801E5038)]
ENT, D2, PL = 0x80000, 0x81000, 0x82000
REGIONS = [("entity", ENT), ("data2", D2), ("player", PL)]
CHECKS = {0xDF44: 0, 0xE120: 1, 0xE200: 2}
f32 = lambda v: struct.unpack(">f", struct.pack(">f", v))[0]


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6, 7)]
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs))
        ret = 0
        if target == 0x800554CC:  # engine hit check updates entity halfword 6
            u.mem_write(ENT + 6, struct.pack(">H", inp["hit_timer_after"]))
        elif target == 0x8004E2F8:
            ret = inp["held"]
        elif target == 0x8004E228:
            ret = inp["landed"]
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs))
        ret = inp["checks"][CHECKS[target]] if target in CHECKS else 0
    u.reg_write(R.UC_PPC_REG_3, ret & 0xFFFFFFFF)
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


def run():
    global events
    events = []
    for base in (ENT, D2, PL, PAD, *BUTTONS):
        cpu.mem_write(base, bytes(0x400))
    cpu.mem_write(ENT, struct.pack(">bbxxhH", inp["action"], inp["action_previous"],
                                   inp["entity_flags"], inp["hit_timer"]))
    cpu.mem_write(D2 + 0x1C, struct.pack(">I", inp["data2_1c"]))
    cpu.mem_write(PL, struct.pack(">bb", inp["slot"], inp["character"]))
    cpu.mem_write(PL + 0x10, struct.pack(">h", inp["flags_10"]))
    cpu.mem_write(PL + 0x64, struct.pack(">ff", inp["speed"], inp["f68"]))
    cpu.mem_write(PL + 0xE8, struct.pack(">f", inp["fe8"]))
    cpu.mem_write(PL + 0x176, struct.pack(">hh", inp["anim_176"], inp["anim_178"]))
    cpu.mem_write(PL + 0x180, struct.pack(">h", inp["flags_180"]))
    cpu.mem_write(PL + 0x35E, struct.pack(">h", inp["timer_35e"]))
    for table, value in zip(BUTTONS, inp["buttons"]):
        cpu.mem_write(table + inp["slot"], struct.pack(">b", value))
    cpu.mem_write(PAD + 52 * inp["slot"] + 0x10, struct.pack(">I", inp["pad_10"]))
    for r, v in zip((3, 4, 5, 6), (ENT, D2, PL, PL)):
        cpu.reg_write(getattr(R, f"UC_PPC_REG_{r}"), v)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=10000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases, seen = 0, set()
actions = (0, 59, 60, 61, 62, 63, 64, 67, 12, 15, 18, 53, 66, 54, 58, 1, 57, -2, 99)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(4000):
        slot = rng.choice((0, 1))
        inp = dict(path="super_control", entity=ENT, data2=D2, player=PL,
                   action=rng.choice(actions), action_previous=rng.choice((15, 0, 59)),
                   entity_flags=rng.choice((0, 0x400, -0x8000, -1, 0x1404, 4)),
                   hit_timer=rng.choice((0, 0, 0, 1, 5)), slot=slot,
                   character=rng.choice((9, 10, 0)),
                   flags_10=rng.choice((0, 0, -0x8000, 3, 1, -1)),
                   anim_176=rng.choice((0, 18, 21, 22, 87, 99)),
                   anim_178=rng.choice((0, 99, 109, 116)),
                   flags_180=rng.choice((0, 1, 2, -1)), timer_35e=rng.choice((0, 119, 120, 121, 500)),
                   f68=f32(rng.choice((0.0, 2.7, -2.7, 3.0, -3.0, 2.0, -2.0))),
                   fe8=f32(rng.uniform(0, 5)), speed=f32(rng.choice((0.0, -1.0, 1.0))),
                   data2_1c=rng.randrange(1 << 32), pad_10=rng.choice((0, 4, 0xFFFFFFFB, 0xFFFFFFFF)),
                   hit_timer_after=rng.choice((0, 0, 30)),
                   held=rng.choice((0, 1)), landed=rng.choice((0, 1)),
                   buttons=[rng.choice((0, 0, 1, -1)) for _ in range(4)],
                   checks=[rng.choice((0, 0, 0, 1)) for _ in range(3)])
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
            if "store" in r:
                assert r == e, (n, r, e)
                continue
            assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
            assert r["args"] == [v & 0xFFFFFFFF for v in e["args"][:len(r["args"])]], (n, r, e)
        seen.update(e["site"] for e in native)
        cases += 1
stores = {0xBB24, 0xBBCC, 0xBBF4, 0xBC04, 0xBC10, 0xBCB0, 0xBD10, 0xBD94, 0xBE88, 0xBEE8,
          0xBF70, 0xBFEC, 0xC030, 0xC07C, 0xC0C0, 0xC100, 0xC144, 0xC184, 0xC1C8, 0xC208,
          0xC274, 0xC2A0, 0xC318, 0xC340, 0xC3B8, 0xC398, 0xC418, 0xC460, 0xC49C, 0xC4D8}
assert stores <= seen, sorted(hex(s) for s in stores - seen)
print(f"{cases} installed PPC super-control updates matched")