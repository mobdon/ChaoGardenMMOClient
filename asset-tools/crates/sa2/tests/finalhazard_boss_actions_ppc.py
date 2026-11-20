"""Compare the Rust Finalhazard boss actions 2 and 3 with installed PPC.

Usage: python finalhazard_boss_actions_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xff4 (action 2) and + 0x10ac (action 3)
to their return. Engine imports and local calls are observed; memory the
code reads again after a call is changed at that read. Every call (with
float arguments) and every store is compared in order.
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
START, END = 0xFF4, 0x12E8
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, data=0x82000, state=0x83000, motion=0x84000,
               stage=0x85000)

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
    return v & 0xFFFFFFFF


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
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs, f1=f1))
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs, f1=f1))
    if target in results:
        u.reg_write(R.UC_PPC_REG_3, results[target] & 0xFFFFFFFF)
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x100 <= addr < STACK + 0x100:
        return
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=name,
                               offset=addr - base, size=size, value=value & ((1 << 8 * size) - 1)))
            return
    raise AssertionError(hex(addr))


def layout(step, child, handles=(0, 0)):
    cpu.mem_write(0x80000, bytes(0x6000))
    cpu.mem_write(REGIONS["task"] + 0x34, struct.pack(">I", REGIONS["entity"]))
    cpu.mem_write(REGIONS["task"] + 0x40, struct.pack(">I", REGIONS["data"]))
    cpu.mem_write(REGIONS["data"] + 0xC, struct.pack(">I", REGIONS["motion"]))
    cpu.mem_write(REGIONS["data"] + 0x10, struct.pack(">I", REGIONS["state"]))
    cpu.mem_write(REGIONS["state"] + 0x10, struct.pack(">III", child, *handles))
    cpu.mem_write(REGIONS["entity"] + 1, struct.pack(">b", step))
    cpu.mem_write(windows[(88, 6, 0x4A8)], struct.pack(">I", REGIONS["stage"]))


def compare(inp, n):
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_3, REGIONS["task"])
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + (0xFF4 if inp["path"] == "boss_action_two" else 0x10AC), RETURN + 4,
                  count=10000)
    q, out = p / "request.json", p / f"{n}.json"
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


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
k4 = lambda o: struct.unpack_from(">f", data, section(4)[0] + o)[0]
low, high = k4(0xC4), k4(0xF4)
frames = (low - 1, low, low + 0.25, (low + high) / 2, high, high + 2)
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n, (step, timer, pattern, sid) in enumerate(itertools.product(
            (0, 1, 2, -1), (0, 1, 2, 0x3C, 0xFFFF), (5, -1, 13), (0, 0x9C))):
        events = []
        child = 0x7000 + n
        layout(step, child)
        cpu.mem_write(REGIONS["stage"], struct.pack(">B", sid))
        after = (n * 29) & 0xFF
        changes = {0x1058: [(REGIONS["entity"] + 1, ">B", after)],
                   0x1064: [(REGIONS["entity"] + 6, ">H", timer)]}
        results = {0xD40: pattern}
        compare(dict(path="boss_action_two", task=REGIONS["task"], step=step, step_after=after,
                     child=child, stage_id=sid, timer=timer, pattern=pattern), n)
        cases += 1
    for n, (step, h0, h1, mflags, frame, done, eflags) in enumerate(itertools.product(
            (0, 1, 2, 3, -1), (0, 0x9100), (0, 0x9200), (0, 2, 0xFFFD), frames, (0, 1),
            (0x7FFF, -1, 4))):
        events = []
        child = 0x7100 + n
        layout(step, child, (h0, h1))
        after = (n * 31) & 0xFF
        tasks, objs = [0x6100 + n, 0x6200 + n], [0x6300 + n, 0x6400 + n]
        cpu.mem_write(windows[(0, 7, 0x801E7348)], struct.pack(">II", *tasks))
        cpu.mem_write(windows[(0, 7, 0x801E7328)], struct.pack(">II", *objs))
        cpu.mem_write(REGIONS["motion"], struct.pack(">H", mflags))
        cpu.mem_write(REGIONS["motion"] + 0x1C, struct.pack(">f", frame))
        changes = {0x115C: [(REGIONS["entity"] + 1, ">B", after)],
                   0x11F0: [(REGIONS["entity"] + 1, ">B", after)],
                   0x1264: [(REGIONS["entity"] + 4, ">h", eflags)]}
        results = {0x8002CB2C: done}
        compare(dict(path="boss_action_three", task=REGIONS["task"], motion=REGIONS["motion"],
                     step=step, step_after=after, handles=[h0, h1], child=child,
                     motion_flags=mflags, player_tasks=tasks, player_objects=objs,
                     motion_frame=frame, motion_done=done, entity_flags=eflags), 10000 + n)
        cases += 1
print(f"{cases} installed PPC boss action 2 and 3 frames matched")
