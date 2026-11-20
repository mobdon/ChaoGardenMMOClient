"""Run main.dol's pre-switch step fn_80056450 on random players and print
JSON for tests/player_pre.rs. The collision-list queries, the gravity
mode and the steps it hands over to (fn_8003D7D4, fn_80054D04,
fn_8003D8D0) are stubbed and logged.

Needs SA2_MAIN_DOL (GSNE8P main.dol, never committed) and the local Gekko
tables.
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
from dol_ppc import Dol  # noqa: E402

gekko_tables.require()
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
R13 = 0x803B52C0
TASK, TW, MW, PW, CWP, INFO = 0x81100000, 0x81101000, 0x81102000, 0x81103000, 0x81104000, 0x81104100
OT, OTW, OCWP, OINFO, PPW = 0x81110000, 0x81111000, 0x81112000, 0x81112100, 0x81120000
GS, GRAV = 0x801CC168, 0x801E5624
rng = random.Random(0x56450)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
log = []
st = {}


def kind_hit(mm):
    k = mm.gpr(4)
    log.append(["kind", k])
    mm.gpr(3, OT if st["kinds"].get(k) else 0)


def flag(name):
    def f(mm):
        log.append([name])
        mm.gpr(3, OT if st[name] else 0)
    return f


m.stub(0x80005500, lambda mm: mm.gpr(3, st["gmode"]))
m.stub(0x800060EC, kind_hit)
m.stub(0x800063F4, flag("h63f4"))
m.stub(0x80006270, flag("h6270"))
m.stub(0x80005C74, flag("first"))
m.stub(0x8003D7D4, lambda mm: log.append(["settle", mm.gpr(3) - PW]))
m.stub(0x80054D04, lambda mm: log.append(["stand", m.get(TW, "b"), m.get(PW + 0x18, "h")]))
m.stub(0x8003D8D0, lambda mm: log.append(["motion", mm.gpr(3) - PW]))
cases = []
for _ in range(8000):
    gmode = rng.choice([0, 0, 1, 2, 3, 4, 5])
    pos = [f32(rng.uniform(-50, 50)) for _ in range(3)]
    opos = [f32(pos[j] + rng.uniform(-8, 8)) for j in range(3)]
    tilt = rng.choice([0, 0, rng.randrange(-0x3000, 0x3000)])
    ang = [rng.choice([0, tilt]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x3000, 0x3000)])]
    grav = [0.0, -1.0, 0.0] if rng.random() < 0.8 else [f32(rng.uniform(-1, 1)), f32(rng.uniform(-1, 1)), f32(rng.uniform(-1, 1))]
    w = {"mode": rng.choice([0, 1, 0x36, 0x35, 0x12, 0x45, 0x39, 0x37, 0x4F]), "b1": rng.choice([0, 0x24, 9, 1]),
         "flag": rng.choice([0, 0x20, 0x21, 0x22, 0x2520, rng.randrange(0, 0x10000)]), "h6": 7,
         "ang": ang, "pos": [fb(x) for x in pos], "info": rng.choice([0, 0x4000]),
         "mwspd": [fb(f32(rng.choice([0.0, 1e-6, -1e-6, rng.uniform(-2, 2)]))) for _ in range(3)],
         "mwang": [rng.randrange(-0x4000, 0x4000), rng.randrange(0, 0x10000), rng.randrange(-0x4000, 0x4000)],
         "pnum": rng.choice([0, 1]), "c": rng.choice([4, 5, 6, 7]), "w28": rng.choice([0x36, 0x55, 3]),
         "h10": rng.randrange(-0x8000, 0x8000), "h18": 9, "action": 0x22,
         "f138": fb(f32(rng.uniform(0, 20))),
         "here": [rng.choice([0, rng.randrange(-0x3000, 0x3000)]), rng.choice([0, rng.randrange(-0x3000, 0x3000)]),
                  0, rng.choice([0, 0x100, 0x2000, 1]), fb(1e6), fb(f32(pos[1] - rng.uniform(-5, 25))), 0, 0]}
    g = {"b1e": rng.choice([0, 1]), "b39": rng.choice([0, 0, 1]), "b42": rng.choice([0, 0, 1]), "b49": rng.choice([0, 1, 2, 3]),
         "b3f": rng.choice([0, 1, 1]), "gmode": gmode, "level": [rng.randrange(-0x2000, 0x2000), rng.randrange(-0x2000, 0x2000)],
         "grav": [fb(x) for x in grav]}
    o = {"pos": [fb(x) for x in opos], "yaw": rng.randrange(0, 0x10000), "z": rng.choice([0, rng.randrange(-0x4000, 0x4000)]),
         "info": rng.choice([0, 0x4000])}
    st.clear()
    st.update({"gmode": gmode, "kinds": {0x11: rng.random() < 0.4, 0xB: rng.random() < 0.2}, "h63f4": rng.random() < 0.3,
               "h6270": rng.random() < 0.5, "first": True})
    m.u.mem_write(TW, struct.pack(">bBhhh", w["mode"], w["b1"], 0,
                                  w["flag"] - 0x10000 if w["flag"] & 0x8000 else w["flag"], 7))
    m.u.mem_write(TW + 8, struct.pack(">3i3f", *ang, *pos))
    m.put(TW + 0x2C, "I", CWP)
    m.put(CWP + 0xC, "I", INFO)
    m.put(INFO + 4, "I", w["info"])
    m.u.mem_write(MW, struct.pack(">3I", *w["mwspd"]))
    m.u.mem_write(MW + 0x18, struct.pack(">3i", *w["mwang"]))
    m.put(PW, "b", w["pnum"])
    m.put(PW + 1, "b", w["c"])
    m.put(PW + 0xC, "B", 9)
    m.put(PW + 0x10, "h", w["h10"])
    m.put(PW + 0x18, "h", 9)
    m.put(PW + 0x28, "i", w["w28"])
    m.put(PW + 0x88, "I", 0x77)
    m.put(PW + 0x9C, "I", 5)
    m.put(PW + 0xA0, "I", 6)
    m.put(PW + 0xA4, "I", 0x12345678)
    m.u.mem_write(PW + 0x64, struct.pack(">3f", 1.0, 2.0, 3.0))
    m.put(PW + 0x138, "I", w["f138"])
    m.put(PW + 0x176, "h", 0x22)
    m.u.mem_write(PW + 0x1A0, struct.pack(">2i6I", *w["here"]))
    m.put(OT + 0x34, "I", OTW)
    m.u.mem_write(OTW + 8, struct.pack(">3i3I", 0, o["yaw"], o["z"], *o["pos"]))
    m.put(OTW + 0x2C, "I", OCWP)
    m.put(OCWP + 0xC, "I", OINFO)
    m.put(OINFO + 4, "I", o["info"])
    for k in ("b1e", "b39", "b42", "b49"):
        m.put(GS + int(k[1:], 16), "b", g[k])
    m.put(GS + 0x3F + w["pnum"], "b", g["b3f"])
    m.put(R13 - 0x79A8, "i", g["level"][0])
    m.put(R13 - 0x79AC, "i", g["level"][1])
    m.u.mem_write(GRAV, struct.pack(">3f", *grav))
    m.put(0x801E7728 + 4 * w["pnum"], "I", PPW)
    m.put(PPW + 0x180, "h", 0)
    log.clear()
    m.call(0x80056450, r3=TASK, r4=TW, r5=MW, r6=PW)
    cases.append({"w": w, "g": g, "o": o, "st": {**st, "kinds": {str(k): v for k, v in st["kinds"].items()}},
                  "out": {"tw": [x for k, x in enumerate(struct.unpack(">bBhhh", m.u.mem_read(TW, 8))) if k != 2], "ang": [m.get(TW + 8 + 4 * j, "i") for j in range(3)],
                          "pos": [m.get(TW + 0x14 + 4 * j, "I") for j in range(3)],
                          "mwspd": [m.get(MW + 4 * j, "I") for j in range(3)],
                          "mwang": [m.get(MW + 0x18 + 4 * j, "i") for j in range(3)],
                          "bc": m.get(PW + 0xC, "B"), "h10": m.get(PW + 0x10, "h"), "h18": m.get(PW + 0x18, "h"),
                          "attr": m.get(PW + 0x88, "I"), "w9c": m.get(PW + 0x9C, "I"), "wa0": m.get(PW + 0xA0, "I"),
                          "wa4": m.get(PW + 0xA4, "I"), "spd": [m.get(PW + 0x64 + 4 * j, "I") for j in range(3)],
                          "action": m.get(PW + 0x176, "h"),
                          "here": list(struct.unpack(">2i6I", m.u.mem_read(PW + 0x1A0, 0x20))),
                          "h180": m.get(PPW + 0x180, "h")},
                  "log": log[:]})
print(json.dumps({"cases": cases}))
