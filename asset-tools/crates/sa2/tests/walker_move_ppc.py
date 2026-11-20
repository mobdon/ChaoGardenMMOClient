"""Local PowerPC oracle for the shared player movement steps (GSNE8P main.dol).

Runs fn_80047A1C and fn_80048ACC on random player states and prints JSON for
tests/walker_move.rs. Needs SA2_MAIN_DOL and the local Gekko tables.
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
rng = random.Random(0x47A1C)
R13 = 0x803B52C0
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
W = lambda a: m.get(a, "I")
TASK, MW, PW, Q = 0x81100000, 0x81101000, 0x81102000, 0x81103000
GLOBAL = 0x801E5624


def rf(s):
    return f32(rng.choice([0.0, rng.uniform(-s, s), rng.uniform(-s, s)]))


def put3(a, v):
    m.u.mem_write(a, struct.pack(">3f", *v))


def get3(a):
    return [W(a + 4 * j) for j in range(3)]


level = []
for _ in range(3000):
    flag = rng.choice([0, 0, 1, 2, 0x100])
    ang = [rng.randrange(-0x8000, 0x8000) for _ in range(3)]
    pos, spd = [rf(100) for _ in range(3)], [rf(8) for _ in range(3)]
    w24, f4, f134 = rng.choice([0, 0x8000]), f32(rng.uniform(2, 12)), f32(rng.uniform(5, 30))
    target = (rng.choice([0, 0, rng.randrange(-0x2000, 0x2000)]), rng.choice([0, 0, rng.randrange(-0x2000, 0x2000)]))
    m.put(TASK + 4, "H", flag)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(TASK + 0x14, pos)
    put3(PW + 0x64, spd)
    m.put(PW + 0x24, "I", w24)
    m.put(PW + 0xF4, "f", f4)
    m.put(PW + 0x134, "f", f134)
    m.put(R13 - 0x79A8, "i", target[0])
    m.put(R13 - 0x79AC, "i", target[1])
    m.call(0x80047A1C, r3=TASK, r4=MW, r5=PW)
    level.append({"flag": flag, "ang": ang, "pos": [fb(x) for x in pos], "spd": [fb(x) for x in spd], "w24": w24,
                  "f4": fb(f4), "f134": fb(f134), "target": target,
                  "ang_after": [m.get(TASK + 8 + 4 * j, "i") for j in range(3)], "pos_after": get3(TASK + 0x14),
                  "spd_after": get3(PW + 0x64)})

acc = []
for _ in range(4000):
    flag = rng.choice([0, 1, 2, 3, 0x4000, 0x4001])
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.8 else [f32(rng.uniform(-1, 1)) for _ in range(3)]
    spd = [rf(6) for _ in range(3)]
    mwacc, across = [rf(1) for _ in range(3)], [rf(3) for _ in range(3)]
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0x40, 0, 2), (0xD0, 0.5, 2), (0x114, -0.1, 0), (0x118, -0.1, 0),
                                                       (0x11C, -0.1, 0), (0x120, 0, 2), (0x124, 0, 2), (0x128, 0, 2),
                                                       (0x138, 0.05, 0.12))}
    m.put(TASK + 4, "H", flag)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(GLOBAL, glob)
    put3(PW + 0x64, spd)
    for o, x in f.items():
        m.put(PW + o, "f", x)
    put3(MW + 0xC, mwacc)
    m.put(PW + 0x90, "I", Q)
    put3(Q + 0x78, across)
    put3(PW + 0x58, [1.5, 2.5, 3.5])
    m.call(0x80048ACC, r3=TASK, r4=MW, r5=PW)
    acc.append({"flag": flag, "ang": ang, "global": [fb(x) for x in glob], "spd": [fb(x) for x in spd],
                "mwacc": [fb(x) for x in mwacc], "across": [fb(x) for x in across],
                "f": {hex(o): fb(x) for o, x in f.items()},
                "acc_after": get3(PW + 0x58), "across_after": get3(Q + 0x78)})
print(json.dumps({"level": level, "acc": acc}))
