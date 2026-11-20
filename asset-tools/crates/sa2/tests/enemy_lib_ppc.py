"""Run main.dol's shared enemy helpers (and stg09D.rel's sqrtf) on random
inputs and print JSON with every float as its bit pattern.
tests/enemy_lib.rs replays the cases with `enemy_lib` and compares.

Needs SA2_MAIN_DOL (GSNE8P main.dol) and SA2_STAGE_REL (the GameCube
stg09D.rel from the same disc), both local files that are never committed.
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402
from stage_rel_ppc import load_rel  # noqa: E402

dol = Dol(os.environ["SA2_MAIN_DOL"])
m = dol.machine()
rel = load_rel(m, os.environ["SA2_STAGE_REL"])
rng = random.Random(9)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))

TWP, WORK, VEC, VEC2 = 0x81000100, 0x81000200, 0x81000500, 0x81000510
PLAYERS = [0x81000600, 0x81000700]
PLAYERTWP = 0x801E7768
GAME = 0x801CC168
NEXT = 0x803AD6A0


def rnd(lo=-3000.0, hi=3000.0):
    k = rng.random()
    if k < 0.05:
        return rng.choice([0.0, -0.0, 1.0, -1.0, 0.5])
    return f32(rng.uniform(lo, hi))


def ang():
    return rng.choice([0, 0x4000, -0x4000, 0x8000, 0xffff, 0x10000, rng.randrange(-0x20000, 0x20000)])


def put_v(a, v):
    m.u.mem_write(a, struct.pack('>3f', *v))


def get_v(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def ret_f():
    return fb(f32(m.fpr(1)))


out = {k: [] for k in ('diff', 'adjust', 'fraction', 'sqrt', 'distance', 'approach', 'scalor', 'unit',
                       'rand', 'leash', 'face', 'notice', 'nearest', 'home_offset', 'motion')}

for _ in range(500):
    a, b, s = ang(), ang(), rng.choice([0x80, 0x100, 0x800, 0x1800, rng.randrange(0, 0x9000)])
    m.call(0x8002B8E0, r3=a & 0xffffffff, r4=b & 0xffffffff)
    out['diff'].append([a, b, m.gpr(3)])
    m.call(0x8002B918, r3=a & 0xffffffff, r4=b & 0xffffffff, r5=s)
    out['adjust'].append([a, b, s, m.gpr(3)])

for _ in range(500):
    x = rng.choice([rnd(), rnd(-3, 3), f32(rng.uniform(-1e8, 1e8)), bf(rng.randrange(1 << 32))])
    m.call(0x8011217C, f1=x)
    fr = ret_f()
    m.call(0x80112320, f1=x)
    out['fraction'].append([fb(x), fr, ret_f()])

for _ in range(500):
    x = rng.choice([abs(rnd()), f32(rng.uniform(0, 1e7)), f32(rng.uniform(0, 1)), -1.0, 0.0])
    m.call(0x8002F15C, f1=x)
    d = ret_f()
    m.call(rel.addr(1, 0xBEDC), f1=x)
    out['sqrt'].append([fb(x), d, ret_f()])

for _ in range(500):
    a = [rnd() for _ in range(3)]
    b = [x + rng.choice([0.0, rnd(-0.1, 0.1), rnd(-50, 50)]) for x in a]
    b = [f32(x) for x in b]
    put_v(VEC, a), put_v(VEC2, b)
    m.call(0x8011B65C, r3=VEC, r4=VEC2)
    out['distance'].append([[fb(x) for x in a], [fb(x) for x in b], ret_f()])
    v, t, st = rnd(-50, 50), rnd(-50, 50), abs(rnd(0, 5))
    m.call(0x8002AE5C, f1=v, f2=t, f3=st)
    out['approach'].append([fb(v), fb(t), fb(st), ret_f()])
    put_v(VEC, a)
    m.call(0x801151F4, r3=VEC)
    out['scalor'].append([[fb(x) for x in a], ret_f()])
    m.call(0x80115D24, r3=VEC)
    out['unit'].append([[fb(x) for x in a], get_v(VEC)])

for _ in range(200):
    s = rng.randrange(1 << 32)
    m.put(NEXT, 'I', s)
    m.call(0x8010DC88)
    out['rand'].append([s, m.gpr(3), m.get(NEXT, 'I')])


def put_task(pos, a):
    m.u.mem_write(TWP, bytes(0x40))
    for i, x in enumerate(a):
        m.put(TWP + 8 + 4 * i, 'i', x)
    put_v(TWP + 0x14, pos)


for _ in range(400):
    pos = [rnd(-500, 500) for _ in range(3)]
    home = [f32(p + rnd(-200, 200)) for p in pos]
    rng_ = abs(rnd(1, 300))
    put_task(pos, [0, ang(), 0])
    m.u.mem_write(WORK, bytes(0x210))
    put_v(WORK + 0x58, home)
    m.call(0x80017918, r3=TWP, r4=WORK, f1=rng_)
    out['leash'].append([[fb(x) for x in pos], [fb(x) for x in home], fb(rng_), get_v(TWP + 0x14)])

for _ in range(400):
    pos = [rnd(-500, 500) for _ in range(3)]
    a = [ang(), ang(), ang()]
    put_task(pos, a)
    m.u.mem_write(WORK, bytes(0x210))
    rate = rng.choice([0x100, 0x800, 0x71c7, rng.randrange(0, 0x4000)])
    m.put(WORK + 0x1C8, 'i', rate)
    pls = [[f32(p + rnd(-300, 300)) for p in pos] for _ in range(2)]
    for i in range(2):
        m.u.mem_write(PLAYERS[i], bytes(0x40))
        put_v(PLAYERS[i] + 0x14, pls[i])
        m.put(PLAYERTWP + 4 * i, 'I', PLAYERS[i])
    idx = rng.randrange(2)
    m.call(0x800176AC, r3=TWP, r4=WORK, r5=idx)
    out['face'].append([[fb(x) for x in pos], a, rate, [fb(x) for x in pls[idx]], m.gpr(3),
                        m.get(TWP + 0xC, 'i'), m.get(WORK + 0x1D0, 'i')])

for _ in range(600):
    pos = [rnd(-500, 500) for _ in range(3)]
    home = [f32(p + rnd(-100, 100)) for p in pos]
    off = [rnd(-100, 100), 0.0, rnd(-100, 100)]
    r2 = f32(rng.choice([1225.0, 3600.0, (60 + rng.randrange(0, 200)) ** 2, rnd(0, 40000)]))
    put_task(pos, [0, 0, 0])
    m.u.mem_write(WORK, bytes(0x210))
    put_v(WORK + 0x58, home), put_v(WORK + 0x70, off)
    m.put(WORK + 0x1DC, 'f', r2)
    present = [rng.random() < 0.8, rng.random() < 0.5]
    pls = [[f32(p + rnd(-150, 150)) for p in pos] for _ in range(2)]
    for i in range(2):
        put_v(PLAYERS[i] + 0x14, pls[i])
        m.put(PLAYERTWP + 4 * i, 'I', PLAYERS[i] if present[i] else 0)
    g39, g42 = rng.choice([0, 0, 0, 1, 2]), rng.choice([0, 0, 0, 1])
    m.put(GAME + 0x39, 'B', g39), m.put(GAME + 0x42, 'B', g42)
    res = []
    for fn in (0x800187C4, 0x800188B0, 0x8001897C):
        m.call(fn, r3=TWP, r4=WORK)
        res.append(m.gpr(3))
    out['notice'].append([[fb(x) for x in pos], [fb(x) for x in home], [fb(x) for x in off], fb(r2),
                          [[fb(x) for x in p] if present[i] else None for i, p in enumerate(pls)], g39, g42, res])
    put_v(VEC, pos)
    for i in range(2, 8):
        m.put(PLAYERTWP + 4 * i, 'I', 0)
    m.call(0x80037C84, r3=VEC)
    out['nearest'].append([[fb(x) for x in pos], [[fb(x) for x in p] if present[i] else None for i, p in enumerate(pls)],
                           m.gpr(3)])
m.put(GAME + 0x39, 'B', 0), m.put(GAME + 0x42, 'B', 0)

for _ in range(400):
    a = [0, ang(), 0]
    scl = [rnd(-10, 10), rnd(-10, 10), rng.choice([rnd(-10, 10), f32(rng.uniform(-200, 200)), 30.25, -4.5])]
    put_task([0.0, 0.0, 0.0], a)
    put_v(TWP + 0x20, scl)
    m.u.mem_write(WORK, bytes(0x210))
    m.call(0x80015B18, r3=TWP, r4=WORK)
    out['home_offset'].append([a[1], [fb(x) for x in scl], get_v(WORK + 0x70), m.get(TWP + 0x28, 'I')])

# The motion sequencer over E AI's table (stg09D.rel section 5 +0xC3EC8).
TABLE = rel.addr(5, 0xC3EC8)
MB = WORK + 0x1E0
sec5 = rel.bases[5]
ptr_id = lambda p: (p - sec5) if p else None
for run in range(60):
    m.u.mem_write(WORK, bytes(0x210))
    m.put(MB + 0x18, 'I', TABLE)
    m.put(MB + 0x13, 'b', rng.randrange(17))
    m.put(MB + 0xC, 'f', f32(rng.choice([1.0, 0.5, rng.uniform(0, 2)])))
    frames = []
    for f in range(200):
        if rng.random() < 0.05:
            m.put(MB + 0x13, 'b', rng.randrange(17))
        before = m.u.mem_read(MB, 0x30)
        m.call(0x80015C40, r3=MB)
        after = m.u.mem_read(MB, 0x30)
        frames.append([before.hex(), after.hex()])
    out['motion'].append(frames)
out['motion_base'] = sec5
print(json.dumps(out))
