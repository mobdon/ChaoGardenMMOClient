"""Compare the Rust super-form draw routine with installed PPC instructions.

Usage: python finalhazard_super_draw_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xd2dc to its return with the engine
tables, effect block and swap target in emulated memory. Every call, its
arguments (module and code addresses translated to "88:SECTION+OFFSET")
and every store outside the stack is compared in order.
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
START, END = 0xD2DC, 0xD60C
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
                           floats=[struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]],
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
MODELS = windows[(0, 7, 0x801E5928)]
WORKPTR = windows[(88, 6, 0x4A8)]
MODE = windows[(0, 9, 0x803AD524)]
RESTORE_MODE = windows[(0, 9, 0x803AD526)]
SCALE = windows[(0, 9, 0x803AD544)]
SYMBOL = {}
for (mod, s, add), w in windows.items():
    SYMBOL[w] = f"88:{s}+{add:#x}" if mod == 88 else add
for off in (0xC77C, 0xCCF8):
    SYMBOL[CODE + off] = f"88:1+{off:#x}"
TASK, WORK, ATAB, FX, TEX, BC, OBJ, P = (0x86000 + 0x400 * i for i in range(8))


def run():
    global events
    events = []
    for base in (ENT, PL, ATAB, MOTIONS, MODELS, TASK, WORK, FX, TEX, BC, OBJ, P, MODE, RESTORE_MODE, SCALE):
        cpu.mem_write(base, bytes(0x400))
    for i in range(0x150):
        cpu.mem_write(MOTIONS + 8 * i + 4, struct.pack(">I", 0xA0000000 + i))
        cpu.mem_write(MODELS + 8 * i + 4, struct.pack(">I", 0xB0000000 + i))
    cpu.mem_write(TASK + 0x34, struct.pack(">I", ENT))
    cpu.mem_write(TASK + 0x40, struct.pack(">I", PL))
    cpu.mem_write(WORKPTR, struct.pack(">I", WORK))
    cpu.mem_write(WORK, struct.pack(">b", inp["active_player"]))
    cpu.mem_write(GLOBAL + 0x42, struct.pack(">b", inp["global_42"]))
    cpu.mem_write(MODE + inp["slot"], struct.pack(">b", inp["effect_mode"]))
    cpu.mem_write(RESTORE_MODE, struct.pack(">b", inp["restore_mode"]))
    cpu.mem_write(SCALE, struct.pack(">f", inp["ring_scale"]))
    cpu.mem_write(ENT, struct.pack(">b", inp["action"]))
    cpu.mem_write(ENT + 8, struct.pack(">3i", *inp["rotation"]))
    cpu.mem_write(PL, struct.pack(">bb", inp["slot"], inp["character"]))
    cpu.mem_write(PL + 0xBC, struct.pack(">I", BC if inp["word_bc"] else 0))
    cpu.mem_write(BC, struct.pack(">I", inp["bc_model"]))
    cpu.mem_write(PL + 0x174, struct.pack(">h", inp["mode_174"]))
    cpu.mem_write(PL + 0x178, struct.pack(">h", pose))
    cpu.mem_write(PL + 0x184, struct.pack(">f", inp["f184"]))
    cpu.mem_write(PL + 0x198, struct.pack(">II", ATAB, inp["word_19c"]))
    cpu.mem_write(PL + 0x370, struct.pack(">I", inp["word_370"]))
    cpu.mem_write(ATAB, struct.pack(">hh", 7, idle))
    cpu.mem_write(ATAB + 16 * pose, struct.pack(">hh", base_pose, pose_slot))
    cpu.mem_write(ATAB + 16 * base_pose, struct.pack(">h", model_slot))
    swap = inp["swap"]
    cpu.mem_write(PL + 0x36C, struct.pack(">I", FX if swap else 0))
    if swap:
        cpu.mem_write(FX + 0x18, struct.pack(">I", TEX))
        cpu.mem_write(TEX + 4, struct.pack(">I", swap["texture"]))
        for off in (0xAEC, 0xBC4):
            cpu.mem_write(MOTIONS + off, struct.pack(">I", OBJ))
        cpu.mem_write(OBJ + 0x2C, struct.pack(">I", P))
        cpu.mem_write(P + 4, struct.pack(">I", swap["saved"]))
    cpu.reg_write(R.UC_PPC_REG_3, TASK)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=10000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases = 0
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(800):
        slot = rng.choice((0, 1))
        pose = rng.randrange(1, 0x40)
        base_pose = rng.choice([x for x in range(1, 0x40) if x != pose])
        idle, pose_slot, model_slot = (rng.randrange(0, 0x150) for _ in range(3))
        inp = dict(path="super_draw", entity=ENT,
                   active_player=rng.choice((slot, slot, 1 - slot)),
                   global_42=rng.choice((0, 0, 1)), slot=slot,
                   character=rng.choice((9, 10, 0)), action=rng.choice((54, 59, 0)),
                   effect_mode=rng.choice((1, 5, 0, 2)), restore_mode=rng.choice((0, 3, -1)),
                   rotation=[rng.randrange(-0x10000, 0x10000) for _ in range(3)],
                   word_370=rng.randrange(1 << 32), word_bc=rng.choice((0, 1)),
                   mode_174=rng.choice((2, 0, 1)), word_19c=rng.randrange(1 << 32),
                   f184=f32(rng.uniform(-5, 5)),
                   swap=rng.choice((None, dict(target=P, texture=rng.randrange(1 << 32),
                                               saved=rng.randrange(1 << 32)))),
                   idle_motion=0xA0000000 + idle, pose_motion=0xA0000000 + pose_slot,
                   pose_model=0xB0000000 + model_slot, bc_model=rng.randrange(1 << 32),
                   ring_scale=f32(rng.uniform(0, 3)))
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
                if r["store"] == "module":
                    assert SYMBOL.get(e["address"]) == r["offset"] and e["value"] == PL, (n, r, e)
                else:
                    assert (e["address"], e["size"], e["value"]) == (r["offset"], r["size"], r["value"]), (n, r, e)
                continue
            assert r["call"] == e["call"], (n, r, e)
            args = [SYMBOL.get(v, v & 0xFFFFFFFF) for v in e["args"][:len(r["args"])]]
            assert r["args"] == args, (n, r, args)
            if "floats" in r:
                assert r["floats"] == e["floats"], (n, r, e)
        cases += 1
print(f"{cases} installed PPC super-form draws matched")