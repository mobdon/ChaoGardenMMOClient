"""Local PowerPC oracle for the shared player movement steps (GSNE8P main.dol).

Runs fn_80047A1C, fn_80048ACC, fn_80048EC4, fn_8004B330, fn_80048724 and
fn_80048834, fn_80048490, fn_80036CE4, fn_8004E490, fn_80048400, fn_80048370, fn_8004B168 on random player states and prints JSON for
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
dol = dol_ppc.Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
# njRotate's PSMTXRotAxisRad is paired-single code; the interpreter runs its
# MSL sinf/cosf calls on a second machine.
m = dol.machine(paired={**dol_ppc.DEFAULT_PAIRED, 0x800ED694: "PSMTXRotAxisRad"})
helper = dol.machine(boot=False)
helper.call(0x800ECE94)
_step = dol_ppc.Interp.step


def _step_msl(self, pc, ins, stack):
    if ins >> 26 == 18 and ins & 3 == 1:
        tgt = (pc + dol_ppc.sext(ins & 0x3FFFFFC, 26)) & 0xFFFFFFFF
        if tgt in (0x801115A8, 0x80111414):
            helper.call(tgt, f1=self.p0[1])
            self.p0[1] = self.p1[1] = helper.fpr(1)
            return pc + 4
    return _step(self, pc, ins, stack)


dol_ppc.Interp.step = _step_msl
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
# fn_80048EC4 (fn_80047714 stubbed: Sonic's player_turn has it).
m.stub(0x80047714, lambda mm: None)
run = []
for _ in range(4000):
    flag = rng.choice([0, 1, 2, 3, 0x4000])
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.8 else [f32(rng.uniform(-1, 1)) for _ in range(3)]
    spd = [rf(6) for _ in range(3)]
    mwacc = [rf(1) for _ in range(3)]
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0x40, 0, 2), (0x108, -0.2, 0), (0x114, -0.1, 0), (0x118, -0.1, 0),
                                                       (0x11C, -0.1, 0), (0x120, 0, 2), (0x124, 0, 2), (0x128, 0, 2),
                                                       (0x138, 0.05, 0.12))}
    m.put(TASK + 4, "H", flag)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(GLOBAL, glob)
    put3(PW + 0x64, spd)
    for o, x in f.items():
        m.put(PW + o, "f", x)
    put3(MW + 0xC, mwacc)
    put3(PW + 0x58, [1.5, 2.5, 3.5])
    m.call(0x80048EC4, r3=TASK, r4=MW, r5=PW)
    run.append({"flag": flag, "ang": ang, "global": [fb(x) for x in glob], "spd": [fb(x) for x in spd],
                "mwacc": [fb(x) for x in mwacc], "f": {hex(o): fb(x) for o, x in f.items()},
                "acc_after": get3(PW + 0x58)})
# fn_8004B330: fn_8004E560 is stubbed to the case's stick.
stick = [0, 0, 0.0]


def analog_stub(mm):
    mm.gpr(3, stick[0])
    if mm.gpr(5):
        mm.put(mm.gpr(5), "i", stick[1])
    if mm.gpr(6):
        mm.put(mm.gpr(6), "f", stick[2])


m.stub(0x8004E560, analog_stub)
air = []
for _ in range(4000):
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.8 else [f32(rng.uniform(-1, 1)) for _ in range(3)]
    spd = [rf(6) for _ in range(3)]
    mwacc, mwspd_y = [rf(1) for _ in range(3)], rf(3)
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0x100, 0, 0.2), (0x10C, 0, 1), (0x110, -0.1, 0), (0x118, -0.1, 0),
                                                       (0x11C, -0.1, 0), (0x138, 0.05, 0.12), (0xE8, 1, 5))}
    stick[0] = rng.choice([0, 1, 1])
    stick[1] = rng.choice([ang[1], (ang[1] + rng.randrange(-0x1800, 0x1800)) & 0xFFFF, rng.randrange(0, 0x10000)])
    stick[2] = f32(rng.uniform(0, 1))
    aim = rng.randrange(0, 0x10000)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(GLOBAL, glob)
    put3(PW + 0x64, spd)
    for o, x in f.items():
        m.put(PW + o, "f", x)
    put3(MW + 0xC, mwacc)
    m.put(MW + 4, "f", mwspd_y)
    m.put(MW + 0x1C, "i", aim)
    m.call(0x8004B330, r3=TASK, r4=MW, r5=PW)
    air.append({"ang": ang, "global": [fb(x) for x in glob], "spd": [fb(x) for x in spd], "mwacc": [fb(x) for x in mwacc],
                "mwspd_y": fb(mwspd_y), "f": {hex(o): fb(x) for o, x in f.items()}, "stick": [stick[0], stick[1], fb(stick[2])],
                "aim": aim, "acc_after": get3(PW + 0x58), "aim_after": m.get(MW + 0x1C, "i")})
# fn_80048724 and fn_80048834.
grav, m43 = [], []
for k in range(8000):
    full = k >= 4000
    flag = rng.choice([0, 1, 2, 3, 0x100, 0x101, 0x4000])
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    glob = [0.0, -1.0, 0.0] if rng.random() < 0.8 else [f32(rng.uniform(-1, 1)) for _ in range(3)]
    spd = [rf(6) for _ in range(3)]
    mwacc, across = [rf(1) for _ in range(3)], [rf(3) for _ in range(3)]
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0xD0, 0.5, 2), (0x114, -0.1, 0), (0x118, -0.1, 0),
                                                       (0x11C, -0.1, 0), (0x138, 0.05, 0.12), (0xE8, 0, 4))}
    f[0x34] = f32(rng.choice([0.0, -0.0, 0.98, 1.0, rng.uniform(-1, 1), rng.uniform(0.9, 1)]))
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
    m.call(0x80048834 if full else 0x80048724, r3=TASK, r4=MW, r5=PW)
    (m43 if full else grav).append({"flag": flag, "ang": ang, "global": [fb(x) for x in glob], "spd": [fb(x) for x in spd],
                "mwacc": [fb(x) for x in mwacc], "across": [fb(x) for x in across],
                "f": {hex(o): fb(x) for o, x in f.items()},
                "acc_after": get3(PW + 0x58), "across_after": get3(Q + 0x78)})
# fn_80048490 with its callees stubbed and logged.
log = []
m.stub(0x80038F40, lambda mm: log.append(["flagged", 0]))
m.stub(0x80048150, lambda mm: log.append(["slope", 0]))
m.stub(0x800473B4, lambda mm: log.append(["toward", mm.gpr(6)]))
m.stub(0x80047714, lambda mm: log.append(["smooth", mm.gpr(6)]))
CAM = 0x81104000
m.put(0x80175378, "I", CAM)
steer = []
for _ in range(3000):
    flag = rng.choice([0, 1, 2, 0x2000, 0x4000, 0, 1])
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    attr = rng.choice([0, 0x100000, 0x100000, 0x100100])
    n = [f32(rng.uniform(-0.6, 0.6)), f32(rng.uniform(0.5, 1)), f32(rng.uniform(-0.6, 0.6))]
    stk = [rng.randrange(-128, 128), rng.randrange(-128, 128)]
    cam = [rng.randrange(-0x2000, 0x2000), rng.randrange(0, 0x10000)]
    stick[0] = rng.choice([0, 1, 1])
    stick[1] = rng.randrange(0, 0x10000)
    stick[2] = 1.0
    aim = rng.randrange(0, 0x10000)
    m.put(TASK + 4, "H", flag)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    m.put(PW, "B", 0)
    m.put(PW + 0x88, "I", attr)
    put3(PW + 0x7C, n)
    m.put(0x801E5438 + 0x1C, "h", stk[0])
    m.put(0x801E5438 + 0x1E, "h", stk[1])
    m.put(CAM + 0xC, "i", cam[0])
    m.put(CAM + 0x10, "i", cam[1])
    m.put(MW + 0x1C, "i", aim)
    log.clear()
    m.call(0x80048490, r3=TASK, r4=MW, r5=PW)
    steer.append({"flag": flag, "ang": ang, "attr": attr, "n": [fb(x) for x in n], "stk": stk, "cam": cam,
                  "stick": [stick[0], stick[1]], "aim": aim, "aim_after": m.get(MW + 0x1C, "i"), "log": list(log)})
# fn_80036CE4 with fn_80048490's callees and fn_80048834 stubbed and logged.
m.stub(0x80048834, lambda mm: log.append(["accel43", mm.get(TASK + 0xC, "i"), get3(MW + 0xC)]))
TGT = 0x81105000
jump = []
import math
for _ in range(3000):
    yaw = rng.randrange(0, 0x10000)
    ang = [0, yaw, 0]
    pos = [f32(rng.uniform(-50, 50)), f32(rng.uniform(0, 80)), f32(rng.uniform(-50, 50))]
    th = yaw * math.pi / 0x8000
    sp = rng.uniform(0, 4)
    mwspd = [f32(sp * math.cos(th) + rng.uniform(-0.5, 0.5)), rf(3), f32(sp * math.sin(th) + rng.uniform(-0.5, 0.5))]
    mwacc = [rf(0.2) for _ in range(3)]
    tg = []
    for _ in range(rng.choice([0, 1, 2, 3, 5])):
        a = th + rng.uniform(-1.2, 1.2)
        d = rng.uniform(2, 120)
        tg.append([f32(pos[0] + d * math.cos(a)), f32(pos[1] - rng.uniform(-10, 60)), f32(pos[2] + d * math.sin(a)),
                   f32(rng.uniform(5, 150)), f32(rng.choice([rng.uniform(-1, 5), 1.0])),
                   rf(9), rng.randrange(-5, 5), rng.randrange(-5, 5)])
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0x110, -0.05, 0), (0x118, -0.05, 0), (0x11C, -0.05, 0),
                                                       (0x138, 0.03, 0.12), (0xCC, 1, 8), (0xE8, 0, 6), (0xFC, 0.02, 1))}
    stick[0] = rng.choice([0, 1, 1, 1])
    stick[1] = (yaw + rng.randrange(-0x2400, 0x2400)) & 0xFFFF
    aim = rng.randrange(0, 0x10000)
    m.put(TASK + 4, "H", 0)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(TASK + 0x14, pos)
    put3(MW, mwspd)
    put3(MW + 0xC, mwacc)
    m.put(MW + 0x1C, "i", aim)
    m.put(PW + 0x88, "I", 0)
    for o, x in f.items():
        m.put(PW + o, "f", x)
    m.put(PW + 0x1E, "h", len(tg))
    m.put(PW + 0xB4, "I", TGT)
    for k, t in enumerate(tg):
        m.u.mem_write(TGT + 0x20 * k, struct.pack(">6f2i", *t))
    log.clear()
    m.call(0x80036CE4, r3=TASK, r4=MW, r5=PW)
    jump.append({"pos": [fb(x) for x in pos], "yaw": yaw, "mwspd": [fb(x) for x in mwspd], "mwacc": [fb(x) for x in mwacc],
                 "f": {hex(o): fb(x) for o, x in f.items()}, "stick": [stick[0], stick[1]], "aim": aim,
                 "tg": [[fb(x) for x in t[:6]] + t[6:] for t in tg], "ret": m.gpr(3),
                 "tg_after": [list(struct.unpack(">6I2i", m.u.mem_read(TGT + 0x20 * k, 0x20))) for k in range(len(tg))],
                 "yaw_after": m.get(TASK + 0xC, "i"), "aim_after": m.get(MW + 0x1C, "i"), "acc_after": get3(MW + 0xC),
                 "log": list(log)})
# fn_8004E490, fn_80048400, fn_80048370 (with fn_80047220) and fn_8004B168.
touch = [0]
m.stub(0x800060EC, lambda mm: (log.append(["touch", mm.gpr(4)]), mm.gpr(3, touch[0])))
m.put(0x801E7788, "I", 0x81106000)
small = []
for k in range(8000):
    fn = [0x8004E490, 0x80048400, 0x80048370, 0x8004B168][k % 4]
    flag = rng.choice([0, 1, 2, 0x2000, 0x4000, 0, 1])
    ang = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(0, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
    mode2 = rng.choice([0, 3, 1])
    spd = [rf(6) for _ in range(3)]
    mwacc = [rf(1) for _ in range(3)]
    f = {o: f32(rng.uniform(lo, hi)) for o, lo, hi in ((0x100, 0, 0.2), (0x10C, 0, 1), (0x110, -0.1, 0), (0x118, -0.1, 0),
                                                       (0x11C, -0.1, 0), (0xE8, 1, 5))}
    f[0x34] = f32(rng.choice([0.4, rng.uniform(-1, 1)]))
    stick[0] = rng.choice([0, 1, 1])
    stick[1] = rng.choice([ang[1], (ang[1] + rng.randrange(-0x1800, 0x1800)) & 0xFFFF, rng.randrange(0, 0x10000)])
    stick[2] = f32(rng.uniform(0, 1))
    touch[0] = rng.choice([0, 0, 0x81107000])
    aim = rng.randrange(0, 0x10000)
    m.put(TASK + 4, "H", flag)
    m.put(TASK + 2, "B", mode2)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *ang))
    put3(PW + 0x64, spd)
    for o, x in f.items():
        m.put(PW + o, "f", x)
    put3(MW + 0xC, mwacc)
    m.put(MW + 0x1C, "i", aim)
    put3(PW + 0x58, [1.5, 2.5, 3.5])
    log.clear()
    m.call(fn, r3=TASK, r4=PW if fn == 0x8004E490 else MW, r5=PW)
    small.append({"fn": fn, "flag": flag, "ang": ang, "mode2": mode2, "spd": [fb(x) for x in spd], "mwacc": [fb(x) for x in mwacc],
                  "f": {hex(o): fb(x) for o, x in f.items()}, "stick": [stick[0], stick[1], fb(stick[2])], "touch": touch[0] != 0,
                  "aim": aim, "ret": m.gpr(3), "ang_after": [m.get(TASK + 8 + 4 * j, "i") for j in range(3)],
                  "spd_after": get3(PW + 0x64), "acc_after": get3(PW + 0x58), "aim_after": m.get(MW + 0x1C, "i"), "log": list(log)})
print(json.dumps({"level": level, "acc": acc, "run": run, "air": air, "grav": grav, "m43": m43, "steer": steer, "jump": jump,
                  "small": small}))
