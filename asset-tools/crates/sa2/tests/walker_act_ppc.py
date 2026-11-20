"""Run the walker's action-step routines from main.dol (GSNE8P) on random
native work areas and print JSON for tests/walker_act.rs: each case's
function, the task, motion and player work before and after (hex) and the
log of stubbed callees.

Needs SA2_MAIN_DOL (never committed) and the local Gekko tables.
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402

fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
from dol_ppc import Dol  # noqa: E402

gekko_tables.require()
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
R13 = 0x803B52C0
TW, MW, PW = 0x81100000, 0x81101000, 0x81102000
TWN, MWN, PWN = 0x30, 0x40, 0x454
GS = 0x801CC168
rng = random.Random(int(os.environ.get("SA2_ACT_SEED", "0x24F4"), 0))
log = []
g = {}


def s16(v):
    v &= 0xFFFF
    return v - 0x10000 if v & 0x8000 else v


m.stub(0x80054D04, lambda mm: log.append(["stand"]))
m.stub(0x800BCF50, lambda mm: log.append(["deferred", 0x800BCF50, mm.gpr(3) - (1 << 32) if mm.gpr(3) & 0x80000000 else mm.gpr(3)]))
m.stub(0x80011714, lambda mm: log.append(["voice", s16(mm.gpr(3))]))
m.stub(0x8001DD78, lambda mm: mm.gpr(3, g["stage"]))


def floats(n):
    return b"".join(struct.pack(">f", rng.choice([0.0, 1.0, rng.uniform(-10, 10), rng.uniform(-1, 1)])) for _ in range(n // 4))


FUNCS = {
    "command": 0x800AD910,
    "go_stand": 0x8005523C,
    "pressed": 0x8004E454,
    "held": 0x8004E400,
    "queue": 0x80053784,
    "f48": 0x800B5630,
    "events": 0x800AD584,
    "buttons": 0x80053C74,
    "ground": 0x80053DBC,
}
cases = []
for k in range(int(os.environ.get("SA2_ACT_CASES", "6000"))):
    name = list(FUNCS)[k % len(FUNCS)]
    t = bytearray(floats(TWN))
    mw = bytearray(floats(MWN))
    p = bytearray(floats(PWN))
    t[0] = rng.choice([0, 1, 0x36, 2, 6])
    t[1] = rng.choice(list(range(0x27)) + [0x40, 0xFF])
    struct.pack_into(">H", t, 4, rng.choice([0x1000, 0x1000 | rng.randrange(0, 0x10000), rng.randrange(0, 0x10000), 0x4000]))
    p[0] = rng.choice([0, 1, 2, 3])
    p[1] = rng.choice([6, 7, 5])
    p[3] = rng.choice([6, 7, 0])
    p[0xC] = rng.choice([0, 0, 1, 7, 8, 0xFF])
    p[0xD] = rng.choice([0x2D, 0x2E, 0x30, 0x32, 0, 7])
    struct.pack_into(">h", p, 0x12, rng.randrange(-5, 40))
    struct.pack_into(">i", p, 0xC0, rng.randrange(0, 40))
    struct.pack_into(">i", p, 0x28, rng.choice([0x36, 0x55, 0x56, 0x57, 3, 0x54]))
    struct.pack_into(">I", p, 0x24, rng.choice([0, 0x800000, rng.randrange(0, 1 << 32)]))
    g.clear()
    g.update({"stage": rng.choice([0x1D, 0x14, 0xB, 0x22]), "b49": rng.choice([0, 1, 2, 0xF]),
              "pressed": [rng.choice([0, 0, 1]) for _ in range(8)], "held": [rng.choice([0, 1]) for _ in range(8)],
              "m56": rng.random() < 0.5})
    m.u.mem_write(TW, bytes(t))
    m.u.mem_write(MW, bytes(mw))
    m.u.mem_write(PW, bytes(p))
    gs = bytearray(rng.choice([0, 0, 0, 1, 2, 0xFF]) for _ in range(0x100))
    gs[0x49] = g["b49"] & 0xFF
    g["gs"] = gs.hex()
    m.u.mem_write(GS, bytes(gs))
    m.u.mem_write(R13 - 0x7A38, bytes(x & 0xFF for x in g["pressed"]))
    m.u.mem_write(R13 - 0x7A28, bytes(x & 0xFF for x in g["held"]))
    m.put(0x801E5D28 + 0x2B4, "I", 0x81300000 if g["m56"] else 0)
    log.clear()
    a = FUNCS[name]
    if name == "f48":
        m.call(a, r3=PW)
    elif name in ("pressed", "held", "queue", "events", "buttons", "ground"):
        m.call(a, r3=TW, r4=PW)
    elif name == "go_stand":
        m.call(a, r3=TW, r4=MW, r5=PW)
    else:
        m.call(a, r3=TW, r4=MW, r5=PW, r6=PW)
    cases.append({"fn": name, "g": g.copy(), "t": t.hex(), "m": mw.hex(), "p": p.hex(), "ret": m.gpr(3),
                  "t_after": bytes(m.u.mem_read(TW, TWN)).hex(), "m_after": bytes(m.u.mem_read(MW, MWN)).hex(),
                  "p_after": bytes(m.u.mem_read(PW, PWN)).hex(), "log": log[:]})
# fn_800AC950 with fn_80037AB8, fn_80005510 and fn_8000EDD0 stubbed.
lk = {}
centers = {}


def edd0(mm):
    c = centers[mm.gpr(3)]
    m.u.mem_write(mm.gpr(5), struct.pack(">3f", *c))
    log.append(["edd0"])


m.stub(0x80037AB8, lambda mm: mm.gpr(3, lk["partner"] & 0xFFFFFFFF))
m.stub(0x80005510, lambda mm: mm.gpr(3, lk["counts"][mm.gpr(3)]))
m.stub(0x8000EDD0, edd0)
LISTS = (0x801E31B0, 0x801E2FB0)
OBJ = 0x81200000
lock = []
for _ in range(3000):
    t = bytearray(floats(TWN))
    p = bytearray(floats(PWN))
    t[0] = rng.choice([0, 1, 0x11, 2])
    struct.pack_into(">3i", t, 8, rng.randrange(-0x2000, 0x2000), rng.randrange(0, 0x10000), rng.randrange(-0x2000, 0x2000))
    base = [rng.uniform(-50, 50) for _ in range(3)]
    struct.pack_into(">3f", p, 0x218, *base)
    p[0] = rng.choice([0, 1])
    near = lambda r: [base[j] + rng.uniform(-r, r) for j in range(3)]
    lk.clear()
    lk["partner"] = rng.choice([-1, 1, 0])
    lk["counts"] = [rng.randrange(0, 4), rng.randrange(0, 4)]
    pq = near(rng.choice([25, 40, 80]))
    m.put(0x801E7728 + 4 * (lk["partner"] & 3), "I", OBJ + 0x8000)
    m.u.mem_write(OBJ + 0x8000 + 0x218, struct.pack(">3f", *pq))
    centers.clear()
    lists = []
    k = 0
    for li in range(2):
        ents = []
        for j in range(lk["counts"][li]):
            task = OBJ + 0x400 * k
            k += 1
            tw_, cwp, tgt, ttw, info = task + 0x100, task + 0x180, task + 0x200, task + 0x280, task + 0x300
            valid = rng.choice(["ok", "ok", "ok", "nocwp", "notgt", "notw"])
            kind = rng.choice([0, 0xC, 3])
            c = [f32(x) for x in near(rng.choice([25, 40, 80]))]
            m.put(LISTS[li] + 4 * j, "I", task)
            m.put(task + 0x34, "I", tw_)
            m.put(tw_ + 0x2C, "I", 0 if valid == "nocwp" else cwp)
            m.put(cwp + 0x9C, "I", 0 if valid == "notgt" else tgt)
            m.put(tgt + 0x34, "I", 0 if valid == "notw" else ttw)
            m.put(cwp + 0xC, "I", info)
            m.put(info, "B", kind)
            centers[ttw] = c
            ents.append({"valid": valid == "ok", "kind": kind, "c": [fb(x) for x in c]})
        lists.append(ents)
    m.u.mem_write(TW, bytes(t))
    m.u.mem_write(PW, bytes(p))
    log.clear()
    m.call(0x800AC950, r3=TW, r4=PW)
    lock.append({"t": t.hex(), "p": p.hex(), "partner": lk["partner"], "pq": [fb(f32(x)) for x in pq], "lists": lists,
                 "ret": m.gpr(3), "log": log[:]})
print(json.dumps({"cases": cases, "lock": lock}))
