"""Local PowerPC oracle for the walker exec's per-frame steps (GSNE8P main.dol).

Runs fn_8004F4D0 and fn_800AC698 on random player states and prints JSON for
tests/walker_exec.rs. Needs SA2_MAIN_DOL and the local Gekko tables.
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import dol_ppc  # noqa: E402
import gekko_tables  # noqa: E402

gekko_tables.require()
m = dol_ppc.Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"]).machine()
rng = random.Random(0x4F4D0)
R13 = 0x803B52C0
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
W = lambda a: m.get(a, "I")
TASK, MW, PW, CWP, INFO = 0x81100000, 0x81101000, 0x81102000, 0x81103000, 0x81103100
PTASK, HPOS, HANG, NEWT, NEWPW = 0x81104000, 0x81110000, 0x81120000, 0x81130000, 0x81131000


def put3(a, v):
    m.u.mem_write(a, struct.pack(">3f", *v))


def get3(a):
    return [W(a + 4 * j) for j in range(3)]


created = []


def cet(mm):
    created.append([mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6)])
    mm.gpr(3, NEWT)


m.stub(0x80071E0C, cet)
m.put(NEWT + 0x40, "I", NEWPW)
m.put(R13 - 0x7908, "I", HPOS)
m.put(R13 - 0x7904, "I", HANG)
end = []
for _ in range(4000):
    pnum = rng.choice([0, 0, 1])
    flag = rng.randrange(0, 0x10000)
    ang = [rng.randrange(-0x4000, 0x4000) for _ in range(3)]
    pos = [f32(rng.uniform(-100, 100)) for _ in range(3)]
    ppos = [f32(rng.uniform(-100, 100)) for _ in range(3)]
    pang = [rng.randrange(0, 0x10000) for _ in range(3)]
    hidx = rng.choice([0, 0xFE, 0xFF, rng.randrange(0, 0x100)])
    b = {"b1": rng.choice([0, 4, 4]), "b3": rng.choice([0, 0x10, 6]), "bd": rng.choice([0, 0x32, 7])}
    h = {"h10": rng.choice([0, 0x4000, 0x10]), "h14": rng.randrange(-3, 4), "h16": rng.choice([0, 0x473, 0x472, 0x474, 5]),
         "h1a": rng.randrange(-3, 4), "h1e": rng.randrange(0, 9)}
    wd = {"w24": rng.choice([0, 0x2000]), "w1b8": rng.choice([0, 2, 0x2000, 0x10])}
    head = f32(rng.uniform(0, 30))
    water = f32(rng.choice([pos[1] + rng.uniform(-30, 30), 1e9, -1e9]))
    gm = rng.choice([0, 1, 2])
    m.put(TASK + 4, "H", flag)
    m.put(TASK + 1, "B", 7)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(TASK + 0x14, pos)
    m.put(TASK + 0x2C, "I", CWP)
    m.put(CWP + 0xC, "I", INFO)
    m.put(INFO + 2, "B", 0xFF)
    put3(MW + 0xC, [1.0, 2.0, 3.0])
    m.put(PW, "B", pnum)
    m.put(PW + 1, "B", b["b1"])
    m.put(PW + 3, "B", b["b3"])
    m.put(PW + 0xD, "B", b["bd"])
    for k, o in (("h10", 0x10), ("h14", 0x14), ("h16", 0x16), ("h1a", 0x1A), ("h1e", 0x1E)):
        m.put(PW + o, "h", h[k])
    m.put(PW + 0x24, "I", wd["w24"])
    m.put(PW + 0x1B8, "I", wd["w1b8"])
    m.put(PW + 0xA8, "I", 5)
    m.put(PW + 0xAC, "I", 6)
    for o in (0x38, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60):
        m.put(PW + o, "f", 9.0)
    m.put(PW + 0x13C, "f", head)
    m.put(PW + 0x1B0, "f", water)
    m.put(0x801CC168 + 0x42, "B", gm)
    m.put(R13 - 0x7900, "I", 0x1234)
    m.put(R13 - 0x7910 + pnum, "B", hidx)
    m.put(0x801E7768 + 4 * pnum, "I", PTASK)
    m.u.mem_write(PTASK + 8, struct.pack(">3i", *pang))
    put3(PTASK + 0x14, ppos)
    created.clear()
    m.put(NEWPW, "B", 0xEE)
    m.call(0x8004F4D0, r3=TASK, r4=MW, r5=PW)
    slot = (pnum << 8) + ((hidx + 1) & 0xFF)
    end.append({"pnum": pnum, "flag": flag, "ang": ang, "pos": [fb(x) for x in pos], "ppos": [fb(x) for x in ppos],
                "pang": pang, "hidx": hidx, "b": b, "h": h, "w": wd, "head": fb(head), "water": fb(water), "gm": gm,
                "flag_after": m.get(TASK + 4, "H"), "tb1_after": m.get(TASK + 1, "B"), "mwacc_after": get3(MW + 0xC),
                "pf_after": [W(PW + o) for o in (0x38, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60)],
                "h_after": [m.get(PW + o, "h") for o in (0x14, 0x16, 0x1A, 0x1E)],
                "wa8": W(PW + 0xA8), "wac": W(PW + 0xAC), "bd_after": m.get(PW + 0xD, "B"), "info_b2": m.get(INFO + 2, "B"),
                "r13_7900": W(R13 - 0x7900), "hidx_after": m.get(R13 - 0x7910 + pnum, "B"),
                "hpos": get3(HPOS + 0xC * slot), "hang": [m.get(HANG + 0xC * slot + 4 * j, "i") for j in range(3)],
                "created": created[:], "newpw": m.get(NEWPW, "B")})
# fn_800AC698 (with fn_800068BC) on random collision info bytes.
TOBJ = 0x81140000
m.put(TOBJ + 0x34, "I", TASK)
ccl = []
for _ in range(3000):
    info = bytes(rng.randrange(0, 256) for _ in range(0xF0))
    mode = rng.choice([0x41, 0x3D, 0x3E, 1, 0x41])
    b2 = rng.choice([3, 0, 3])
    pos = [f32(rng.uniform(-100, 100)) for _ in range(3)]
    h10 = rng.choice([0, 0x8000, 0x10])
    f21c = f32(rng.uniform(0, 30))
    v1e8 = [rng.randrange(0, 1 << 32) for _ in range(3)]
    has = rng.random() < 0.9
    m.put(TASK, "B", mode)
    m.put(TASK + 2, "B", b2)
    put3(TASK + 0x14, pos)
    m.put(TASK + 0x2C, "I", CWP if has else 0)
    m.put(CWP + 8, "f", 1.25)
    m.u.mem_write(INFO, info)
    m.put(PW + 0x10, "h", h10 - 0x10000 if h10 & 0x8000 else h10)
    m.put(PW + 0x21C, "f", f21c)
    m.u.mem_write(PW + 0x1E8, struct.pack(">3I", *v1e8))
    m.call(0x800AC698, r3=TOBJ, r4=TASK, r5=PW)
    ccl.append({"info": info.hex(), "mode": mode, "b2": b2, "pos": [fb(x) for x in pos], "h10": h10, "f21c": fb(f21c),
                "v1e8": v1e8, "has": has, "info_after": bytes(m.u.mem_read(INFO, 0xF0)).hex(), "range": W(CWP + 8)})
print(json.dumps({"end": end, "ccl": ccl}))
