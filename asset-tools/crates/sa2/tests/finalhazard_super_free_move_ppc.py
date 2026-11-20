"""Compare the Rust action-58 free movement with installed PPC instructions.

Usage: python finalhazard_super_free_move_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xdc90 (with the local square root at
+0x40cc running natively) and compares every call, its arguments and
float arguments, and every non-stack store.
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gekko_float import install_frsqrte  # noqa: E402

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
START, END = 0xDC90, 0xDF44
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(CODE + START, data[code + START:code + END])
cpu.mem_write(CODE + 0x40CC, data[code + 0x40CC:code + 0x4158])
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not (START <= at < END or 0x40CC <= at < 0x4158):
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
REGIONS = [("entity", ENT), ("data2", D2), ("player", PL), ("inner", INNER)]
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
    if imports[off] == 0x80110E20:
        floats_in = [d2f(u.reg_read(getattr(R, f"UC_PPC_REG_FPR{n}"))) for n in (1, 2)]
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    assert ins >> 26 == 18 and ins & 1, hex(off)
    target = imports[off]
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    if target in SAVE:
        saved.append([u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(14, 32)])
    elif target in RESTORE:
        for n, v in zip(range(14, 32), saved.pop()):
            u.reg_write(getattr(R, f"UC_PPC_REG_{n}"), v)
    else:
        if target == 0x80110E20:
            u.reg_write(R.UC_PPC_REG_FPR1, f2d(inp["atan2"]))
        elif target in (0x80112068, 0x80112050):
            u.reg_write(R.UC_PPC_REG_FPR1, f2d(inp["trig"][0 if target == 0x80112068 else 1]))
        events.append(dict(site=off, call=target, args=regs,
                           floats=floats_in if target == 0x80110E20 else None,
                           vectors={str(i + 3): floats(u, regs[i], 3) for i in range(3)},
                           blocks={str(i + 3): floats(u, regs[i], 6) for i in range(2)}))
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


install_frsqrte(cpu, CODE + 0x40E4)  # Gekko estimate in the stage square root
cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)


PAD = windows[(0, 7, 0x801E5038)]
CAMPTR = windows[(0, 5, 0x80174F98)]
CAM = 0x86000


def run():
    global events
    events = []
    for base in (ENT, D2, PL, PAD, CAM):
        cpu.mem_write(base, bytes(0x1000))
    cpu.mem_write(CAMPTR, struct.pack(">I", CAM))
    cpu.mem_write(CAM + 0x10, struct.pack(">i", inp["camera_yaw"]))
    cpu.mem_write(PL, struct.pack(">b", slot))
    entry = PAD + 52 * slot
    cpu.mem_write(entry + 8, struct.pack(">I", inp["buttons_08"]))
    cpu.mem_write(entry + 0x10, struct.pack(">I", inp["buttons_10"]))
    cpu.mem_write(entry + 0x1C, struct.pack(">2h", *inp["stick"]))
    cpu.mem_write(ENT + 0x14, struct.pack(">3f", *inp["position"]))
    cpu.reg_write(R.UC_PPC_REG_3, ENT)
    cpu.reg_write(R.UC_PPC_REG_5, PL)
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN + 4, count=10000)
    return events


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
cases = 0
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(1500):
        slot = rng.choice((0, 1))
        stick = [rng.choice((0, 14, 15, -14, -15, 100, -128, 127, rng.randrange(-128, 128)))
                 for _ in range(2)]
        inp = dict(path="super_free_move",
                   buttons_08=rng.choice((0, 0x400, 0xFFFFFFFF, 0x3FF)),
                   buttons_10=rng.choice((0, 2, 0xFFFFFFFD)),
                   stick=stick,
                   position=[f32(rng.uniform(-1500, 1500)) for _ in range(3)],
                   atan2=rng.uniform(-3.2, 3.2), camera_yaw=rng.randrange(-0x10000, 0x10000),
                   trig=[f32(rng.uniform(-1, 1)) for _ in range(2)])
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
            assert r["call"] == e["call"]
            if "args" in r:
                assert r["args"] == [v & 0xFFFFFFFF for v in e["args"][:len(r["args"])]], (n, r, e)
            if "floats" in r:
                assert r["floats"] == e["floats"], (n, r, e)
        cases += 1
print(f"{cases} installed PPC action-58 free-movement updates matched")