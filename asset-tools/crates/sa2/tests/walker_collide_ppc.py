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
import gekko_tables  # noqa: E402

gekko_tables.require()  # bit-exact frsqrte and fres

m = dol_ppc.Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"]).machine(boot=False)
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
# fn_8011B7B8: two planes' intersection line.
PA, PB, LINE = 0x81108000, 0x81108100, 0x81108200
planes = []
for _ in range(3000):
    # Zero normals make NaNs, whose sign x86 and PowerPC create differently.
    na = [f32(rng.uniform(-3, 3)) for _ in range(3)]
    nb = rng.choice([[f32(rng.uniform(-3, 3)) for _ in range(3)], [f32(x * rng.uniform(-2, 2) + rng.uniform(-0.05, 0.05)) for x in na]])
    pa, pb = [rf(100) for _ in range(3)], [rf(100) for _ in range(3)]
    put3(PA, pa); put3(PA + 12, na); put3(PB, pb); put3(PB + 12, nb)
    m.u.mem_write(LINE, bytes([0xEE]) * 24)
    with_line = rng.random() < 0.9
    m.call(0x8011B7B8, r3=PA, r4=PB, r5=LINE if with_line else 0)
    planes.append({"a": [[fb(x) for x in pa], [fb(x) for x in na]], "b": [[fb(x) for x in pb], [fb(x) for x in nb]],
                   "with_line": with_line, "ret": fb(f32(m.fpr(1))), "line": [W(LINE + 4 * j) for j in range(6)]})

# fn_80040ACC: slide along the nearest wall.
wall = []
for _ in range(4000):
    n = rng.choice([0, 1, 1, 2, 3, 5])
    up = [0.0, 1.0, 0.0] if rng.random() < 0.6 else unit()
    center = [rf(50) for _ in range(3)]
    motion = [rf(12) for _ in range(3)]
    vel = [rf(8) for _ in range(3)]
    along, across, head = [rf(8) for _ in range(3)], [rf(3) for _ in range(3)], [rf(50) for _ in range(3)]
    if rng.random() < 0.1:
        along = [0.0, 0.0, 0.0]
    hits = []
    for _ in range(n):
        mm = sum(x * x for x in motion) ** 0.5 or 1.0
        facing = rng.choice([unit(), [f32(-x / mm + rng.uniform(-0.3, 0.3)) for x in motion], up])
        nn = sum(x * x for x in facing) ** 0.5 or 1.0
        facing = [f32(x / nn) for x in facing]
        foot = [f32(center[j] + rng.uniform(-20, 20)) for j in range(3)]
        hd = [f32(center[j] + rng.uniform(-8, 8)) for j in range(3)]
        hits.append((facing, foot, hd))
    for k, (facing, foot, hd) in enumerate(hits):
        put3(TAILS + 0x58 * k + 0x14, facing)
        put3(TAILS + 0x58 * k + 0x20, foot)
        put3(TAILS + 0x58 * k + 0x2C, hd)
    m.put(Q + 0x28, "H", n)
    put3(Q + 4, center); put3(Q + 0x10, motion); put3(Q + 0x1C, up)
    put3(Q + 0x60, head); put3(Q + 0x6C, along); put3(Q + 0x78, across)
    put3(VEL, vel)
    ch = rng.choice([0, 1, 2, 6])
    f134 = f32(rng.uniform(0, 20))
    last = rng.choice([0, 0, 0x100])
    eflags = rng.choice([0, 0x08000000, 1])
    m.put(PW + 1, "b", ch); m.put(PW + 0x134, "f", f134); m.put(PW + 0x8C, "I", last)
    put3(PW + 0x70, [1.5, 2.5, 3.5])
    m.call(0x80040ACC, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=eflags)
    wall.append({"ch": ch, "f134": fb(f134), "last": last, "eflags": eflags,
                 "center": [fb(x) for x in center], "up": [fb(x) for x in up], "motion": [fb(x) for x in motion],
                 "vel": [fb(x) for x in vel], "along": [fb(x) for x in along], "across": [fb(x) for x in across],
                 "head": [fb(x) for x in head],
                 "hits": [[[fb(x) for x in v] for v in h] for h in hits],
                 "ret": m.gpr(3), "along_after": [W(Q + 0x6C + 4 * j) for j in range(3)],
                 "vel_after": [W(VEL + 4 * j) for j in range(3)], "wall_after": [W(PW + 0x70 + 4 * j) for j in range(3)]})
# fn_80042694: tilt the task toward the contact angles; and the angle helpers.
tilt = []
for _ in range(3000):
    ang = [rng.randrange(-0x20000, 0x20000) for _ in range(3)]
    qa = [rng.randrange(0, 0x10000), rng.randrange(0, 0x10000)]
    if rng.random() < 0.5:
        qa = [(ang[0] + rng.randrange(-0x3000, 0x3000)) & 0xFFFF, (ang[2] + rng.randrange(-0x3000, 0x3000)) & 0xFFFF]
    facing = unit() if rng.random() < 0.7 else [0.0, 1.0, 0.0]
    up = [0.0, 1.0, 0.0] if rng.random() < 0.5 else unit()
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    status = rng.choice([0, 1, 3, 0x2001, rng.randrange(1 << 16)])
    spd = [f32(rng.uniform(-1, 12)), f32(rng.uniform(1, 2)), f32(rng.uniform(2, 4)), f32(rng.uniform(4, 6)), f32(rng.uniform(6, 9))]
    flags = rng.choice([0, 0x00100000, rng.randrange(1 << 32)])
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    m.u.mem_write(Q + 0x34, struct.pack(">2i", *qa))
    put3(Q + 0x48, facing); put3(Q + 0x1C, up); put3(GLOBAL, glob)
    m.put(PW + 0x1C, "H", status)
    for o, x in zip((0x64, 0xE4, 0xE8, 0xEC, 0xF4), spd):
        m.put(PW + o, "f", x)
    m.call(0x80042694, r3=TASK, r4=PW, r5=Q, r6=flags)
    tilt.append({"ang": ang, "qa": qa, "facing": [fb(x) for x in facing], "up": [fb(x) for x in up],
                 "global": [fb(x) for x in glob], "status": status, "spd": [fb(x) for x in spd], "flags": flags,
                 "after": [m.get(TASK + 8 + 4 * j, "i") for j in range(3)]})
angles = []
for _ in range(3000):
    a, b, st = rng.randrange(-0x20000, 0x20000), rng.randrange(-0x20000, 0x20000), rng.randrange(0, 0x9000)
    out = []
    for addr, extra in ((0x8002B8E0, {}), (0x8002B904, {}), (0x8002B918, {"r5": st})):
        m.call(addr, r3=a & 0xFFFFFFFF, r4=b & 0xFFFFFFFF, **extra)
        out.append(m.gpr(3))
    angles.append([a, b, st] + out)
# fn_80042B4C (with fn_80040540): settle after a contact.
MW, POINT, OFFSET = 0x81109000, 0x81109100, 0x81109200
settle = []
for _ in range(4000):
    pnum = rng.choice([0, 1])
    m.put(PW, "b", pnum)
    m.put(0x801E7748 + 4 * pnum, "I", MW)
    tflag = rng.choice([0, 1, 2, 3, rng.randrange(1 << 16)])
    pos = [rf(50) for _ in range(3)]
    ang_y = rng.randrange(-0x20000, 0x20000)
    vel = [rf(6) for _ in range(3)]
    facing = unit()
    vals = {"center": [rf(50) for _ in range(3)], "motion": [rf(6) for _ in range(3)], "up": unit() if rng.random() < 0.4 else [0.0, 1.0, 0.0],
            "head": [rf(50) for _ in range(3)], "along": [rf(6) for _ in range(3)] if rng.random() < 0.9 else [0.0, 0.0, 0.0]}
    if rng.random() < 0.3:
        vel = [f32(-x * 2) for x in facing]
    point = [f32(pos[j] - facing[j] * rng.uniform(-5, 5)) for j in range(3)] if rng.random() < 0.8 else pos
    offset = [rf(10) for _ in range(3)]
    status = rng.choice([0, 1, 2, 3, rng.randrange(1 << 16)])
    spd = [rf(6) for _ in range(3)]
    jog = f32(rng.uniform(0, 4))
    ch = rng.choice([0, 1, 2, 6])
    f134 = f32(rng.uniform(0, 20))
    mws = [rf(6) for _ in range(3)]
    aim = rng.randrange(-0x20000, 0x20000)
    ret_in, flags, stuck = rng.choice([0, 1, 7]), rng.choice([0, 0x1000]), rng.choice([0, 1])
    m.put(TASK + 4, "H", tflag & 0xFFFF)
    put3(TASK + 0x14, pos)
    m.put(TASK + 0xC, "i", ang_y)
    put3(VEL, vel)
    put3(Q + 4, vals["center"]); put3(Q + 0x10, vals["motion"]); put3(Q + 0x1C, vals["up"])
    put3(Q + 0x48, facing); put3(Q + 0x60, vals["head"]); put3(Q + 0x6C, vals["along"])
    put3(POINT, point); put3(OFFSET, offset)
    m.put(PW + 0x1C, "H", status & 0xFFFF)
    put3(PW + 0x64, spd)
    m.put(PW + 0xE4, "f", jog)
    m.put(PW + 1, "b", ch); m.put(PW + 0x134, "f", f134)
    put3(PW + 0x70, [1.5, 2.5, 3.5])
    put3(MW, mws)
    m.put(MW + 0x1C, "i", aim)
    m.put(R13 - 0x78F4, "I", stuck)
    m.call(0x80042B4C, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=POINT, r8=OFFSET, r9=ret_in, r10=flags)
    settle.append({"pnum": pnum, "tflag": tflag, "pos": [fb(x) for x in pos], "ang_y": ang_y, "vel": [fb(x) for x in vel],
                   "facing": [fb(x) for x in facing], **{k: [fb(x) for x in v] for k, v in vals.items()},
                   "point": [fb(x) for x in point], "offset": [fb(x) for x in offset], "status": status,
                   "spd": [fb(x) for x in spd], "jog": fb(jog), "ch": ch, "f134": fb(f134),
                   "mw": [fb(x) for x in mws], "aim": aim, "ret_in": ret_in, "flags": flags, "stuck": stuck,
                   "ret": m.gpr(3),
                   "pos_after": [W(TASK + 0x14 + 4 * j) for j in range(3)], "ang_y_after": m.get(TASK + 0xC, "i"),
                   "vel_after": [W(VEL + 4 * j) for j in range(3)], "wall_after": [W(PW + 0x70 + 4 * j) for j in range(3)],
                   "center_after": [W(Q + 4 + 4 * j) for j in range(3)], "motion_after": [W(Q + 0x10 + 4 * j) for j in range(3)],
                   "along_after": [W(Q + 0x6C + 4 * j) for j in range(3)], "mw_after": [W(MW + 4 * j) for j in range(3)],
                   "aim_after": m.get(MW + 0x1C, "i")})
# fn_80041C78: grounded wall push.
push = []
for _ in range(4000):
    n = rng.choice([0, 1, 1, 2, 3, 5])
    up = [0.0, 1.0, 0.0] if rng.random() < 0.6 else unit()
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    center = [rf(50) for _ in range(3)]
    motion = [rf(10) for _ in range(3)]
    vel = [rf(8) for _ in range(3)]
    hits = []
    for _ in range(n):
        if hits and rng.random() < 0.2:
            facing = hits[-1][0]
        else:
            mm = sum(x * x for x in motion) ** 0.5 or 1.0
            facing = rng.choice([unit(), [f32(-x / mm + rng.uniform(-0.3, 0.3)) for x in motion], up])
            nn = sum(x * x for x in facing) ** 0.5 or 1.0
            facing = [f32(x / nn) for x in facing]
        hits.append((facing, [f32(center[j] + rng.uniform(-15, 15)) for j in range(3)]))
    for k, (facing, head) in enumerate(hits):
        put3(TAILS + 0x58 * k + 0x14, facing)
        put3(TAILS + 0x58 * k + 0x2C, head)
    m.put(Q + 0x28, "H", n)
    put3(Q + 4, center); put3(Q + 0x10, motion); put3(Q + 0x1C, up)
    put3(VEL, vel); put3(GLOBAL, glob)
    mode = rng.choice([0x4C, 0x10, 1, 1, 0])
    ch = rng.choice([0, 1, 2, 6])
    f134 = f32(rng.uniform(0, 20))
    has_task = rng.random() < 0.3
    eflags = rng.choice([0, 0x00080000, 0x00400000, 0x1000, 0x00401000, rng.randrange(1 << 32)])
    sp = [rf(10), f32(rng.uniform(2, 8)), f32(rng.uniform(0.5, 2)), f32(rng.uniform(2, 6))]
    m.put(TASK, "b", mode)
    m.put(PW + 1, "b", ch); m.put(PW + 0x134, "f", f134)
    for o, x in zip((0x64, 0xF0, 0xE4, 0xE8), sp):
        m.put(PW + o, "f", x)
    m.put(ENTRY, "I", eflags)
    m.put(ENTRY + 8, "I", 0x81107000 if has_task else 0)
    m.put(R13 - 0x78F4, "I", 0)
    m.call(0x80041C78, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=ENTRY)
    push.append({"mode": mode, "ch": ch, "f134": fb(f134), "task": has_task, "eflags": eflags,
                 "speeds": [fb(x) for x in sp], "center": [fb(x) for x in center], "up": [fb(x) for x in up],
                 "global": [fb(x) for x in glob], "motion": [fb(x) for x in motion], "vel": [fb(x) for x in vel],
                 "hits": [[[fb(x) for x in f], [fb(x) for x in h]] for f, h in hits],
                 "ret": m.gpr(3), "stuck": W(R13 - 0x78F4),
                 "motion_after": [W(Q + 0x10 + 4 * j) for j in range(3)], "vel_after": [W(VEL + 4 * j) for j in range(3)]})
# fn_80040F48: airborne wall slide that turns the task.
wturn = []
for _ in range(4000):
    n = rng.choice([0, 1, 1, 2, 3, 5])
    up = [0.0, 1.0, 0.0] if rng.random() < 0.6 else unit()
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    center = [rf(50) for _ in range(3)]
    motion = [rf(12) for _ in range(3)]
    vel = [rf(8) for _ in range(3)]
    along, head = [rf(8) for _ in range(3)], [rf(50) for _ in range(3)]
    if rng.random() < 0.1:
        along = [0.0, 0.0, 0.0]
    hits = []
    for _ in range(n):
        mm = sum(x * x for x in motion) ** 0.5 or 1.0
        facing = rng.choice([unit(), [f32(-x / mm + rng.uniform(-0.3, 0.3)) for x in motion], up])
        nn = sum(x * x for x in facing) ** 0.5 or 1.0
        facing = [f32(x / nn) for x in facing]
        hits.append((facing, [f32(center[j] + rng.uniform(-20, 20)) for j in range(3)],
                     [f32(center[j] + rng.uniform(-8, 8)) for j in range(3)]))
    if hits and rng.random() < 0.3:
        vel = [f32(-x * 3) for x in hits[0][0]]
    for k, (facing, foot, hd) in enumerate(hits):
        put3(TAILS + 0x58 * k + 0x14, facing)
        put3(TAILS + 0x58 * k + 0x20, foot)
        put3(TAILS + 0x58 * k + 0x2C, hd)
    m.put(Q + 0x28, "H", n)
    put3(Q + 4, center); put3(Q + 0x10, motion); put3(Q + 0x1C, up)
    put3(Q + 0x60, head); put3(Q + 0x6C, along); put3(GLOBAL, glob)
    put3(MW, vel)
    aim, ang_y = rng.randrange(-0x20000, 0x20000), rng.randrange(-0x20000, 0x20000)
    m.put(MW + 0x1C, "i", aim)
    m.put(TASK + 0xC, "i", ang_y)
    ch = rng.choice([0, 1, 2, 6])
    f134 = f32(rng.uniform(0, 20))
    last = rng.choice([0, 0, 0x100])
    flags = rng.choice([0, 0, 0x100, 0x08000000])
    sp = [rf(10), f32(rng.uniform(2, 8)), f32(rng.uniform(0.5, 4))]
    m.put(PW + 1, "b", ch); m.put(PW + 0x134, "f", f134); m.put(PW + 0x8C, "I", last)
    for o, x in zip((0x64, 0xF0, 0xE4), sp):
        m.put(PW + o, "f", x)
    put3(PW + 0x70, [1.5, 2.5, 3.5])
    m.call(0x80040F48, r3=TASK, r4=MW, r5=PW, r6=Q, r7=flags)
    wturn.append({"ch": ch, "f134": fb(f134), "last": last, "flags": flags, "speeds": [fb(x) for x in sp],
                  "center": [fb(x) for x in center], "up": [fb(x) for x in up], "global": [fb(x) for x in glob],
                  "motion": [fb(x) for x in motion], "vel": [fb(x) for x in vel], "along": [fb(x) for x in along],
                  "head": [fb(x) for x in head], "aim": aim, "ang_y": ang_y,
                  "hits": [[[fb(x) for x in v] for v in h] for h in hits],
                  "ret": m.gpr(3), "along_after": [W(Q + 0x6C + 4 * j) for j in range(3)],
                  "vel_after": [W(MW + 4 * j) for j in range(3)], "wall_after": [W(PW + 0x70 + 4 * j) for j in range(3)],
                  "aim_after": m.get(MW + 0x1C, "i"), "ang_y_after": m.get(TASK + 0xC, "i")})
# fn_80042E0C (with fn_800407A0): grounded-mode contact settle.
OFF2 = 0x81109300
gsettle = []
for _ in range(5000):
    pnum = rng.choice([0, 1])
    m.put(PW, "b", pnum)
    m.put(0x801E7748 + 4 * pnum, "I", MW)
    mode = rng.choice([0, 0x38, 0xd, 0xa, 0x10, 1])
    tflag = rng.choice([0, 1, 2, 3])
    ang = [rng.randrange(-0x8000, 0x8000) if rng.random() < 0.5 else 0 for _ in range(3)]
    pos = [rf(50) for _ in range(3)]
    facing = unit()
    vel = [rf(6) for _ in range(3)]
    if rng.random() < 0.3:
        vel = [f32(-x * rng.uniform(0.5, 3)) for x in facing]
    q = {"center": [rf(50) for _ in range(3)], "motion": [rf(6) for _ in range(3)],
         "up": unit() if rng.random() < 0.4 else [0.0, 1.0, 0.0], "head": [rf(50) for _ in range(3)],
         "along": [rf(6) for _ in range(3)] if rng.random() < 0.9 else [0.0, 0.0, 0.0], "foot": [rf(50) for _ in range(3)]}
    point = [f32(pos[j] - facing[j] * rng.uniform(-5, 5)) for j in range(3)] if rng.random() < 0.9 else pos
    off, off2 = [rf(10) for _ in range(3)], [rf(10) for _ in range(3)]
    status = rng.choice([0, 1, 2, 3])
    spd = [f32(rng.choice([rng.uniform(-2, 20), 16.0])), rf(3), rf(3)]
    ch = rng.choice([0, 1, 2, 4, 5, 6, 7])
    sw = {"f0": f32(rng.uniform(3, 12)), "weight": f32(rng.uniform(0, 1)), "jog": f32(rng.uniform(0, 3)),
          "f134": f32(rng.uniform(0, 20))}
    last = rng.choice([0, 0x80000, 0x100])
    wc0 = rng.randrange(1 << 32)
    mws = [rf(6) for _ in range(3)]
    aim = rng.randrange(-0x20000, 0x20000)
    ret_in = rng.choice([0, 5])
    flags = rng.choice([0, 0x1000, 0x00400000, 0x1080, 0x80])
    stuck, probe = rng.choice([0, 1]), rng.choice([0, 0, 1])
    stage[0] = rng.choice([0x2b, 11, 0x2b])
    m.put(TASK, "B", mode)
    m.put(TASK + 4, "H", tflag)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(TASK + 0x14, pos)
    put3(VEL, vel)
    put3(Q + 4, q["center"]); put3(Q + 0x10, q["motion"]); put3(Q + 0x1C, q["up"]); put3(Q + 0x48, facing)
    put3(Q + 0x54, q["foot"]); put3(Q + 0x60, q["head"]); put3(Q + 0x6C, q["along"])
    put3(POINT, point); put3(OFFSET, off); put3(OFF2, off2)
    m.put(PW + 1, "b", ch); m.put(PW + 0x1C, "H", status); put3(PW + 0x64, spd)
    m.put(PW + 0xF0, "f", sw["f0"]); m.put(PW + 0x138, "f", sw["weight"]); m.put(PW + 0xE4, "f", sw["jog"])
    m.put(PW + 0x134, "f", sw["f134"]); m.put(PW + 0x8C, "I", last); m.put(PW + 0xC0, "I", wc0)
    m.put(PW + 0x12, "H", 0x1234)
    put3(PW + 0x70, [1.5, 2.5, 3.5])
    put3(MW, mws)
    m.put(MW + 0x1C, "i", aim)
    m.put(R13 - 0x78F4, "I", stuck)
    m.put(R13 - 0x7900, "I", probe)
    m.put(dol_ppc.STACK_TOP - 0x100 + 8, "I", flags)
    m.call(0x80042E0C, r3=TASK, r4=VEL, r5=PW, r6=Q, r7=POINT, r8=OFFSET, r9=OFF2, r10=ret_in)
    rec = {"mode": mode, "tflag": tflag, "ang": ang, "pos": [fb(x) for x in pos], "facing": [fb(x) for x in facing],
           "vel": [fb(x) for x in vel], "point": [fb(x) for x in point], "off": [fb(x) for x in off],
           "off2": [fb(x) for x in off2], "status": status, "spd": [fb(x) for x in spd], "ch": ch,
           "last": last, "wc0": wc0, "mw": [fb(x) for x in mws], "aim": aim, "ret_in": ret_in, "flags": flags,
           "stuck": stuck, "probe": probe, "stage": stage[0]}
    rec.update({k: [fb(x) for x in v] for k, v in q.items()})
    rec.update({k: fb(v) for k, v in sw.items()})
    rec.update({"ret": m.gpr(3), "pos_after": [W(TASK + 0x14 + 4 * j) for j in range(3)],
                "ang_after": [m.get(TASK + 8 + 4 * j, "i") for j in range(3)],
                "vel_after": [W(VEL + 4 * j) for j in range(3)], "spd_after": [W(PW + 0x64 + 4 * j) for j in range(3)],
                "wall_after": [W(PW + 0x70 + 4 * j) for j in range(3)], "h12_after": m.get(PW + 0x12, "h"),
                "center_after": [W(Q + 4 + 4 * j) for j in range(3)], "motion_after": [W(Q + 0x10 + 4 * j) for j in range(3)],
                "along_after": [W(Q + 0x6C + 4 * j) for j in range(3)], "mw_after": [W(MW + 4 * j) for j in range(3)],
                "aim_after": m.get(MW + 0x1C, "i")})
    gsettle.append(rec)
# fn_80041814: any wall ahead.
ahead = []
for _ in range(3000):
    n = rng.choice([0, 1, 2, 3, 5])
    up = [0.0, 1.0, 0.0] if rng.random() < 0.6 else unit()
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.7 else unit()
    motion = [rf(10) for _ in range(3)]
    hits = [unit() for _ in range(n)]
    for k, facing in enumerate(hits):
        put3(TAILS + 0x58 * k + 0x14, facing)
        put3(TAILS + 0x58 * k + 0x2C, [rf(30) for _ in range(3)])
    m.put(Q + 0x28, "H", n)
    put3(Q + 4, [rf(30) for _ in range(3)]); put3(Q + 0x10, motion); put3(Q + 0x1C, up); put3(GLOBAL, glob)
    m.call(0x80041814, r3=TASK, r4=VEL, r5=PW, r6=Q)
    ahead.append({"up": [fb(x) for x in up], "global": [fb(x) for x in glob], "motion": [fb(x) for x in motion],
                  "hits": [[fb(x) for x in f] for f in hits], "ret": m.gpr(3)})
print(json.dumps({"ahead": ahead, "gsettle": gsettle, "lift": lift, "rows": rows, "surface": surface, "slide": slide, "planes": planes, "wall": wall,
                  "wturn": wturn,
                  "push": push,
                  "settle": settle,
                  "tilt": tilt, "angles": angles,
                  "row74": [m.get(PHYS + 0x74, "I"), m.get(PHYS + 0xF8, "I")]}))
