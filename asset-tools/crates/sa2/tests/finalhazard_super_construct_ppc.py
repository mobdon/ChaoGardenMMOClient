"""Compare the Rust Finalhazard super-form constructors with installed PPC.

Usage: python finalhazard_super_construct_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xa87c (character 9) and + 0xab04
(character 10) to their return. Engine imports are observed and given the
request's results; every call and every store is compared in order, and the
engine table words the code reads are compared with the reported reads.
"""
import itertools
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_WRITE, UC_HOOK_MEM_READ
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: (word(word(16) + 8 * n) & ~1, word(word(16) + 8 * n + 4))
code = section(1)[0]
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
SAVE, RESTORE = 0x8010AC80, 0x8010ACCC
REGIONS = dict(task=0x80000, player=0x81000, entity=0x82000, collision=0x83000,
               inner=0x84000, fx=0x85000, fx_owner=0x86000, timer_task=0x87000)


def relocations(start, end):
    out = {}
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
            elif t != 201 and sec == 1 and start <= pos < end:
                out[pos] = (t, mod, s, add)
    return out


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
    s = symbol(addr)
    assert isinstance(s, str) and s.startswith("88:"), hex(addr)
    return "module", s


def code_hook(u, pc, size, _):
    if pc == RETURN:
        u.emu_stop()
        return
    off = pc - CODE
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6)]
    if off in imports:
        target = imports[off]
        if target not in (SAVE, RESTORE):
            events.append(dict(site=off, call=target, args=regs))
            if target in results:
                u.reg_write(R.UC_PPC_REG_3, results[target].pop(0))
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
                       size=size, value=symbol(value & ((1 << 8 * size) - 1))))


def read_hook(u, access, addr, size, value, _):
    if table <= addr < table + 0x1000:
        reads.append(0x801E6288 + addr - table)


cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for start, character in ((0xA87C, 9), (0xAB04, 10)):
        end = start + 0x288
        relocs = relocations(start, end)
        cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
        cpu.mem_map(0, 0x200000)
        cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
        cpu.mem_write(CODE + start, data[code + start:code + end])
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
        table = windows[(0, 7, 0x801E6288)]
        offsets = [struct.unpack_from(">I", data, code + start + d)[0] & 0xFFFF
                   for d in (0x124, 0x134, 0x13C, 0x14C)]
        cpu.hook_add(UC_HOOK_CODE, code_hook)
        cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
        cpu.hook_add(UC_HOOK_MEM_READ, read_hook)
        for n, (slot, alloc, glob, timer_word) in enumerate(itertools.product(
                (0, 1), ("task", "player", "built"), (0, 1, -1), (0, 0x9000))):
            events, reads = [], []
            task = 0 if alloc == "task" else REGIONS["task"]
            player = 0 if alloc == "player" else REGIONS["player"]
            vec = [0x11110000 + 0x100 * n + i for i in range(3)]
            fx_source, f130 = 0x5150 + n, 1.5 + n
            position = [0.25 * n, -3.0, 7.5 + n]
            res = [0x7000 + n, 0x7100 + n, 0x7200 + n]
            results = {0x80071BA4: [task, REGIONS["timer_task"]], 0x801194B4: [player],
                       0x800722E0: [res[0]], 0x8004FFA8: [res[1]], 0x8004FE2C: [res[2]],
                       0x8002A958: [REGIONS["fx"]]}
            cpu.mem_write(0x80000, bytes(0x8000))
            cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
            cpu.mem_write(REGIONS["entity"] + 0x14, struct.pack(">3f", *position))
            cpu.mem_write(REGIONS["entity"] + 0x2C, struct.pack(">I", REGIONS["collision"]))
            cpu.mem_write(REGIONS["collision"] + 0xC, struct.pack(">I", REGIONS["inner"]))
            cpu.mem_write(REGIONS["player"] + 0x130, struct.pack(">f", f130))
            cpu.mem_write(REGIONS["fx_owner"] + 0x2C, struct.pack(">I", fx_source))
            for o, v in zip(offsets, vec):
                cpu.mem_write(table + o, struct.pack(">I", v))
            cpu.mem_write(table + offsets[3], struct.pack(">I", REGIONS["fx_owner"]))
            cpu.mem_write(windows[(0, 6, 0x801CBD88)], struct.pack(">b", glob))
            cpu.mem_write(windows[(88, 6, 0x4AC)], struct.pack(">I", timer_word))
            cpu.reg_write(R.UC_PPC_REG_1, STACK)
            cpu.reg_write(R.UC_PPC_REG_3, slot)
            cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
            cpu.emu_start(CODE + start, RETURN + 4, count=10000)
            inp = dict(path="super_construct", slot=slot, character=character, task=task,
                       player=player, entity=REGIONS["entity"], inner=REGIONS["inner"],
                       f130=f130, results=res, table_vector=vec, fx_source=fx_source,
                       fx=REGIONS["fx"], position=position, global_byte=glob,
                       module_4ac=timer_word, timer_task=REGIONS["timer_task"])
            q, out = p / "request.json", p / f"{character}-{n}.json"
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
            assert report["built"] == (alloc == "built"), (n, report)
            if alloc == "built":
                assert report["table_reads"] == reads, (n, report["table_reads"], reads)
            cases += 1
print(f"{cases} installed PPC super-form constructions matched")
