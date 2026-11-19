"""Compare the Rust handoff replay with installed PPC instructions, frame by frame.

Usage: python finalhazard_handoff_replay_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Each frame runs Boss_Last2D.rel section 1 + 0xf458 through the shared tail's
timer store (0xff34). Engine imports and local calls are observed and given
controlled results; every non-stack store is recorded with its instruction.
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
FIRST, STOP = 0xf458, 0xff38

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

cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x100000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(0x10000 + FIRST, data[code + FIRST:code + STOP])
CODE = 0x10000  # section 1 offset X lives at CODE + X

# Every data reference in the range gets its own 0x1000 window.
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not FIRST <= at < STOP:
        continue
    if t == 10:
        imports[at] = add
        continue
    assert t in (4, 6), (hex(at), t)
    key = (mod, s, add)
    if key not in windows:
        windows[key] = 0x40000 + 0x1000 * len(windows)
        if mod == 88:
            start, size = section(s)
            if start:
                chunk = data[start + add:start + min(size, add + 0x1000)]
                cpu.mem_write(windows[key], chunk)
    dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))

GLOBAL = windows[(0, 6, 0x801CBD88)]
TABLE1, TABLE2, TABLE3 = (windows[(0, 7, a)] for a in (0x801E7368, 0x801E7328, 0x801E7348))
RESOURCES = windows[(0, 7, 0x801E5928)]
P1, P2, TASKS = 0x80000, 0x82000, 0x84000
TASK, WORK, STACK = 0x86000, 0x87000, 0x90000
player1 = [P1 + 0x800 * s for s in range(2)]
player2 = [P2 + 0x800 * s for s in range(2)]
player_tasks = [TASKS + 0x800 * s for s in range(2)]
for s in range(2):
    cpu.mem_write(TABLE1 + 4 * s, struct.pack(">I", player1[s]))
    cpu.mem_write(TABLE2 + 4 * s, struct.pack(">I", player2[s]))
    cpu.mem_write(TABLE3 + 4 * s, struct.pack(">I", player_tasks[s]))
offsets = (0x354, 0x364, 0x37C, 0x39C)
table = [0x11110000 + o for o in offsets]
for o, v in zip(offsets, table):
    cpu.mem_write(RESOURCES + o, struct.pack(">I", v))
POLL = {0xF52C, 0xF6C0}
MODE = {0xF680, 0xF690, 0xF7D4, 0xF7E4}
SUBMIT = {0xF6A8, 0xF7FC}


def region(addr):
    for name, base, slots in (("player1", P1, 2), ("player2", P2, 2)):
        if base <= addr < base + 0x800 * slots:
            return name, (addr - base) // 0x800, (addr - base) % 0x800
    if GLOBAL <= addr < GLOBAL + 0x1000:
        return "global", None, addr - GLOBAL
    if WORK <= addr < WORK + 0x100:
        return "task", None, addr - WORK
    raise AssertionError(f"unexpected store at {addr:#x}")


def code_hook(u, pc, size, _):
    off = pc - CODE
    if off == STOP:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    r3, r4, r5, r6 = (u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in (3, 4, 5, 6))
    if off in imports:
        ev = dict(site=off, call=imports[off], args=[r3, r4, r5, r6])
        if off in SUBMIT:
            ev["packet"] = list(struct.unpack(">6I", u.mem_read(r4, 24)))
        events.append(ev)
        ret = 0
        if off in POLL:
            ret = frame["poll"]
        elif off in MODE:
            ret = frame["camera_mode"] if off in (0xF680, 0xF7D4) else frame["second_lookup"]
        u.reg_write(R.UC_PPC_REG_3, ret & 0xFFFFFFFF)
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target))
        u.reg_write(R.UC_PPC_REG_3, frame["cue_result"] & 0xFFFFFFFF if target == 0xEB38 else 0)
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x1000 <= addr < STACK + 0x1000:
        return
    name, slot, offset = region(addr)
    events.append(dict(site=u.reg_read(R.UC_PPC_REG_PC) - CODE, store=name, slot=slot,
                       offset=offset, size=size, value=value & ((1 << (8 * size)) - 1)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)


def run(start):
    """Emulate one replay and return the request plus native per-frame events."""
    player, state, timer, countdown, visits, frames_in = start
    cpu.mem_write(WORK, bytes(0x100))
    cpu.mem_write(WORK, struct.pack(">bbbb", player, state, *visits))
    cpu.mem_write(WORK + 8, struct.pack(">h", timer))
    cpu.mem_write(WORK + 0xE, struct.pack(">h", countdown))
    native = []
    global frame, events
    for frame in frames_in:
        events = []
        cpu.mem_write(GLOBAL + 0x7C, struct.pack(">I", frame["global_7c"]))
        for s in range(2):
            cpu.mem_write(player1[s] + 0xC, struct.pack(">i", frame["yaws"][s]))
            cpu.mem_write(player2[s] + 1, struct.pack(">b", frame["characters"][s]))
        cpu.reg_write(R.UC_PPC_REG_1, STACK)
        cpu.reg_write(R.UC_PPC_REG_29, TASK)
        cpu.reg_write(R.UC_PPC_REG_30, WORK)
        cpu.emu_start(CODE + FIRST, CODE + STOP + 4, count=1000)
        p, st = struct.unpack(">bb", cpu.mem_read(WORK, 2))
        t, = struct.unpack(">h", cpu.mem_read(WORK + 8, 2))
        c, = struct.unpack(">h", cpu.mem_read(WORK + 0xE, 2))
        native.append((events, dict(player=p, switch_state=st, timer=t, countdown=c)))
        if st == 0:
            break
    return native


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
replays = frames_total = 0
seen = set()
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for case in range(160):
        player = case & 1
        state = (1, 2, 3)[case // 2 % 3]
        timer = rng.choice((0, 100, 119, 120, 121, 179, 180, 181, -3, 32767))
        countdown = rng.choice((0, 1, 5, -2))
        visits = [rng.choice((-1, 0, 1, 2, 3)) for _ in range(2)]
        poll_rate = rng.choice((0.0, 0.1, 0.5, 1.0))
        frames_in = []
        for _ in range(rng.choice((1, 3, 40, 400))):
            mode = rng.choice((-1, 13, 14, 20))
            frames_in.append(dict(
                poll=rng.choice((1, -1, 7)) if rng.random() < poll_rate else 0,
                characters=[rng.choice((9, 10, 0, -1)) for _ in range(2)],
                yaws=[rng.choice((0, 0x4000, 0x8000, 0x9000, -1, 0x7FFFFFFF, -0x80000000))
                      for _ in range(2)],
                camera_mode=mode, second_lookup=0x2A + mode,
                cue_result=rng.choice((-1, 0, 5)),
                global_7c=rng.choice((0, 14399, 14400, 14400, 14401))))
        native = run((player, state, timer, countdown, visits, frames_in))
        frames_in = frames_in[:len(native)]
        inp = dict(path="switch_replay", player=player, switch_state=state, timer=timer,
                   countdown=countdown, visits=visits, player1=player1, player2=player2,
                   player_tasks=player_tasks, table_words=table, frames=frames_in)
        q = p / "request.json"
        out = p / f"{case}.json"
        q.write_text(json.dumps(inp))
        subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                       stdout=subprocess.DEVNULL)
        got = json.loads(out.read_text())
        assert len(got["frames"]) == len(native), (case, len(got["frames"]), len(native))
        for n, (rust, (events, after)) in enumerate(zip(got["frames"], native)):
            assert {k: rust[k] for k in after} == after, (case, n, rust, after)
            rev = rust["events"]
            assert len(rev) == len(events), (case, n, rev, events)
            for r, e in zip(rev, events):
                if "call" in r:
                    e = dict(e, args=e["args"][:len(r["args"])])
                    r = {k: v for k, v in r.items() if k != "resource_table_offset"}
                assert r == e, (case, n, r, e)
        assert got["returned_to_normal_dispatch"] == (native[-1][1]["switch_state"] == 0)
        replays += 1
        frames_total += len(native)
        seen.update(e["site"] for events, _ in native for e in events)
# Every handoff branch must have been exercised at least once.
required = {0xF47C, 0xF498, 0xF4C4, 0xF4D8, 0xF52C, 0xF55C, 0xF564, 0xF5C8, 0xF690, 0xF69C,
            0xF6A8, 0xF6C0, 0xF718, 0xF7E4, 0xF7F0, 0xF7FC, 0xF800, 0xF838, 0xF848, 0xF86C,
            0xF87C, 0xF884, 0xF8BC, 0xF8C4, 0xFF34}
assert required <= seen, sorted(hex(s) for s in required - seen)
print(f"{replays} installed PPC handoff replays ({frames_total} frames) matched")
