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
active = [0]
calls = [0]
def fake_analog(mm):
    calls[0] += 1
    mm.gpr(3, active[0])
m.stub(0x8004E560, fake_analog)
speeds = []
for i in range(1500):
    jog = f32(rng.choice([0.46, rng.uniform(0, 2)]))
    run = f32(rng.choice([1.39, rng.uniform(0, 3)]))
    brk = f32(rng.choice([-0.18, rng.uniform(-1, 0)]))
    spd = [f32(rng.choice([0.0, -0.0, rng.uniform(-3, 3), jog * 0.87, -jog * 0.87])) for _ in range(3)]
    acc = [f32(rng.choice([0.0, rng.uniform(-0.5, 0.5)])) for _ in range(3)]
    dotp = f32(rng.choice([1.0, 0.5, rng.uniform(0, 1)]))
    attr = rng.choice([0, 0x100, rng.randrange(1 << 32)])
    normal = rng.choice([[0.0, 1.0, 0.0], unit([rng.uniform(-1, 1) for _ in range(3)])])
    lim = [f32(rng.choice([16.0, 1.0, rng.uniform(0, 4)])) for _ in range(2)]
    flags = rng.choice([0, 0x2000, 0x2, 0x2002, rng.randrange(1 << 32)])
    h = f32(rng.uniform(-50, 80))
    mode = rng.choice([0, 6, 6, 1])
    angles = [rng.choice([0, rng.randrange(-0x10000, 0x10000), rng.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]
    pos_y = f32(rng.uniform(-50, 50))
    world_y = f32(rng.choice([1.0, -1.0, 0.0]))
    active[0] = rng.choice([0, 1])
    calls[0] = 0
    m.u.mem_write(PWP + 0x64, struct.pack(">3f", *spd))
    m.u.mem_write(PWP + 0x58, struct.pack(">3f", *acc))
    m.u.mem_write(PWP + 0x34, struct.pack(">f", dotp))
    m.put(PWP + 0x88, "I", attr)
    m.u.mem_write(PWP + 0x7C, struct.pack(">3f", *normal))
    m.u.mem_write(PWP + 0xE4, struct.pack(">2f", jog, run))
    m.u.mem_write(PWP + 0x108, struct.pack(">f", brk))
    m.u.mem_write(PWP + 0xC8, struct.pack(">2f", *lim))
    m.put(PWP + 0x1A8, "I", flags)
    m.u.mem_write(PWP + 0x1B0, struct.pack(">f", h))
    m.put(TASK, "B", mode)
    for k, a in enumerate(angles):
        m.put(TASK + 8 + 4 * k, "i", a)
    m.u.mem_write(TASK + 0x18, struct.pack(">f", pos_y))
    m.u.mem_write(VEC, struct.pack(">3f", 0.0, world_y, 0.0))
    m.call(0x80046DE4, r3=TASK, r4=VEC, r5=PWP)
    speeds.append({
        "in": [[fb(x) for x in spd], [fb(x) for x in acc], fb(dotp), attr, [fb(x) for x in normal],
               fb(jog), fb(run), fb(brk), [fb(x) for x in lim], flags, fb(h), mode, angles, fb(pos_y), fb(world_y), active[0]],
        "spd": [m.get(PWP + 0x64 + 4 * k, "I") for k in range(3)],
        "world": [m.get(VEC + 4 * k, "I") for k in range(3)],
        "calls": calls[0],
    })
print(json.dumps({"frame": out, "clamp": clamps, "speed": speeds}))
