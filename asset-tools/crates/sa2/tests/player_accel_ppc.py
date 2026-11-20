"""Local PowerPC oracle for fn_8004B568 and fn_8004BA74 (ground acceleration)
and njRotate in main.dol.
Prints JSON; tests/player_accel_dol.rs compares.
Usage: python player_accel_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
T, M, P = 0x81000000, 0x81000100, 0x81000200
STICK, STAGE, GRAV, FRAME, PHYS = 0x801E53F8, 0x803AD820, 0x801E5624, 0x801CC1E4, 0x80196578
rng = random.Random(58)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))


def unit(scale=1.0):
    v = [rng.uniform(-1, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(scale * x / n) for x in v]


# The game's own rows (Sonic, Shadow, ...): +0x04..+0x84 of each 0x84 entry.
rows = [[m.get(PHYS + 0x84 * r + 4 + 4 * k, 'I') for k in range(32)] for r in range(4)]
cases = []
for _ in range(3000):
    phys = list(rng.choice(rows))
    if rng.random() < 0.3:
        phys = [fb(f32(rng.uniform(-1, 1) * abs(bf(w)) * 2)) if rng.random() < 0.3 else w for w in phys]
    jog, run, rush = (bf(phys[k]) for k in (8, 9, 10))
    p = rng.choice([0, 1])
    status = rng.choice([0, 1, 3, 3, 0x2003, rng.randrange(0x10000)])
    status = status - 0x10000 if status & 0x8000 else status
    angles = [rng.choice([0, rng.randrange(-0x4000, 0x4000), rng.randrange(-0x10000, 0x10000)]) for _ in range(3)]
    nocon = rng.choice([0, 0, 0, 2]); confuse = rng.choice([0, 0, 0, 3])
    sx = rng.choice([0.0, -0.0, f32(rng.uniform(-3, 3)), jog, run, rush, f32(rng.uniform(0, 1.2 * rush)),
                     f32(0.5 * (jog + run))])
    speed = [sx] + [rng.choice([0.0, f32(rng.uniform(-3, 3))]) for _ in range(2)]
    base = [rng.choice([0.0, f32(rng.uniform(-0.2, 0.2))]) for _ in range(3)]
    aim = rng.randrange(-0x10000, 0x10000)
    gravity = rng.choice([[0.0, -1.0, 0.0], unit(), [f32(0.3), f32(-0.95), 0.0]])
    stage = rng.choice([13, 0x2c, 3])
    up = rng.choice([0.0, 1.0, f32(rng.uniform(-1, 1))]); scale = rng.choice([1.0, 0.5, f32(rng.uniform(0, 1.5))])
    mag = rng.choice([0.0, 1.0, f32(rng.uniform(0, 1))])
    stick = rng.choice([angles[1] & 0xFFFF, rng.randrange(0x10000), (angles[1] + 0x1800) & 0xFFFF])
    frame = rng.randrange(1 << 32)
    m.u.mem_write(T, bytes(0x40)); m.u.mem_write(M, bytes(0x40)); m.u.mem_write(P, bytes(0x150))
    m.put(T + 4, 'h', status)
    for i, a in enumerate(angles): m.put(T + 8 + 4 * i, 'i', a)
    for i, x in enumerate(base): m.put(M + 0xC + 4 * i, 'f', x)
    m.put(M + 0x1C, 'i', aim)
    m.put(P, 'b', p); m.put(P + 0x14, 'h', nocon); m.put(P + 0x1A, 'h', confuse)
    m.put(P + 0x34, 'f', up); m.put(P + 0x40, 'f', scale)
    for i, x in enumerate(speed): m.put(P + 0x64 + 4 * i, 'f', x)
    for k, w in enumerate(phys): m.put(P + 0xC4 + 4 * k, 'I', w)
    for i, x in enumerate(gravity): m.put(GRAV + 4 * i, 'f', x)
    m.put(STAGE, 'h', stage); m.put(FRAME, 'I', frame)
    m.put(STICK + 8 * p, 'i', stick); m.put(STICK + 8 * p + 4, 'f', mag)
    m.call(0x8004B568, r3=T, r4=M, r5=P)
    cases.append({'status': status, 'angles': angles, 'nocon': nocon, 'confuse': confuse,
                  'speed': [fb(x) for x in speed], 'base': [fb(x) for x in base], 'aim': aim, 'phys': phys,
                  'gravity': [fb(x) for x in gravity], 'stage': stage, 'up': fb(up), 'scale': fb(scale),
                  'magnitude': fb(mag), 'stick': stick, 'frame': frame,
                  'outAngles': [m.get(T + 8 + 4 * i, 'i') for i in range(3)],
                  'outSpeed': [m.get(P + 0x64 + 4 * i, 'I') for i in range(3)],
                  'outAccel': [m.get(P + 0x58 + 4 * i, 'I') for i in range(3)],
                  'outExcess': m.get(P + 0x38, 'I'), 'outPush': m.get(P + 0x3C, 'I'),
                  'outAim': m.get(M + 0x1C, 'i')})

# njRotate about an arbitrary axis (PSMTXRotAxisRad + PSVECNormalize).
CUR = 0x81000800
rot = []
for _ in range(800):
    axis = rng.choice([unit(), [f32(rng.uniform(-5, 5)) for _ in range(3)], [0.0, 1.0, 0.0]])
    ang = rng.choice([rng.randrange(-0x10000, 0x10000), rng.randrange(-0x80000000, 0x80000000), 0x4000])
    m0 = [f32(rng.uniform(-2, 2)) for _ in range(12)]
    for k, x in enumerate(m0): m.put(CUR + 4 * k, 'f', x)
    for k, x in enumerate(axis): m.put(CUR + 0x40 + 4 * k, 'f', x)
    m.call(0x80115218, r3=CUR, r4=CUR + 0x40, r5=ang & 0xFFFFFFFF)
    rot.append({'m': [fb(x) for x in m0], 'axis': [fb(x) for x in axis], 'angle': ang,
                'out': [m.get(CUR + 4 * k, 'I') for k in range(12)]})

RAW, CAMS, CAM, H, PTS, CS = 0x801E5438, 0x80175378, 0x81000900, 0x81000A00, 0x81000B00, 0x81000D00


def make_path():
    kind = rng.choice([1, 2]); count = rng.randrange(2, 7)
    pos = [[f32(rng.uniform(-100, 100)) for _ in range(3)]]
    for _ in range(count - 1):
        pos.append([f32(pos[-1][k] + rng.uniform(-60, 60)) for k in range(3)])
    recs, total = [], 0.0
    for i in range(count):
        j = (i + 1) % count
        ln = f32(sum((pos[j][k] - pos[i][k]) ** 2 for k in range(3)) ** 0.5)
        total = f32(total + ln)
        if kind == 1:
            recs.append([rng.randrange(-0x8000, 0x8000), rng.randrange(-0x8000, 0x8000), fb(ln), [fb(x) for x in pos[i]]])
        else:
            recs.append([[fb(x) for x in unit()], fb(ln), [fb(x) for x in pos[i]]])
    flags = (0x8000 if rng.random() < 0.5 else 0) | kind
    flags = flags - 0x10000 if flags & 0x8000 else flags
    return {'flags': flags, 'count': count, 'total': fb(total), 'kind': kind, 'records': recs}, pos


def write_path(pt):
    m.put(H, 'h', pt['flags']); m.put(H + 2, 'h', pt['count']); m.put(H + 4, 'I', pt['total']); m.put(H + 8, 'I', PTS)
    stride = 0x14 if pt['kind'] == 1 else 0x1c
    for i, r in enumerate(pt['records']):
        a = PTS + stride * i
        if pt['kind'] == 1:
            m.put(a, 'h', r[0]); m.put(a + 2, 'h', r[1]); m.put(a + 4, 'I', r[2])
            for k in range(3): m.put(a + 8 + 4 * k, 'I', r[3][k])
        else:
            for k in range(3): m.put(a + 4 * k, 'I', r[0][k])
            m.put(a + 12, 'I', r[1])
            for k in range(3): m.put(a + 16 + 4 * k, 'I', r[2][k])


TP, OBJ = 0x81000E00, 0x81001000


def ground_case(fn):
    phys = list(rng.choice(rows))
    if rng.random() < 0.3:
        phys = [fb(f32(rng.uniform(-1, 1) * abs(bf(w)) * 2)) if rng.random() < 0.3 else w for w in phys]
    jog, run, rush, maxx = (bf(phys[k]) for k in (8, 9, 10, 3))
    p = rng.choice([0, 1])
    status = rng.choice([0, 1, 3, 3, 0x2003, 0x2001, 0x103, rng.randrange(0x10000)])
    status = status - 0x10000 if status & 0x8000 else status
    angles = [rng.choice([0, rng.randrange(-0x4000, 0x4000), rng.randrange(-0x10000, 0x10000)]) for _ in range(3)]
    nocon = rng.choice([0, 0, 0, 2]); confuse = rng.choice([0, 0, 0, 3])
    sx = rng.choice([0.0, f32(rng.uniform(-3, 3)), jog, run, rush, maxx, f32(rng.uniform(0, 1.5 * maxx)),
                     f32(0.4 * jog), f32(rng.uniform(0, jog))])
    speed = [sx] + [rng.choice([0.0, f32(rng.uniform(-3, 3))]) for _ in range(2)]
    base = [rng.choice([0.0, f32(rng.uniform(-0.2, 0.2))]) for _ in range(3)]
    motion = rng.choice([unit(rng.uniform(0, 6)), [f32(rng.uniform(-6, 6)) for _ in range(3)]])
    aim = rng.randrange(-0x10000, 0x10000)
    gravity = rng.choice([[0.0, -1.0, 0.0], unit(), [f32(0.3), f32(-0.95), 0.0]])
    down = rng.choice([[0.0, -1.0, 0.0], gravity, unit()])
    stage = rng.choice([13, 0x2c, 0x22, 3])
    up = rng.choice([0.0, 1.0, 0.97, f32(rng.uniform(-1, 1))]); scale = rng.choice([1.0, 0.5, f32(rng.uniform(0, 1.5))])
    mag = rng.choice([0.0, 1.0, 0.65, 0.3, f32(rng.uniform(0, 1))])
    stick = rng.choice([angles[1] & 0xFFFF, rng.randrange(0x10000), (angles[1] + 0x1800) & 0xFFFF])
    raw = [rng.randrange(-128, 128), rng.randrange(-128, 128)]
    cam = [f32(rng.uniform(-300, 300)) for _ in range(3)]
    surface = rng.choice([0, 4, 0x80000, 0x80004, rng.randrange(1 << 32)])
    if fn == 0x8004C734 and rng.random() < 0.9:
        surface &= ~0x100100
    character = rng.choice([0, 1, 5, 6, 7])
    wall = rng.choice([[0.0, 0.0, 0.0], unit(), unit()])
    sharp = rng.choice([0, 1])
    frame = rng.randrange(1 << 32)
    pt, pos = make_path(); write_path(pt)
    tpos = [f32(rng.choice(pos)[k] + rng.uniform(-20, 20)) for k in range(3)]
    normal = rng.choice([[0.0, 1.0, 0.0], unit()])
    path_dist = f32(rng.uniform(0, 300))
    m.u.mem_write(T, bytes(0x40)); m.u.mem_write(M, bytes(0x40)); m.u.mem_write(P, bytes(0x150))
    m.u.mem_write(CAM, bytes(0x40)); m.u.mem_write(CS, bytes(0x40))
    m.put(T + 4, 'h', status)
    for i, a in enumerate(angles): m.put(T + 8 + 4 * i, 'i', a)
    for i, x in enumerate(tpos): m.put(T + 0x14 + 4 * i, 'f', x)
    for i, x in enumerate(motion): m.put(M + 4 * i, 'f', x)
    for i, x in enumerate(base): m.put(M + 0xC + 4 * i, 'f', x)
    m.put(M + 0x1C, 'i', aim)
    m.put(P, 'b', p); m.put(P + 0x14, 'h', nocon); m.put(P + 0x1A, 'h', confuse)
    m.put(P + 0x30, 'f', path_dist); m.put(P + 0x34, 'f', up); m.put(P + 0x40, 'f', scale)
    for i, x in enumerate(speed): m.put(P + 0x64 + 4 * i, 'f', x)
    for i, x in enumerate(down): m.put(P + 0x7C + 4 * i, 'f', x)
    m.put(P + 0x88, 'I', surface); m.put(P + 0x90, 'I', CS); m.put(P + 0xB8, 'I', H)
    for i, x in enumerate(normal): m.put(CS + 0x1C + 4 * i, 'f', x)
    for k, w in enumerate(phys): m.put(P + 0xC4 + 4 * k, 'I', w)
    for i, x in enumerate(gravity): m.put(GRAV + 4 * i, 'f', x)
    m.put(STAGE, 'h', stage); m.put(FRAME, 'I', frame)
    m.put(STICK + 8 * p, 'i', stick); m.put(STICK + 8 * p + 4, 'f', mag)
    m.put(RAW + 0x34 * p + 0x1C, 'h', raw[0]); m.put(RAW + 0x34 * p + 0x1E, 'h', raw[1])
    m.put(CAMS + 4 * p, 'I', CAM)
    for i, x in enumerate(cam): m.put(CAM + 0x18 + 4 * i, 'f', x)
    m.put(P + 1, 'b', character)
    for i, x in enumerate(wall): m.put(P + 0x70 + 4 * i, 'f', x)
    m.put(0x801E7788 + 4 * p, 'I', TP); m.put(TP + 0x40, 'I', OBJ); m.put(OBJ + 0x35C, 'B', sharp)
    m.call(fn, r3=T, r4=M, r5=P)
    return {'character': character, 'wall': [fb(x) for x in wall], 'sharp': sharp,
            'status': status, 'angles': angles, 'pos': [fb(x) for x in tpos], 'nocon': nocon,
                    'confuse': confuse, 'speed': [fb(x) for x in speed], 'base': [fb(x) for x in base],
                    'motion': [fb(x) for x in motion], 'aim': aim, 'phys': phys,
                    'gravity': [fb(x) for x in gravity], 'down': [fb(x) for x in down], 'stage': stage,
                    'up': fb(up), 'scale': fb(scale), 'magnitude': fb(mag), 'stick': stick, 'frame': frame,
                    'raw': raw, 'camera': [fb(x) for x in cam], 'surface': surface, 'path': pt,
                    'normal': [fb(x) for x in normal], 'pathDist': fb(path_dist),
                    'outStatus': m.get(T + 4, 'h'),
                    'outAngles': [m.get(T + 8 + 4 * i, 'i') for i in range(3)],
                    'outSpeed': [m.get(P + 0x64 + 4 * i, 'I') for i in range(3)],
                    'outAccel': [m.get(P + 0x58 + 4 * i, 'I') for i in range(3)],
                    'outExcess': m.get(P + 0x38, 'I'), 'outPush': m.get(P + 0x3C, 'I'),
                    'outPathDist': m.get(P + 0x30, 'I'), 'outAim': m.get(M + 0x1C, 'i')}


steered = [ground_case(0x8004BA74) for _ in range(3000)]
moves = [ground_case(0x8004C734) for _ in range(6000)]
print(json.dumps({'plain': cases, 'rotate': rot, 'steered': steered, 'move': moves}))
