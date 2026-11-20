"""Compare the Rust handoff replay with installed PPC instructions, frame by frame.

Usage: python finalhazard_handoff_replay_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Each frame runs Boss_Last2D.rel section 1 + 0xf458 through the shared tail's
timer store (0xff34). Engine imports and local calls are observed and given
controlled results; every non-stack store is recorded with its instruction.
"""
import json
import math
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

cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x100000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
CODE = 0x10000  # section 1 offset X lives at CODE + X
# The handoff frame plus the stage-local calls it can run natively: square
# root, voice gate, arena bounds and timed voice lines.
LOCALS = [(0x40CC, 0x4158), (0xEB38, 0xEE8C), (0xEE8C, 0xF3F8)]
RANGES = [(FIRST, STOP)] + LOCALS
for a, b in RANGES:
    cpu.mem_write(CODE + a, data[code + a:code + b])

# Every data reference in the ranges gets its own 0x1000 window.
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not any(a <= at < b for a, b in RANGES):
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

assert 0x40000 + 0x1000 * len(windows) <= 0x80000, len(windows)
JUMPS = windows[(88, 5, 0x40008)]
for pos in range(0x40008, 0x40030, 4):
    t, mod, s, add = data_relocs[(5, pos)]
    assert (t, mod, s) == (1, 88, 1), (hex(pos), t, mod, s)
    cpu.mem_write(JUMPS + pos - 0x40008, struct.pack(">I", CODE + add))
VOICE = windows[(0, 9, 0x803AD3D0)]
PRESENT = windows[(0, 7, 0x801E7388)]
CHANNELS, BOSS = 0x8A000, 0x8B000
HOLD = windows[(88, 6, 0x664)]
rings = [0, 0]
counters = {}
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
    for name, base, slots in (("player1", P1, 2), ("player2", P2, 2), ("player_task", TASKS, 2)):
        if base <= addr < base + 0x800 * slots:
            return name, (addr - base) // 0x800, (addr - base) % 0x800
    if GLOBAL <= addr < GLOBAL + 0x1000:
        return "global", None, addr - GLOBAL
    if WORK <= addr < WORK + 0x100:
        return "task", None, addr - WORK
    raise AssertionError(f"unexpected store at {addr:#x}")




SAVE = {0x8010AC70, 0x8010AC80}  # register-save helpers in the local prologues
RESTORE = {0x8010ACBC, 0x8010ACCC}
saved = []
in_local = lambda off: any(a <= off < b for a, b in LOCALS)


def code_hook(u, pc, size, _):
    off = pc - CODE
    if off == STOP:
        u.emu_stop()
        return
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = lambda *ns: [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in ns]
    r3, r4, r5, r6 = regs(3, 4, 5, 6)
    stage = frame.get("stage")
    if off in imports:
        target = imports[off]
        ret = 0
        if target in SAVE:
            saved.append(regs(*range(14, 32)))
        elif target in RESTORE:
            for n, v in zip(range(14, 32), saved.pop()):
                u.reg_write(getattr(R, f"UC_PPC_REG_{n}"), v)
        elif target == 0x8011B314:  # engine distance(a, b)
            vec = lambda p: list(struct.unpack(">3f", u.mem_read(p, 12)))
            events.append(dict(site=off, call=target, vectors=[vec(r3), vec(r4)]))
            slot = u.reg_read(R.UC_PPC_REG_28)
            bits = struct.unpack(">Q", struct.pack(">d", stage["distances"][slot]))[0]
            u.reg_write(R.UC_PPC_REG_FPR1, bits)
        elif target in (0x80112050, 0x80112068):  # engine trigonometry
            events.append(dict(site=off, call=target, args=[r3]))
            value = frame["debris"]["trig"][counters["trig"]]
            counters["trig"] += 1
            u.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", value))[0])
            u.reg_write(R.UC_PPC_REG_PC, pc + 4)
            return
        elif target == 0x80114BA0:  # engine vector helper (no effect modeled)
            vec = lambda p: list(struct.unpack(">3f", u.mem_read(p, 12)))
            events.append(dict(site=off, call=target, vectors=[vec(r3), vec(r4)]))
        elif target == 0x80039754:  # engine set position(slot, x, y, z)
            fpr = lambda n: struct.unpack(">d", struct.pack(
                ">Q", u.reg_read(getattr(R, f"UC_PPC_REG_FPR{n}"))))[0]
            events.append(dict(site=off, call=target, args=[r3], floats=[fpr(n) for n in (1, 2, 3)]))
        else:
            ev = dict(site=off, call=target, args=[r3, r4, r5, r6])
            if off in SUBMIT:
                ev["packet"] = list(struct.unpack(">6I", u.mem_read(r4, 24)))
            events.append(ev)
            if off in POLL:
                ret = frame["poll"]
            elif off in MODE:
                ret = frame["camera_mode"] if off in (0xF680, 0xF7D4) else frame["second_lookup"]
            elif off == 0xFDC8:  # camera mode read after a ring-out
                ret = frame["camera_mode"]
            elif target == 0x80062A24:  # ring count (modeled counter)
                ret = rings[r3]
            elif target == 0x80063584:  # add rings (modeled as exact addition)
                rings[r3] = (rings[r3] + struct.unpack(">i", struct.pack(">I", r4))[0])
            elif target == 0x80022BC4:
                ret = stage["sound_queries"][r3]
            elif target == 0x8010D940:  # engine random
                ret = frame["debris"]["randoms"][counters["rand"]]
                counters["rand"] += 1
        if target not in SAVE | RESTORE and target not in (0x8011B314,):
            u.reg_write(R.UC_PPC_REG_3, ret & 0xFFFFFFFF)
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
    li = ins & 0x3FFFFFC
    if li & 0x2000000:
        li -= 0x4000000
    target = off + li
    if target == 0x40CC:  # stage square root: always run natively
        return
    if target == 0x9F4:  # local boss lookup: answer with the supplied flags
        events.append(dict(site=off, local=target))
        flags = stage["boss_flags"]
        if flags is not None:
            u.mem_write(BOSS + 0x34, struct.pack(">I", BOSS + 0x100))
            u.mem_write(BOSS + 0x104, struct.pack(">h", flags))
        u.reg_write(R.UC_PPC_REG_3, 0 if flags is None else BOSS)
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
    if off == 0xFBA0:  # local debris spawn 0x1425c stays external
        vec = lambda p: list(struct.unpack(">3f", u.mem_read(p, 12)))
        f1 = struct.unpack(">d", struct.pack(">Q", u.reg_read(R.UC_PPC_REG_FPR1)))[0]
        events.append(dict(site=off, local=target, vectors=[vec(r3), vec(r4)], floats=[f1]))
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
    if off == 0xF3DC:  # voice-task helper 0x18ed0 stays external
        events.append(dict(site=off, local=target, args=regs(4, 5, 6, 7),
                           vector=list(struct.unpack(">3f", u.mem_read(r3, 12)))))
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)
        return
    events.append(dict(site=off, local=target))
    if stage is not None and target in (0xEB38, 0xEC04, 0xEE8C, 0xF070):
        return  # run the stage-local call natively
    u.reg_write(R.UC_PPC_REG_3, frame["cue_result"] & 0xFFFFFFFF if target == 0xEB38 else 0)
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x2000 <= addr < STACK + 0x1000:
        return
    site = u.reg_read(R.UC_PPC_REG_PC) - CODE
    if in_local(site):
        return  # local-call stores are checked through the final work state
    name, slot, offset = region(addr)
    events.append(dict(site=site, store=name, slot=slot,
                       offset=offset, size=size, value=value & ((1 << (8 * size)) - 1)))


cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
cpu.mem_write(windows[(88, 6, 0x4A8)], struct.pack(">I", WORK))


def player_now():
    return struct.unpack(">b", cpu.mem_read(WORK, 1))[0] & 1


def run(start):
    """Emulate one replay and return the native per-frame events and state."""
    player, state, timer, countdown, visits, line, extra, frames_in = start
    cpu.mem_write(WORK, bytes(0x100))
    cpu.mem_write(WORK, struct.pack(">bbbb", player, state, *visits))
    cpu.mem_write(WORK + 6, struct.pack(">b", line))
    cpu.mem_write(WORK + 8, struct.pack(">h", timer))
    cpu.mem_write(WORK + 0xE, struct.pack(">h", countdown))
    cpu.mem_write(WORK + 4, struct.pack(">bb", extra["reason"], extra["switch_count"]))
    cpu.mem_write(WORK + 0xA, struct.pack(">hh", extra["ring_clock"], extra["pool"]))
    rings[:] = extra["rings"]
    native = []
    global frame, events
    for frame in frames_in:
        events = []
        counters.update(rand=0, trig=0)
        if "debris" in frame:
            cpu.mem_write(player2[player_now()] + 0x230, struct.pack(">3f", *frame["debris"]["origin"]))
        cpu.mem_write(GLOBAL + 0x7C, struct.pack(">I", frame["global_7c"] & 0xFFFFFFFF))
        for s in range(2):
            cpu.mem_write(player1[s] + 0xC, struct.pack(">i", frame["yaws"][s]))
            cpu.mem_write(player2[s] + 1, struct.pack(">b", frame["characters"][s]))
        stage = frame.get("stage")
        if stage is not None:
            cpu.mem_write(GLOBAL + 0x42, struct.pack(">bb", stage["global_42"], stage["global_43"]))
            cpu.mem_write(TASK + 0xC, struct.pack(">I", stage["task_0c"]))
            if stage["channels"] is None:
                cpu.mem_write(VOICE, struct.pack(">I", 0))
            else:
                cpu.mem_write(VOICE, struct.pack(">I", CHANNELS))
                for i, ch in enumerate(stage["channels"]):
                    cpu.mem_write(CHANNELS + 0x1C * i + 0x30, struct.pack(">i", ch["state"]))
                    cpu.mem_write(CHANNELS + 0x1C * i + 0x25, struct.pack(">b", ch["category"]))
            for s in range(2):
                cpu.mem_write(PRESENT + 4 * s, struct.pack(">I", 0x9000 if stage["present"][s] else 0))
                cpu.mem_write(player1[s] + 0x14, struct.pack(">3f", *stage["positions"][s]))
                cpu.mem_write(player2[s] + 0x176, struct.pack(">2h", *stage["actions"][s]))
            cpu.mem_write(HOLD, struct.pack(">I", stage["global_664"]))
        cpu.reg_write(R.UC_PPC_REG_1, STACK)
        cpu.reg_write(R.UC_PPC_REG_29, TASK)
        cpu.reg_write(R.UC_PPC_REG_30, WORK)
        cpu.emu_start(CODE + FIRST, CODE + STOP + 4, count=20000)
        assert not saved, saved
        p, st = struct.unpack(">bb", cpu.mem_read(WORK, 2))
        t, = struct.unpack(">h", cpu.mem_read(WORK + 8, 2))
        c, = struct.unpack(">h", cpu.mem_read(WORK + 0xE, 2))
        ln, = struct.unpack(">b", cpu.mem_read(WORK + 6, 1))
        after = dict(player=p, switch_state=st, timer=t, countdown=c)
        if stage is not None:
            r4, r5 = struct.unpack(">bb", cpu.mem_read(WORK + 4, 2))
            rc, pl = struct.unpack(">hh", cpu.mem_read(WORK + 0xA, 4))
            after.update(line=ln, reason=r4, switch_count=r5, ring_clock=rc, pool=pl,
                         rings=list(rings), visits=list(struct.unpack(">bb", cpu.mem_read(WORK + 2, 2))))
        native.append((events, after))
        if st == 0 and stage is None:
            break
    return native


RUST_ONLY = {"resource_table_offset", "radial_clamp", "result", "voice", "spoke", "cue",
             "played", "motion_write"}


def flatten(rust_events):
    """Rust nests a local call's own events; the emulator sees them inline."""
    for r in rust_events:
        inner = r.get("events", [])
        yield {k: v for k, v in r.items() if k not in RUST_ONLY and k != "events"}
        yield from flatten(inner)


f32 = lambda v: struct.unpack(">f", struct.pack(">f", v))[0]


def stage_frame(rng):
    positions = [[f32(rng.choice((0.0, 240.0, 260.0, 1499.0, -1600.0)) + rng.uniform(-2, 2)),
                  f32(rng.choice((0.0, 301.0, -501.0))),
                  f32(rng.choice((0.0, 1400.0, 1600.0, -2000.0)) + rng.uniform(-2, 2))]
                 for _ in range(2)]
    busy = rng.random() < 0.3
    return dict(
        channels=None if rng.random() < 0.15 else [
            dict(state=rng.choice((1, 6) if busy else (0, 5, -1)), category=rng.choice((3, 2)))
            for _ in range(2)],
        present=[rng.random() < 0.85 for _ in range(2)],
        positions=positions,
        distances=[f32(rng.choice((math.hypot(p[0], p[2]), 1500.0, 0.0))) for p in positions],
        global_42=rng.choice((0, 0, 0, 0, 1)), global_43=rng.choice((0, 0, 0, 0, 1)),
        task_0c=rng.choice((0, 0, 0, 0x9100)),
        global_664=rng.choice((0, 0, 0, 0, 0, 1)),
        boss_flags=rng.choice((None, 0, 0, 4, -1)),
        actions=[[rng.choice((0, 0, 0, 0x6D, 0x74)) for _ in range(2)] for _ in range(2)],
        sound_queries=[rng.choice((0, 1, 1)) for _ in range(2)])


rng = random.Random(421)
Path(".local").mkdir(exist_ok=True)
replays = frames_total = staged = spoken = 0
seen = set()
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for case in range(400):
        player = case & 1
        state = (1, 2, 3)[case // 2 % 3]
        normal = case >= 240  # replays that start in state 0 (or out of range)
        if normal:
            state = rng.choice((0, 0, 0, 0, 4, 7, -1))
        timer = rng.choice((0, 100, 119, 120, 121, 179, 180, 181, -3, 32767))
        countdown = rng.choice((0, 1, 5, -2))
        visits = [rng.choice((-1, 0, 1, 2, 3)) for _ in range(2)]
        line = rng.choice((0, 0, 3, 9, 10, -1))
        with_stage = case >= 160
        extra = dict(reason=0, switch_count=0, ring_clock=0, pool=0, rings=[0, 0])
        if normal:
            extra = dict(reason=rng.choice((0, 0, 1)), switch_count=rng.choice((-1, 0, 2, 3, 5)),
                         ring_clock=rng.choice((0, 55, 60, 61)),
                         pool=rng.choice((30, 40, 50)),
                         rings=[rng.choice((1, 2, 5, 16, 17, 40)) for _ in range(2)])
        poll_rate = rng.choice((0.0, 0.1, 0.5, 1.0))
        clock = rng.choice((0, 1796, 9000, 17396, 17950) if normal else (0, 1796, 9000, 17396))
        frames_in = []
        for k in range(rng.choice((1, 3, 40, 400))):
            mode = rng.choice((-1, 13, 14, 20))
            fr = dict(
                poll=rng.choice((1, -1, 7)) if rng.random() < poll_rate else 0,
                characters=[rng.choice((9, 10, 0, -1)) for _ in range(2)],
                yaws=[rng.choice((0, 0x4000, 0x8000, 0x9000, -1, 0x7FFFFFFF, -0x80000000))
                      for _ in range(2)],
                camera_mode=mode, second_lookup=0x2A + mode,
                cue_result=rng.choice((-1, 0, 5)),
                global_7c=(clock + k) if with_stage else rng.choice((0, 14399, 14400, 14401)))
            if with_stage:
                fr["stage"] = stage_frame(rng)
                fr["debris"] = dict(
                    randoms=[rng.choice((0, 1, 16384, 32767, rng.randrange(32768), -5, 40000))
                             for _ in range(7)],
                    trig=[f32(rng.uniform(-1, 1)) for _ in range(5)],
                    origin=[f32(rng.uniform(-100, 100)) for _ in range(3)])
            if normal:
                for pos in fr["stage"]["positions"]:
                    pos[2] = f32(rng.choice((-60.0, -50.0, -40.0, 0.0, 40.0, 50.0, 60.0)))
            frames_in.append(fr)
        if normal and rng.random() < 0.3:
            # Land the first drain exactly on the 18000-frame boundary (or one before).
            extra["ring_clock"] = 60
            base = rng.choice((17999, 18000))
            for k, fr in enumerate(frames_in):
                fr["global_7c"] = base + k
            frames_in[0]["stage"].update(global_664=0, global_42=0, global_43=0)
        native = run((player, state, timer, countdown, visits, line, extra, frames_in))
        frames_in = frames_in[:len(native)]
        inp = dict(path="switch_replay", player=player, switch_state=state, timer=timer,
                   countdown=countdown, visits=visits, player1=player1, player2=player2,
                   player_tasks=player_tasks, table_words=table, line=line, frames=frames_in,
                   **extra)
        q = p / "request.json"
        out = p / f"{case}.json"
        q.write_text(json.dumps(inp))
        res = subprocess.run([exe, "finalhazard-placement-clock", module, out, q],
                             capture_output=True, text=True)
        assert res.returncode == 0, (case, res.stderr[-600:])
        got = json.loads(out.read_text())
        assert len(got["frames"]) == len(native), (case, len(got["frames"]), len(native))
        for n, (rust, (events, after)) in enumerate(zip(got["frames"], native)):
            assert {k: rust[k] for k in after} == after, (case, n, rust, after)
            rev = list(flatten(rust["events"]))
            assert len(rev) == len(events), (case, n, rev, events)
            for r, e in zip(rev, events):
                if "args" in r and "call" in r:
                    e = dict(e, args=e["args"][:len(r["args"])])
                assert r == e, (case, n, r, e)
            spoken += sum(1 for e in events if e.get("local") == 0x18ED0)
        assert got["returned_to_normal_dispatch"] == (native[-1][1]["switch_state"] == 0)
        replays += 1
        staged += with_stage
        frames_total += len(native)
        seen.update(e["site"] for events, _ in native for e in events)
# Every handoff branch and every stage-local call site must have been exercised.
required = {0xF47C, 0xF498, 0xF4C4, 0xF4D8, 0xF52C, 0xF55C, 0xF564, 0xF5C8, 0xF690, 0xF69C,
            0xF6A8, 0xF6C0, 0xF718, 0xF7E4, 0xF7F0, 0xF7FC, 0xF800, 0xF838, 0xF848, 0xF86C,
            0xF87C, 0xF884, 0xF8BC, 0xF8C4, 0xFF34, 0xEBC0, 0xEF74, 0xF038, 0xF0E8, 0xF380,
            0xF3A4, 0xF3DC, 0xFC28, 0xFC54, 0xFC68, 0xFC8C, 0xFC9C, 0xFCC0, 0xFD18, 0xFD7C,
            0xFDA4, 0xFDD8, 0xFDDC, 0xFE90, 0xFEC4, 0xFECC, 0xFEDC, 0xFEFC, 0xFF10, 0xED08, 0xF97C, 0xFB58, 0xFBA0, 0xFBC4, 0xFBD8}
# The ring transfer inside 0xec04 needs reason 1, which blocks the state 0 trigger;
# only the boss's own call reaches it, so it is not required here.
assert required <= seen, sorted(hex(s) for s in required - seen)
print(f"{replays} installed PPC handoff replays ({frames_total} frames, {staged} with stage "
      f"calls run natively, {spoken} voice lines) matched")
