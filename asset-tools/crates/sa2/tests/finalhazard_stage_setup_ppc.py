"""Compare the Rust Finalhazard stage setup with installed PPC instructions.

Usage: python finalhazard_stage_setup_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xa078 to its return. Engine imports and
local calls are observed; every call (module and code addresses as
"88:SECTION+OFFSET", engine data addresses by their engine address) and
every store is compared in order.
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
START, END = 0xA078, 0xA2A0
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000

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
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, address=symbol(addr),
                       size=size, value=symbol(value)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
HOOK = windows[(0, 6, 0x801A2638)]
PLAYERS = windows[(0, 7, 0x801E7388)]
FLAG = windows[(88, 6, 0x484)]
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, (hook, t0, t1, flag) in enumerate(itertools.product(
            (0, 0x80123456), (0, 0x9000), (0, 0x9400), (0, 1, 0xFFFFFFFF))):
        events = []
        cpu.mem_write(HOOK, struct.pack(">I", hook))
        cpu.mem_write(PLAYERS, struct.pack(">II", t0, t1))
        cpu.mem_write(FLAG, struct.pack(">I", flag))
        cpu.reg_write(R.UC_PPC_REG_1, STACK)
        cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
        cpu.emu_start(CODE + START, RETURN + 4, count=10000)
        inp = dict(path="stage_setup", hit_handler=hook, player_tasks=[t0, t1], module_484=flag)
        q, out = p / "request.json", p / f"{n}.json"
        q.write_text(json.dumps(inp))
        subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                       stdout=subprocess.DEVNULL)
        got = json.loads(out.read_text())["events"]
        assert [e["site"] for e in got] == [e["site"] for e in events], (n, got, events)
        for r, e in zip(got, events):
            if "store" in r:
                assert (r["address"], r["size"], r["value"]) == (e["address"], e["size"], e["value"]), (n, r, e)
                continue
            assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
            assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
        cases += 1
print(f"{cases} installed PPC stage setups matched")
