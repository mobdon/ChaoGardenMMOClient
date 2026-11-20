"""Compare the Rust Finalhazard boss constructor with installed PPC instructions.

Usage: python finalhazard_boss_construct_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x414 to its return. Engine imports and
local calls are observed and given the request's results; every call, float
argument and store is compared in order, and so is the returned task.
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
START, END = 0x414, 0x638
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, work=0x82000, block=0x83000,
               collision=0x84000, stage=0x85000)

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


def place(addr):
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            return name, addr - base
    return "module", symbol(addr)


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    f1 = struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs, f1=f1))
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs))
        if target == 0x328:
            u.mem_write(REGIONS["entity"] + 0x14, struct.pack(">3f", *position))
    if target in results:
        u.reg_write(R.UC_PPC_REG_3, results[target])
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x100 <= addr < STACK + 0x100:
        return
    region, offset = place(addr)
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=region, offset=offset,
                       size=size, value=symbol(value & ((1 << 8 * size) - 1))))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, (fail, b50c, flags, sbyte) in enumerate(itertools.product(
            (None, "task", "block", "allocation"), (0, 5, -7, 127, -128),
            (0, 0x40, 0xFFFF, 0x1234), (0, 3, 0xFF))):
        events = []
        task = 0 if fail == "task" else REGIONS["task"]
        block = 0 if fail == "block" else REGIONS["block"]
        alloc = 0 if fail == "allocation" else 0x9A00 + n
        position = [1.25 * n, -2.0, 100.5 - n]
        w510, work0c, obj = 0x51000000 + n, 0x0C000000 + n, 0x8528000 + n
        results = {0x80071BA4: task, 0x1AFE8: REGIONS["work"], 0x1B018: block,
                   0x801194B4: alloc, 0x8528: obj}
        cpu.mem_write(0x80000, bytes(0x6000))
        cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
        cpu.mem_write(REGIONS["entity"] + 0x2C, struct.pack(">I", REGIONS["collision"]))
        cpu.mem_write(REGIONS["collision"] + 4, struct.pack(">H", flags))
        cpu.mem_write(REGIONS["work"] + 0xC, struct.pack(">I", work0c))
        cpu.mem_write(windows[(88, 5, 0x510)], struct.pack(">I", w510))
        cpu.mem_write(windows[(88, 5, 0x50C)], struct.pack(">b", b50c))
        cpu.mem_write(windows[(88, 6, 0x4A8)], struct.pack(">I", REGIONS["stage"]))
        cpu.mem_write(REGIONS["stage"], struct.pack(">B", sbyte))
        cpu.reg_write(R.UC_PPC_REG_1, STACK)
        cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
        cpu.emu_start(CODE + START, RETURN + 4, count=10000)
        returned = cpu.reg_read(R.UC_PPC_REG_3)
        inp = dict(path="boss_construct", task=task, entity=REGIONS["entity"],
                   work=REGIONS["work"], block=block, allocation=alloc, module_510=w510,
                   module_50c=b50c, collision=REGIONS["collision"], collision_flags=flags,
                   work_0c=work0c, position=position, stage_byte=sbyte, object=obj)
        q, out = p / "request.json", p / f"{n}.json"
        q.write_text(json.dumps(inp))
        subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                       stdout=subprocess.DEVNULL)
        report = json.loads(out.read_text())
        got = report["events"]
        assert [e["site"] for e in got] == [e["site"] for e in events], (n, got, events)
        for r, e in zip(got, events):
            if "store" in r:
                assert (r["store"], r["offset"], r["size"], r["value"]) == (
                    e["store"], e["offset"], e["size"], e["value"]), (n, r, e)
                continue
            assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
            assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
            if "floats" in r:
                assert r["floats"] == [e["f1"]], (n, r, e)
        assert report["returned"] == returned, (n, report["returned"], returned)
        cases += 1
print(f"{cases} installed PPC boss constructions matched")
