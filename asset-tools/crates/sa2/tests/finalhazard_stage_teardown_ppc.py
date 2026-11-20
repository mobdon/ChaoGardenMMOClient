"""Compare the Rust Finalhazard stage teardown and stage object set-up with PPC.

Usage: python finalhazard_stage_teardown_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xa2a0 and + 0xa390 to their return.
Engine imports and local calls are observed; every call and every store is
compared in order.
"""
import itertools
import json
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
START, END = 0xA2A0, 0xA3F4
CODE, RETURN, STACK, OBJECT = 0x100000, 0x0FFFF0, 0x0F0000, 0x80000

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
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    if off in imports:
        events.append(dict(site=off, call=imports[off], args=regs))
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        events.append(dict(site=off, local=off + li, args=regs))
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x100 <= addr < STACK + 0x100:
        return
    site = u.reg_read(R.UC_PPC_REG_PC) - CODE
    if OBJECT <= addr < OBJECT + 0x100:
        events.append(dict(site=site, store="object", offset=addr - OBJECT, size=size,
                           value=symbol(value)))
    else:
        events.append(dict(site=site, address=symbol(addr), size=size, value=symbol(value)))


def run(start, inp, n):
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + start, RETURN + 4, count=10000)
    q, out = p / "request.json", p / f"{inp['path']}-{n}.json"
    q.write_text(json.dumps(inp))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    got = json.loads(out.read_text())["events"]
    assert [e["site"] for e in got] == [e["site"] for e in events], (n, got, events)
    for r, e in zip(got, events):
        if "store" in r:
            keys = ("offset", "size", "value") if r["store"] == "object" else ("address", "size", "value")
            assert r["store"] == e.get("store", "engine"), (n, r, e)
            assert [r[k] for k in keys] == [e[k] for k in keys], (n, r, e)
            continue
        assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
        assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
HOOK = windows[(0, 6, 0x801A2638)]
PLAYERS = windows[(0, 7, 0x801E7388)]
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    handlers = (0, 0x80123456, CODE + 0x19470, CODE + 0x19474)
    for n, (w484, hook, w480, t0, t1) in enumerate(itertools.product(
            (0, 0x9100), handlers, (0, 0x9200), (0, 0x9000), (0, 0x9400))):
        events = []
        cpu.mem_write(windows[(88, 6, 0x484)], struct.pack(">I", w484))
        cpu.mem_write(windows[(88, 6, 0x480)], struct.pack(">I", w480))
        cpu.mem_write(HOOK, struct.pack(">I", hook))
        cpu.mem_write(PLAYERS, struct.pack(">II", t0, t1))
        run(0xA2A0, dict(path="stage_teardown", module_484=w484, hit_handler=symbol(hook),
                         module_480=w480, player_tasks=[t0, t1]), n)
        cases += 1
    for n, flag in enumerate((0, 1, -1, 0x7FFFFFFF)):
        events = []
        cpu.mem_write(OBJECT, bytes(0x100))
        cpu.mem_write(windows[(88, 5, 0x382B8)], struct.pack(">i", flag))
        cpu.reg_write(R.UC_PPC_REG_3, OBJECT)
        run(0xA390, dict(path="stage_object", module_382b8=flag), n)
        cases += 1
print(f"{cases} installed PPC stage teardowns and object set-ups matched")
