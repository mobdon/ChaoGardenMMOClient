"""Local PowerPC oracle for fn_8003E668 (GSNE8P main.dol).

Runs main.dol's own routine on randomized task, vector and player work and
prints JSON (floats as bit patterns). tests/walker_frame.rs replays them.
Usage: python walker_frame_ppc.py [MAIN_DOL]
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
TASK, VEC, PWP = 0x81000000, 0x81000100, 0x81000200
GLOBAL = 0x801E5624
GET_STAGE = 0x8001DD78
rng = random.Random(0x3E668)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
stage = [0]
m.stub(GET_STAGE, lambda mm: mm.gpr(3, stage[0]))


def unit(v):
    n = sum(c * c for c in v) ** 0.5
    return [f32(c / n) for c in v]


def rnd_speed():
    k = rng.random()
    if k < 0.15:
        return f32(rng.uniform(-0.002, 0.002))
    if k < 0.25:
        return rng.choice([0.0, -0.0])
    return f32(rng.uniform(-8.0, 8.0))


out = []
for i in range(1500):
    flag = rng.choice([0, 1, 2, 3, 4, 0x13])
    angles = [rng.choice([0, 0x4000, rng.randrange(-0x10000, 0x10000), rng.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]
    vangles = [rng.randrange(-(1 << 31), 1 << 31) for _ in range(3)]
    world = [rnd_speed() for _ in range(3)]
    attr = rng.choice([0, 0x100, rng.randrange(1 << 32)])
    last_attr = rng.choice([0, 4, 8, rng.randrange(1 << 32)])
    normal = rng.choice([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], unit([rng.uniform(-1, 1) for _ in range(3)])])
    run_accel = f32(rng.choice([0.1, 0.05, rng.uniform(0, 1)]))
    weight = f32(rng.choice([0.2, 0.08, rng.uniform(0, 1)]))
    glob = rng.choice([[0.0, -1.0, 0.0], unit([rng.uniform(-1, 1) for _ in range(3)])])
    stage[0] = rng.choice([11, 0x5A, 0x5A])
    m.put(TASK + 4, "h", flag)
    for k, a in enumerate(angles):
        m.put(TASK + 8 + 4 * k, "i", a)
    m.u.mem_write(VEC, struct.pack(">3f", *world))
    for k, a in enumerate(vangles):
        m.put(VEC + 0x18 + 4 * k, "i", a)
    m.put(PWP + 0x88, "I", attr)
    m.put(PWP + 0x8C, "I", last_attr)
    m.u.mem_write(PWP + 0x7C, struct.pack(">3f", *normal))
    m.u.mem_write(PWP + 0xFC, struct.pack(">f", run_accel))
    m.u.mem_write(PWP + 0x138, struct.pack(">f", weight))
    m.u.mem_write(GLOBAL, struct.pack(">3f", *glob))
    m.call(0x8003E668, r3=TASK, r4=VEC, r5=PWP)
    out.append({
        "in": [flag, angles, [fb(x) for x in world], vangles, attr, last_attr,
               [fb(x) for x in normal], fb(run_accel), fb(weight), [fb(x) for x in glob], stage[0]],
        "spd": [m.get(PWP + 0x64 + 4 * k, "I") for k in range(3)],
        "dotp": m.get(PWP + 0x34, "I"),
        "task": [m.get(TASK + 8 + 4 * k, "i") for k in range(3)],
        "vec": [m.get(VEC + 0x18 + 4 * k, "i") for k in range(3)],
    })
clamps = []
for i in range(600):
    v = [rnd_speed() * rng.choice([1, 4]) for _ in range(3)]
    lim = [f32(rng.choice([16.0, 1.0, rng.uniform(0, 8)])) for _ in range(2)]
    flag = rng.choice([0, 0x800])
    m.put(TASK + 4, "h", flag)
    m.u.mem_write(VEC, struct.pack(">3f", *v))
    m.u.mem_write(PWP + 0xC8, struct.pack(">2f", *lim))
    m.call(0x80046950, r3=TASK, r4=VEC, r5=PWP)
    clamps.append([[fb(x) for x in v], [fb(x) for x in lim], [m.get(VEC + 4 * k, "I") for k in range(3)]])
print(json.dumps({"frame": out, "clamp": clamps}))
