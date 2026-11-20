"""Local PowerPC oracle for the hunter routines in src/hunter.rs
(GSNE8P main.dol): fn_80091454, fn_80093270 and fn_8009B708.

Runs main.dol's own code on randomized task and player work and prints JSON
(floats as bit patterns). tests/hunter_dol.rs replays them.
Usage: python hunter_ppc.py [MAIN_DOL]
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402
import gekko_tables  # noqa: E402

gekko_tables.require()  # njUnitVector's frsqrte

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
TWP, CWP, INFO, PWP = 0x81000000, 0x81000100, 0x81000200, 0x81000400
TABLES = 0x803AD894
rng = random.Random(0x91454)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]


def rf(scale=50.0):
    k = rng.random()
    if k < 0.1:
        return rng.choice([0.0, -0.0])
    if k < 0.15:
        return rng.choice([4.0, 7.0, -7.0, float("inf"), float("nan")])
    return f32(rng.uniform(-scale, scale))


def setf(a, v):
    m.u.mem_write(a, struct.pack(">f", v))


def getb(a):
    return m.get(a, "I")


m.put(TWP + 0x2C, "I", CWP)
m.put(CWP + 0xC, "I", INFO)

drop = []
for _ in range(3000):
    action = rng.choice([6, 6, 1, rng.randrange(256)])
    flag = rng.choice([0, 1, 2, 3, rng.randrange(-0x8000, 0x8000)])
    pl = rng.randrange(4)
    table = bytes(rng.choice([0, 0, 0, 1, 0x80]) for _ in range(12))
    m.u.mem_write(TABLES, table)
    center_y, spd_y, h0 = rf(), rf(5.0), rf()
    h1 = rng.choice([rf(), f32(h0 + rng.uniform(-10, 10)), f32(h0 + 7.0)])
    f1a8 = rng.choice([0, 2, 0xFFFFFFFF, rng.randrange(1 << 32)])
    m.put(TWP, "B", action)
    m.put(TWP + 4, "h", flag)
    m.put(PWP, "B", pl)
    setf(INFO + 0xC, center_y)
    setf(PWP + 0x68, spd_y)
    m.put(PWP + 0x1A8, "I", f1a8)
    setf(PWP + 0x1B0, h0)
    setf(PWP + 0x1B4, h1)
    m.call(0x80091454, r3=TWP, r5=PWP)
    drop.append({
        "in": [action, flag, fb(center_y), fb(spd_y), f1a8, fb(h0), fb(h1),
               table[pl], table[4 + pl]],
        "out": m.gpr(3),
    })

climb = []
for _ in range(3000):
    motion = rng.choice([0x62, 0x68, 0x6A, 0x64, rng.randrange(-0x8000, 0x8000)])
    flag = rng.choice([0, 1, 3, rng.randrange(-0x8000, 0x8000)])
    x = rng.choice([0.0, -0.0, rf(3.0)])
    z = rng.choice([0.0, -0.0, rf(3.0), x, f32(-x)])
    req, f44 = rng.randrange(-0x8000, 0x8000), rf()
    m.put(TWP + 4, "h", flag)
    m.put(PWP + 0x178, "h", motion)
    setf(PWP + 0x64, x)
    setf(PWP + 0x6C, z)
    m.put(PWP + 0x176, "h", req)
    setf(PWP + 0x44, f44)
    m.call(0x80093270, r3=TWP, r4=PWP)
    climb.append({
        "in": [flag, motion, fb(x), fb(z), req, fb(f44)],
        "request": m.get(PWP + 0x176, "h"),
        "f44": getb(PWP + 0x44),
    })

center = []
for _ in range(1500):
    action = rng.choice([0x3D, 0x3E, 0x41, 0x42, 0x40, 0x43, rng.randrange(256)])
    angles = [rng.choice([0, 0x4000, rng.randrange(-0x10000, 0x10000),
                          rng.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]
    pos = [rf(5000.0) for _ in range(3)]
    height = rng.choice([0.0, f32(rng.uniform(0, 20))])
    push = rf(10.0)
    m.put(TWP, "B", action)
    for k, a in enumerate(angles):
        m.put(TWP + 8 + 4 * k, "i", a)
    m.u.mem_write(TWP + 0x14, struct.pack(">3f", *pos))
    setf(PWP + 0x140, height)
    setf(PWP + 0x3B4, push)
    m.call(0x8009B708, r3=TWP, r5=PWP, r6=PWP)
    center.append({
        "in": [action, angles, [fb(v) for v in pos], fb(height), fb(push)],
        "out": [getb(INFO + 8 + 4 * k) for k in range(3)],
    })

json.dump({"drop": drop, "climb": climb, "center": center}, sys.stdout)
