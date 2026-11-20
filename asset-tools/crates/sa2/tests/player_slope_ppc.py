"""Local PowerPC oracle for njScalor, njUnitVector, fn_80047CBC and
fn_80048150 (turn to face down a slope) in main.dol.

Prints JSON; tests/player_slope_dol.rs compares.
Usage: python player_slope_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
T, M, P, V = 0x81000000, 0x81000100, 0x81000200, 0x81000400
STICK, STAGE, GRAV = 0x801E53F8, 0x803AD820, 0x801E5624
rng = random.Random(33)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))


def unit(scale=1.0):
    v = [rng.uniform(-1, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(scale * x / n) for x in v]


vectors = []
for _ in range(1500):
    v = rng.choice([unit(rng.uniform(0, 10)), [f32(rng.uniform(-50, 50)) for _ in range(3)], [0.0, 0.0, 0.0]])
    m.u.mem_write(V, struct.pack('>3f', *v))
    m.call(0x801151F4, r3=V); sc = fb(m.fpr(1))
    m.call(0x80115D24, r3=V); ret = fb(m.fpr(1))
    vectors.append([[fb(x) for x in v], sc, [m.get(V + 4 * k, 'I') for k in range(3)], ret])

cases = []
for _ in range(2400):
    fn = rng.choice([0x80047CBC, 0x80048150])
    p = rng.choice([0, 1])
    status = rng.choice([0, 1, 3, 0x101, 0x103, 0x100, rng.randrange(0x10000)])
    status = status - 0x10000 if status & 0x8000 else status
    angles = [rng.choice([0, rng.randrange(-0x4000, 0x4000), rng.randrange(-0x10000, 0x10000)]) for _ in range(3)]
    nocon = rng.choice([0, 0, 0, 2]); confuse = rng.choice([0, 0, 3])
    surface = rng.choice([0, 0, 0x100, rng.randrange(1 << 32)])
    speed = [f32(rng.uniform(-6, 6)) for _ in range(3)]
    jog = rng.choice([f32(0.46), f32(rng.uniform(0, 3))])
    motion = rng.choice([unit(rng.uniform(0, 6)), [f32(rng.uniform(-6, 6)) for _ in range(3)]])
    aim = rng.randrange(-0x10000, 0x10000)
    gravity = rng.choice([[0.0, -1.0, 0.0], unit(), [f32(0.3), f32(-0.95), 0.0]])
    stage = rng.choice([13, 0x22, 0x22, 3])
    up = rng.choice([0.0, 1.0, f32(rng.uniform(-1, 1))]); scale = rng.choice([1.0, 0.5, f32(rng.uniform(0, 1.5))])
    mag = rng.choice([0.0, 1.0, f32(rng.uniform(0, 1))])
    m.u.mem_write(T, bytes(0x40)); m.u.mem_write(M, bytes(0x40)); m.u.mem_write(P, bytes(0x150))
    m.put(T + 4, 'h', status)
    for i, a in enumerate(angles): m.put(T + 8 + 4 * i, 'i', a)
    for i, x in enumerate(motion): m.put(M + 4 * i, 'f', x)
    m.put(M + 0x1C, 'i', aim)
    m.put(P, 'b', p); m.put(P + 0x14, 'h', nocon); m.put(P + 0x1A, 'h', confuse)
    m.put(P + 0x34, 'f', up); m.put(P + 0x40, 'f', scale)
    for i, x in enumerate(speed): m.put(P + 0x64 + 4 * i, 'f', x)
    m.put(P + 0x88, 'I', surface); m.put(P + 0xE4, 'f', jog)
    for i, x in enumerate(gravity): m.put(GRAV + 4 * i, 'f', x)
    m.put(STAGE, 'h', stage); m.put(STICK + 8 * p + 4, 'f', mag)
    m.call(fn, r3=T, r4=M, r5=P)
    cases.append({'fn': fn, 'status': status, 'angles': angles, 'nocon': nocon, 'confuse': confuse,
                  'surface': surface, 'speed': [fb(x) for x in speed], 'jog': fb(jog),
                  'motion': [fb(x) for x in motion], 'aim': aim, 'gravity': [fb(x) for x in gravity],
                  'stage': stage, 'up': fb(up), 'scale': fb(scale), 'magnitude': fb(mag),
                  'outAngles': [m.get(T + 8 + 4 * i, 'i') for i in range(3)],
                  'outSpeed': [m.get(P + 0x64 + 4 * i, 'I') for i in range(3)],
                  'outAim': m.get(M + 0x1C, 'i')})
print(json.dumps({'vectors': vectors, 'cases': cases}))
