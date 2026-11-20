"""Local PowerPC oracle for the walker collision helpers (GSNE8P main.dol).

Runs each helper on random player states and prints JSON with the inputs
and outputs for tests/walker_collide.rs. Needs SA2_MAIN_DOL.
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import dol_ppc  # noqa: E402
import gekko_fres  # noqa: E402

m = dol_ppc.Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"]).machine(boot=False)
gekko_fres.install(m, dol_ppc)
m.boot()
rng = random.Random(0x428F8)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
VEL, PW, COLL, OUT = 0x81100000, 0x81101000, 0x81102000, 0x81103000
GLOBAL = 0x801E5624


def rf(scale):
    return f32(rng.choice([0.0, rng.uniform(-scale, scale), rng.uniform(-scale, scale)]))


def unit():
    v = [rng.gauss(0, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(x / n) for x in v]


def put3(a, v):
    m.u.mem_write(a, struct.pack(">3f", *v))


lift = []
for _ in range(3000):
    vel = [rf(10) for _ in range(3)]
    up = unit() if rng.random() < 0.8 else [0.0, 1.0, 0.0]
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    height = f32(rng.uniform(1, 20))
    last = rng.choice([0, 0x100, 0x8, 0x108, rng.randrange(1 << 16)])
    spd = [rf(8), rf(4)]
    status = rng.choice([0, 1, 2, 3, 0x2000, 0x2001, rng.randrange(1 << 16)]) & 0xFFFF
    run, f4 = f32(rng.uniform(1, 6)), f32(rng.uniform(4, 12))
    out_y = rf(10)
    put3(VEL, vel)
    put3(COLL + 0x1C, up)
    put3(GLOBAL, glob)
    m.put(PW + 0xC4, "f", height)
    m.put(PW + 0x8C, "I", last)
    m.put(PW + 0x64, "f", spd[0])
    m.put(PW + 0x68, "f", spd[1])
    m.put(PW + 0x1C, "H", status)
    m.put(PW + 0xE8, "f", run)
    m.put(PW + 0xF4, "f", f4)
    put3(OUT, [f32(rng.uniform(-5, 5)), out_y, f32(rng.uniform(-5, 5))])
    m.call(0x800428F8, r3=VEL, r4=PW, r5=COLL, r6=OUT)
    lift.append({"motion": [fb(x) for x in vel], "up": [fb(x) for x in up], "global": [fb(x) for x in glob],
                 "height": fb(height), "last": last, "spd": [fb(x) for x in spd], "status": status,
                 "run": fb(run), "f4": fb(f4), "out_y": fb(out_y),
                 "out": [m.get(OUT + 4 * j, "I") for j in range(3)]})
print(json.dumps({"lift": lift}))
