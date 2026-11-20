"""Local PowerPC oracle for the angle helpers and fn_800473B4 in main.dol.

Prints JSON with randomized SubAngle/DiffAngle/AdjustAngle cases and
fn_800473B4 (turn toward a target yaw) runs. tests/player_turn_dol.rs
compares. Usage: python player_turn_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
T, P = 0x81000000, 0x81000100
rng = random.Random(21)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v


def word():
    return rng.choice([0, 1, -1, 0x4000, 0x7fff, 0x8000, 0xffff, 0x10000, rng.randrange(-0x20000, 0x20000),
                       rng.randrange(-(1 << 31), 1 << 31)])


angles = []
for _ in range(3000):
    a, b, step = word(), word(), rng.choice([0, 1, 0x100, 0x800, 0x8000, rng.randrange(-0x200, 0x10000), word()])
    res = []
    for fn, args in ((0x8002B904, (a, b)), (0x8002B8E0, (a, b)), (0x8002B918, (a, b, step))):
        regs = {f'r{3 + i}': v & 0xffffffff for i, v in enumerate(args)}
        m.call(fn, **regs)
        res.append(s32(m.gpr(3)))
    angles.append([a, b, step] + res)

turns = []
for _ in range(1500):
    ang = [rng.choice([0, 0x4000, rng.randrange(-0x10000, 0x10000)]) for _ in range(3)]
    spd = [f32(rng.choice([rng.uniform(-8, 8), rng.uniform(-1, 1), 0.0])) for _ in range(3)]
    dash = rng.choice([f32(5.09), f32(3.0), f32(rng.uniform(0, 8))])
    up = rng.choice([0.0, 1.0, f32(0.4), f32(0.4000001), f32(rng.uniform(-1, 1))])
    target = word()
    m.u.mem_write(T, bytes(0x40)); m.u.mem_write(P, bytes(0x150))
    for i, a in enumerate(ang): m.put(T + 8 + 4 * i, 'i', a)
    for i, v in enumerate(spd): m.put(P + 0x64 + 4 * i, 'f', v)
    m.put(P + 0xF4, 'f', dash); m.put(P + 0x34, 'f', up)
    m.call(0x800473B4, r3=T, r4=0, r5=P, r6=target & 0xffffffff)
    turns.append({'angles': ang, 'speed': [fb(v) for v in spd], 'dash': fb(dash), 'up': fb(up), 'target': target,
                  'outAngles': [m.get(T + 8 + 4 * i, 'i') for i in range(3)],
                  'outSpeed': [m.get(P + 0x64 + 4 * i, 'I') for i in range(3)]})
print(json.dumps({'angles': angles, 'turns': turns}))
