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


W = lambda a: m.get(a, "I")


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
# fn_800420C0: surface physics. GetStageNum is stubbed to the case's stage.
TASK = 0x81104000
PHYS = 0x80196578
stage = [0]
m.stub(0x8001DD78, lambda mm: mm.gpr(3, stage[0]))
rows = [[m.get(PHYS + 0x84 * k + o, "I") for o in (0x3C, 0x58, 0x78)] for k in range(16)]
surface = []
for _ in range(3000):
    stage[0] = rng.choice([30, 32, 40, 43, 44, 50, 53, 31, 13, 11, 0, 70, rng.randrange(-5, 80)])
    ch, b3 = rng.choice([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]), rng.randrange(0, 14)
    attr = rng.choice([0, 0x8, 0x4, 0x10, 0x200000, rng.randrange(1 << 32)])
    flags = rng.choice([0, 2, 0x2000, 0x2002, rng.randrange(1 << 32)])
    pos_y, f134, h1b0 = rf(100), f32(rng.uniform(0, 20)), rf(100)
    before = [rng.randrange(1 << 32), fb(rf(2)), fb(rf(2)), fb(rf(2)), fb(rf(2))]
    m.put(PW + 1, "b", ch)
    m.put(PW + 3, "b", b3)
    m.put(PW + 0x88, "I", before[0])
    m.put(PW + 0x40, "I", before[1])
    m.put(PW + 0xFC, "I", before[2])
    m.put(PW + 0x118, "I", before[3])
    m.put(PW + 0x138, "I", before[4])
    m.put(PW + 0x134, "f", f134)
    m.put(PW + 0x1A8, "I", flags)
    m.put(PW + 0x1B0, "f", h1b0)
    m.put(TASK + 0x18, "f", pos_y)
    m.call(0x800420C0, r3=TASK, r4=VEL, r5=PW, r6=attr)
    surface.append({"stage": stage[0], "ch": ch, "b3": b3, "attr": attr, "flags": flags,
                    "pos_y": fb(pos_y), "f134": fb(f134), "h1b0": fb(h1b0), "before": before,
                    "after": [m.get(PW + o, "I") for o in (0x88, 0x40, 0xFC, 0x118, 0x138)]})
# fn_800414F0 / fn_80041984: sort contact hits (the 0x58-byte copies at
# 0x80295D10) into floors and walls and cut the motion.
TAILS, Q, ENTRY = 0x80295D10, 0x81105000, 0x81106000
R13 = 0x803B52C0
slide = []
for _ in range(4000):
    n = rng.choice([0, 1, 1, 2, 3, 5, 8])
    up = [0.0, 1.0, 0.0] if rng.random() < 0.6 else unit()
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    center = [rf(50) for _ in range(3)]
    motion = [rf(8) for _ in range(3)]
    vel = [rf(8) for _ in range(3)]
    hits = []
    for _ in range(n):
        if hits and rng.random() < 0.2:
            facing = hits[-1][0]
        else:
            facing = rng.choice([up, unit(), unit(), [-x for x in motion] if any(motion) else unit()])
            nn = sum(x * x for x in facing) ** 0.5 or 1.0
            facing = [f32(x / nn) for x in facing]
        head = [f32(center[j] + rng.uniform(-30, 30)) for j in range(3)]
        hits.append((facing, head))
    for k, (facing, head) in enumerate(hits):
        put3(TAILS + 0x58 * k + 0x14, facing)
        put3(TAILS + 0x58 * k + 0x2C, head)
    m.put(Q + 0x28, "H", n)
    put3(Q + 4, center)
    put3(Q + 0x10, motion)
    put3(Q + 0x1C, up)
    put3(VEL, vel)
    put3(GLOBAL, glob)
    mode = rng.choice([0x4C, 0x50, 0x52, 0x10, 0x53, 0x4B])
    ch = rng.choice([0, 1, 2, 6])
    f134 = f32(rng.uniform(0, 20))
    has_task = rng.random() < 0.3
    m.put(TASK, "b", mode)
    m.put(PW + 1, "b", ch)
    m.put(PW + 0x134, "f", f134)
    m.put(ENTRY + 8, "I", 0x81107000 if has_task else 0)
    m.put(R13 - 0x78F4, "I", 0)
    air = rng.random() < 0.5
    f26 = f32(rng.choice([1000.0, 0.0, rng.uniform(0, 60)]))
    if air:
        m.call(0x80041984, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=ENTRY)
    else:
        m.call(0x800414F0, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=ENTRY, f26=f26)
    slide.append({"air": air, "f26": fb(f26), "mode": mode, "ch": ch, "f134": fb(f134), "task": has_task,
                  "center": [fb(x) for x in center], "up": [fb(x) for x in up], "global": [fb(x) for x in glob],
                  "motion": [fb(x) for x in motion], "vel": [fb(x) for x in vel],
                  "hits": [[[fb(x) for x in f], [fb(x) for x in h]] for f, h in hits],
                  "ret": m.gpr(3), "stuck": W(R13 - 0x78F4),
                  "motion_after": [W(Q + 0x10 + 4 * j) for j in range(3)],
                  "vel_after": [W(VEL + 4 * j) for j in range(3)]})
print(json.dumps({"lift": lift, "rows": rows, "surface": surface, "slide": slide,
                  "row74": [m.get(PHYS + 0x74, "I"), m.get(PHYS + 0xF8, "I")]}))
