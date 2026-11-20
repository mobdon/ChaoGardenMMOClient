"""Local PowerPC oracle for fn_8004E560 (analog stick input) in main.dol.

Fills the engine tables it reads, stubs the collision lookup fn_800060EC,
runs the original, and prints JSON. tests/player_analog_dol.rs compares.
Also checks njInnerProduct, njOuterProduct, acosf and VectorAngle.
Usage: python player_analog_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
T, P, OBJ, OBJW, OUT, VA, VB, VC = (0x81000000, 0x81000100, 0x81000200, 0x81000300,
                                     0x81000400, 0x81000500, 0x81000510, 0x81000520)
STICK, FRAME, STAGE, RAW, GRAV, PTASK = 0x801E53F8, 0x801CC1E4, 0x803AD820, 0x801E5438, 0x801E5624, 0x801E7788
rng = random.Random(9)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))
lookup = {}


def collision(mm):
    assert mm.gpr(4) == 0x11 and mm.gpr(3) == 0x81000F00 + lookup['player'] * 0x10
    mm.gpr(3, OBJ if lookup['object'] else 0)


m.stub(0x800060EC, collision)


def unit():
    v = [rng.uniform(-1, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(x / n) for x in v]


def ang():
    return rng.choice([0, 0x4000, -0x4000, 0x8000, rng.randrange(-0x10000, 0x10000), rng.randrange(-(1 << 31), 1 << 31)])


vec = []
for _ in range(1500):
    a, b = unit(), unit()
    if rng.random() < 0.05: b = a[:]
    if rng.random() < 0.05: b = [-x for x in a]
    m.u.mem_write(VA, struct.pack('>3f', *a)); m.u.mem_write(VB, struct.pack('>3f', *b))
    m.call(0x80114FD0, r3=VA, r4=VB); dot = fb(m.fpr(1))
    m.call(0x8011502C, r3=VA, r4=VB, r5=OUT); cross = [m.get(OUT + 4 * k, 'I') for k in range(3)]
    x = f32(rng.uniform(-1, 1))
    m.call(0x80111124, f1=x); ac = fb(m.fpr(1))
    vec.append([[fb(v) for v in a], [fb(v) for v in b], None, dot, cross, None, fb(x), ac])
va_cases = []
for _ in range(600):
    a, b, c = unit(), unit(), unit()
    if rng.random() < 0.05: b = a[:]
    if rng.random() < 0.05: b = [-x for x in a]
    use = rng.random() < 0.7
    m.u.mem_write(VA, struct.pack('>3f', *a)); m.u.mem_write(VB, struct.pack('>3f', *b)); m.u.mem_write(VC, struct.pack('>3f', *c))
    m.call(0x8002B7C4, r3=VA, r4=VB, r5=VC if use else 0)
    va_cases.append([[fb(v) for v in a], [fb(v) for v in b], [fb(v) for v in c] if use else None, m.gpr(3)])
# A NaN side dot (a zero motion vector normalised) must leave the angle alone.
for _ in range(20):
    a, b = unit(), unit()
    c = [float('nan'), 0.0, 0.0]
    m.u.mem_write(VA, struct.pack('>3f', *a)); m.u.mem_write(VB, struct.pack('>3f', *b)); m.u.mem_write(VC, struct.pack('>3f', *c))
    m.call(0x8002B7C4, r3=VA, r4=VB, r5=VC)
    va_cases.append([[fb(v) for v in a], [fb(v) for v in b], [fb(v) for v in c], m.gpr(3)])
analog = []
for _ in range(1500):
    p = rng.choice([0, 1])
    status = rng.choice([0, 0x2000, 0x4000, 0x6000, rng.randrange(0x10000)]) & 0xffff
    status = status - 0x10000 if status & 0x8000 else status
    angles = [ang() for _ in range(3)]
    nocon = rng.choice([0, 0, 0, 5, -3]); confuse = rng.choice([0, 0, 0, 10, -2])
    angle = ang(); mag = rng.choice([0.0, -0.0, 1.0, f32(rng.uniform(0, 1))])
    frame = rng.randrange(1 << 32); stage = rng.choice([13, 0x2c, 0x32, 3, 13])
    obj = rng.random() < 0.6; oang = [ang() for _ in range(3)]
    stick_y = rng.randrange(-128, 128); gy = rng.choice([-1.0, 0.0, 0.1, f32(0.1000001), 1.0])
    want_a, want_m = rng.random() < 0.85, rng.random() < 0.6
    lookup.update(player=p, object=obj)
    m.u.mem_write(T, bytes(0x40)); m.u.mem_write(P, bytes(0x40))
    m.put(T + 4, 'h', status)
    for i, a in enumerate(angles): m.put(T + 8 + 4 * i, 'i', a)
    m.put(P, 'b', p); m.put(P + 0x14, 'h', nocon); m.put(P + 0x1A, 'h', confuse)
    m.put(STICK + 8 * p, 'i', angle); m.put(STICK + 8 * p + 4, 'f', mag)
    m.put(FRAME, 'I', frame); m.put(STAGE, 'h', stage)
    m.put(RAW + 0x34 * p + 0x1E, 'h', stick_y); m.put(GRAV + 4, 'f', gy)
    m.put(PTASK + 4 * p, 'I', 0x81000F00 + p * 0x10)
    m.put(OBJ + 0x34, 'I', OBJW)
    for i, a in enumerate(oang): m.put(OBJW + 8 + 4 * i, 'i', a)
    m.put(OUT, 'I', 0xDEADBEEF); m.put(OUT + 4, 'I', 0xDEADBEEF)
    m.call(0x8004E560, r3=T, r4=P, r5=OUT if want_a else 0, r6=OUT + 4 if want_m else 0)
    analog.append({'status': status, 'angles': angles, 'nocon': nocon, 'confuse': confuse, 'angle': angle,
                   'magnitude': fb(mag), 'frame': frame, 'stage': stage, 'object': oang if obj else None,
                   'stickY': stick_y, 'gravityY': fb(gy), 'wantAngle': want_a, 'wantMagnitude': want_m,
                   'ret': m.gpr(3), 'outAngle': m.get(OUT, 'i'), 'outMagnitude': m.get(OUT + 4, 'I')})
print(json.dumps({'vec': vec, 'vectorAngle': va_cases, 'analog': analog}))
