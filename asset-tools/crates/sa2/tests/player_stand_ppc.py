"""Run main.dol's standing step fn_80054D04 (with its idle voice
fn_8005118C) on random players and print JSON for tests/player_stand.rs.
rand, GetStageNum and the voice call are stubbed and logged; the voice
tables are main.dol's own, read here and passed to the Rust side.

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
TW, MW, PW = 0x81100000, 0x81101000, 0x81102000
GS, GRAV = 0x801CC168, 0x801E5624
rng = random.Random(0x54D04)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
log, rands = [], []
stage = [0]


def rand(mm):
    v = rands.pop(0)
    log.append(["rand", v])
    mm.gpr(3, v)


m.stub(0x8010DC88, rand)
m.stub(0x8001DD78, lambda mm: mm.gpr(3, stage[0]))
m.stub(0x80011714, lambda mm: log.append(["voice", mm.gpr(3) - 0x10000 if mm.gpr(3) & 0x8000 else mm.gpr(3) & 0xFFFF]))
# The character-5 hook: the function pointer at 0x801A2A18 + 0x2C.
HOOK = 0x81200000
hook = [0]
m.stub(HOOK, lambda mm: (log.append(["hook"]), mm.gpr(3, hook[0])))
any_stage, by_stage = [], []
a = 0x801B3168
while m.get(a, "h") != -1:
    any_stage.append([m.get(a, "h"), m.get(a + 2, "h"), m.get(a + 4, "h")])
    a += 6
a = 0x801B3078
while m.get(a + 1, "B") != 0x5B:
    by_stage.append([m.get(a, "b"), m.get(a + 1, "b"), m.get(a + 2, "h"), m.get(a + 4, "h")])
    a += 6
stages = sorted({e[1] for e in by_stage})
cases = []
for _ in range(6000):
    ang = [rng.randrange(-0x3000, 0x3000), rng.randrange(0, 0x10000), rng.randrange(-0x3000, 0x3000)]
    tilt = rng.choice([0.0, 0.1, 0.5, 0.7, 0.9, 1.2])
    n = [f32(rng.uniform(-tilt, tilt)), f32(rng.choice([1.0, rng.uniform(0.3, 1)])), f32(rng.uniform(-tilt, tilt))]
    grav = [0.0, -1.0, 0.0] if rng.random() < 0.9 else [f32(rng.uniform(-0.3, 0.3)), -1.0, 0.0]
    c = rng.choice([0, 1, 2, 3, 4, 5, 6, 7, 6, 7, 9])
    act = rng.choice([0, 1, 2, 3, 0x1A, 0x3F, 0x40, 0x10, 0x11, 0x1B, 0x74, 5, 0x58, 0x59, 0x57, 0xB1, 0x8C, 0x8E,
                      rng.randrange(0, 0x100)])
    w = {"pnum": rng.choice([0, 0, 1]), "c": c, "vc": rng.choice([c, rng.randrange(0, 0x20)]),
         "idle": rng.choice([0, 0xB4, 0xB5, 0x384, 0x385, 1000, rng.randrange(0, 1000)]), "attr": rng.choice([0, 0x100]),
         "action": act, "f44": 7.0, "h17c": 5}
    g = {"b1e": rng.choice([0, 0, 1]), "b39": rng.choice([0, 0, 1]), "frame": rng.randrange(0, 1 << 32),
         "stage": rng.choice(stages + [0x5A, 0x0B])}
    aim = rng.choice([ang[1], (ang[1] + 0x100) & 0xFFFF, rng.randrange(0, 0x10000)])
    rands[:] = [rng.randrange(0, 0x8000) for _ in range(2)]
    given = rands[:]
    hook[0] = rng.choice([0, 1])
    loaded = rng.random() < 0.7
    stage[0] = g["stage"]
    m.u.mem_write(TW + 8, struct.pack(">3i", *ang))
    m.put(MW + 0x1C, "i", aim)
    m.put(PW, "b", w["pnum"])
    m.put(PW + 1, "b", c)
    m.put(PW + 3, "b", w["vc"])
    m.put(PW + 0x18, "h", w["idle"])
    m.put(PW + 0x44, "f", 7.0)
    m.u.mem_write(PW + 0x7C, struct.pack(">3f", *n))
    m.put(PW + 0x88, "I", w["attr"])
    m.put(PW + 0x176, "h", act)
    m.put(PW + 0x17C, "h", 5)
    m.u.mem_write(GRAV, struct.pack(">3f", *grav))
    m.put(GS + 0x1E, "b", g["b1e"])
    m.put(GS + 0x39, "b", g["b39"])
    m.put(GS + 0x7C, "I", g["frame"])
    m.put(0x801E5D28 + 0x6C4, "I", 0x81300000 if loaded else 0)
    m.put(0x801A2A18 + 0x2C, "I", HOOK)
    log.clear()
    m.call(0x80054D04, r3=TW, r4=MW, r5=PW)
    cases.append({"ang": ang, "n": [fb(x) for x in n], "grav": [fb(x) for x in grav], "w": w, "g": g, "aim": aim,
                  "rands": given, "hook": hook[0], "loaded": loaded,
                  "out": [m.get(PW + 0x18, "h"), fb(m.get(PW + 0x44, "f")), m.get(PW + 0x176, "h"), m.get(PW + 0x17C, "h")],
                  "log": log[:]})
print(json.dumps({"any": any_stage, "by": by_stage, "cases": cases}))
