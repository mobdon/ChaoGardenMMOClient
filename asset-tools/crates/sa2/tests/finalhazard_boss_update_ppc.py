"""Compare the Rust Finalhazard boss update with installed PPC instructions.

Usage: python finalhazard_boss_update_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x1bb8 to its return. Engine imports and
local calls are observed. Memory the native code reads again after a call
is changed at that point, so each request value is checked at the read it
stands for. Every call and every store is compared in order.
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
START, END = 0x1BB8, 0x1E04
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
SAVE, RESTORE = 0x8010AC84, 0x8010ACD0
REGIONS = dict(task=0x80000, data=0x81000, state=0x82000, entity=0x83000, stage=0x84000,
               child=0x85000, child_entity=0x86000)

relocs, table_relocs = {}, {}
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
        elif t == 1 and sec == 5 and 0xFDC <= pos < 0xFDC + 56:
            table_relocs[pos] = add
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
TABLE = windows[(88, 5, 0xFDC)]
GLOBAL = windows[(0, 6, 0x801CBD88)]
STAGE_PTR = windows[(88, 6, 0x4A8)]


def symbol(v):
    if CODE <= v < CODE + 0x20000:
        return f"88:1+{v - CODE:#x}"
    return v & 0xFFFFFFFF


def place(addr):
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            return name, addr - base
    raise AssertionError(hex(addr))


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
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    if off in imports:
        if imports[off] not in (SAVE, RESTORE):
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
    region, offset = place(addr)
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=region, offset=offset,
                       size=size, value=value & ((1 << 8 * size) - 1)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(1)
pick = lambda *xs: rng.choice(xs)
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(600):
        events = []
        stage_id = pick(0, 3, 0x80, 0xFF)
        f = dict(path="boss_update", task=REGIONS["task"], data=REGIONS["data"],
                 state=REGIONS["state"], entity=REGIONS["entity"],
                 action=pick(0, 4, 13, -1), global_37=pick(0, 0, 0, 0, 0, 0, 1, -1),
                 global_39=pick(0, 0, 0, 0, 0, 1), stage_lock=pick(0, 0, 0, 1, -1),
                 stage_id=stage_id,
                 last_stage=pick(stage_id - 256 if stage_id > 127 else stage_id, 1, -1),
                 handles=[pick(0, 0x9100 + n), pick(0, 0x9200 + n)], data_0c=0x0C00 + n,
                 entry_stage_id=pick(7, 0xFE), state_0c=pick(1, 1, 0, 2, -1),
                 state_06=pick(1, 2, 0, 0xFFFF), dispatch_action=rng.randrange(-2, 16),
                 after=dict(data_0c=0x0D00 + n, child=pick(0, REGIONS["child"]),
                            child_entity=REGIONS["child_entity"], stage_id=pick(9, 0xF0),
                            entity_flags=pick(0, 0x100, -1, 0x0FF), entity_flags_after=pick(0x100, -2, 0x4321),
                            final_stage_id=pick(11, 0xEE)))
        a = f["after"]
        cpu.mem_write(0x80000, bytes(0x7000))
        cpu.mem_write(GLOBAL + 0x37, struct.pack(">bxb", f["global_37"], f["global_39"]))
        cpu.mem_write(STAGE_PTR, struct.pack(">I", REGIONS["stage"]))
        cpu.mem_write(REGIONS["stage"], struct.pack(">Bb", f["stage_id"], f["stage_lock"]))
        cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
        cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", REGIONS["data"]))
        cpu.mem_write(REGIONS["data"] + 0xC, struct.pack(">I", f["data_0c"]))
        cpu.mem_write(REGIONS["data"] + 0x10, struct.pack(">I", REGIONS["state"]))
        cpu.mem_write(REGIONS["entity"], struct.pack(">b", f["action"]))
        cpu.mem_write(REGIONS["state"] + 5, struct.pack(">b", f["last_stage"]))
        cpu.mem_write(REGIONS["state"] + 0x14, struct.pack(">II", *f["handles"]))
        cpu.mem_write(REGIONS["child"] + 0x34, struct.pack(">I", REGIONS["child_entity"]))
        changes = {
            0x1CA4: [(REGIONS["stage"], ">B", f["entry_stage_id"])],
            0x1CC4: [(REGIONS["state"] + 0xC, ">i", f["state_0c"]),
                     (REGIONS["state"] + 6, ">H", f["state_06"]),
                     (REGIONS["entity"], ">b", f["dispatch_action"])],
            0x1D74: [(REGIONS["data"] + 0xC, ">I", a["data_0c"]),
                     (REGIONS["state"] + 0x10, ">I", a["child"])],
            0x1D7C: [(REGIONS["stage"], ">B", a["stage_id"])],
            0x1DA8: [(REGIONS["entity"] + 4, ">h", a["entity_flags"])],
            0x1DBC: [(REGIONS["entity"] + 4, ">h", a["entity_flags_after"])],
            0x1DD8: [(REGIONS["stage"], ">B", a["final_stage_id"])],
        }
        cpu.mem_write(TABLE, b"".join(struct.pack(">I", CODE + table_relocs[0xFDC + 4 * i])
                                      for i in range(14)))
        cpu.reg_write(R.UC_PPC_REG_1, STACK)
        cpu.reg_write(R.UC_PPC_REG_3, REGIONS["task"])
        cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
        cpu.emu_start(CODE + START, RETURN + 4, count=10000)
        q, out = p / "request.json", p / f"{n}.json"
        q.write_text(json.dumps(f))
        subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                       stdout=subprocess.DEVNULL)
        got = json.loads(out.read_text())["events"]
        assert [e["site"] for e in got] == [e["site"] for e in events], (n, f, got, events)
        for r, e in zip(got, events):
            if "store" in r:
                assert (r["store"], r["offset"], r["size"], r["value"]) == (
                    e["store"], e["offset"], e["size"], e["value"]), (n, r, e)
                continue
            assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
            assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
        cases += 1
print(f"{cases} installed PPC boss updates matched")
