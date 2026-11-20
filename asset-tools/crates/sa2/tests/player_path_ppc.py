"""Local PowerPC oracle for the path-following code in main.dol: floorf,
fn_8002B770, fn_8011B51C, fn_800CE5E8, fn_8005C460, fn_8005CDCC,
fn_8005C910 and fn_80038F40. Prints JSON; tests/player_path_dol.rs compares.
Usage: python player_path_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
H, PTS, A, B, C, OUT, INFO = 0x81000000, 0x81000100, 0x81002000, 0x81002010, 0x81002020, 0x81002040, 0x81002100
T, MW, P, CS = 0x81003000, 0x81003100, 0x81003200, 0x81003400
STICK = 0x801E53F8
rng = random.Random(77)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))
s16 = lambda v: (v + 0x8000) % 0x10000 - 0x8000


def vec(scale=100.0):
    return [f32(rng.uniform(-scale, scale)) for _ in range(3)]


def unit():
    v = [rng.uniform(-1, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(x / n) for x in v]


def make_path():
    kind = rng.choice([1, 2])
    looped = rng.random() < 0.5
    count = rng.randrange(2, 9)
    pos = [vec(500.0)]
    for _ in range(count - 1):
        d = vec(60.0)
        pos.append([f32(pos[-1][k] + d[k]) for k in range(3)])
    recs = []
    total = 0.0
    for i in range(count):
        j = (i + 1) % count
        ln = f32(sum((pos[j][k] - pos[i][k]) ** 2 for k in range(3)) ** 0.5) if rng.random() < 0.9 else 0.0
        total = f32(total + ln)
        if kind == 1:
            recs.append([rng.randrange(-0x8000, 0x8000), rng.randrange(-0x8000, 0x8000), fb(ln), [fb(x) for x in pos[i]]])
        else:
            recs.append([[fb(x) for x in unit()], fb(ln), [fb(x) for x in pos[i]]])
    # The total is the stored lengths' running sum, so a sample never runs
    # past the last record (the original would read beyond the array).
    flags = (0x8000 if looped else 0) | kind
    return {'flags': s16(flags), 'count': count, 'total': fb(total), 'kind': kind, 'records': recs}


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


def put3(a, v):
    for k in range(3): m.put(a + 4 * k, 'f', v[k])


def get3(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


out = {'floor': [], 'wrap': [], 'line': [], 'segment': [], 'nearest': [], 'records': [], 'sample': [], 'follow': []}
for _ in range(2000):
    x = rng.choice([f32(rng.uniform(-1e3, 1e3)), f32(rng.uniform(-3, 3)), float(rng.randrange(-50, 50)), -0.0,
                    f32(rng.uniform(-1e8, 1e8)), bf(rng.randrange(1 << 32))])
    m.call(0x80111240, f1=x); out['floor'].append([fb(x), fb(m.fpr(1))])
    mm = rng.choice([0.0, f32(rng.uniform(1, 500)), f32(rng.uniform(-10, 10))])
    m.call(0x8002B770, f1=x if abs(x) < 1e6 else 1.0, f2=mm)
    out['wrap'].append([fb(x if abs(x) < 1e6 else 1.0), fb(mm), fb(m.fpr(1))])
for _ in range(1500):
    p, pt, d = vec(), vec(), rng.choice([vec(10.0), unit(), [0.0, 0.0, 0.0]])
    want = rng.random() < 0.6
    put3(A, p); put3(B, pt); put3(B + 12, d)
    m.call(0x8011B51C, r3=A, r4=B, r5=OUT if want else 0)
    out['line'].append([[fb(x) for x in p], [fb(x) for x in pt], [fb(x) for x in d], want, fb(m.fpr(1)),
                        get3(OUT) if want else None])
    a, b = vec(), rng.choice([vec(), pt])
    put3(A, p); put3(B, a); put3(C, b)
    m.call(0x800CE5E8, r3=A, r4=B, r5=C, r6=OUT)
    out['segment'].append([[fb(x) for x in p], [fb(x) for x in a], [fb(x) for x in b], fb(m.fpr(1)), get3(OUT)])
for _ in range(500):
    pt = make_path(); write_path(pt)
    pos = vec(600.0)
    put3(A, pos); m.put(OUT, 'I', 0xDEADBEEF); m.put(OUT + 4, 'I', 0xDEADBEEF); m.put(OUT + 8, 'I', 0xDEADBEEF); m.put(B, 'I', 0xDEADBEEF)
    m.call(0x8005C460, r3=H, r4=A, r5=OUT, r6=B)
    out['nearest'].append({'path': pt, 'pos': [fb(x) for x in pos], 'dist': fb(m.fpr(1)), 'point': get3(OUT), 'along': m.get(B, 'I')})
    for i in range(pt['count']):
        m.u.mem_write(OUT, bytes(0x40))
        m.call(0x8005CDCC, r3=H, r4=i, r5=OUT)
        out['records'].append({'path': pt, 'i': i, 'raw': [m.get(OUT + 4 * k, 'I') for k in range(16)]})
    for _ in range(4):
        dist = rng.choice([f32(rng.uniform(-50, 700)), 0.0, bf(pt['total'])])
        m.u.mem_write(INFO, bytes(0x44)); m.put(INFO + 0x10, 'f', dist)
        m.call(0x8005C910, r3=H, r4=INFO)
        out['sample'].append({'path': pt, 'dist': fb(dist), 'ok': m.gpr(3), 'raw': [m.get(INFO + 4 * k, 'I') for k in range(17)]})
    for _ in range(3):
        pnum = rng.choice([0, 1])
        status = rng.choice([0x2000, 0x2003, 0x2001])
        angles = [rng.choice([0, rng.randrange(-0x2000, 0x2000)]), rng.randrange(-0x10000, 0x10000), rng.choice([0, rng.randrange(-0x2000, 0x2000)])]
        recs = pt['records']
        near = [bf(b) for b in (recs[rng.randrange(len(recs))][3] if pt['kind'] == 1 else recs[rng.randrange(len(recs))][2])]
        pos = [f32(near[k] + rng.uniform(-30, 30)) for k in range(3)]
        speed, dash = f32(rng.uniform(0, 8)), f32(rng.choice([5.09, rng.uniform(1, 6)]))
        normal = rng.choice([[0.0, 1.0, 0.0], unit()])
        stick = rng.choice([0.0, 1.0, f32(rng.uniform(0, 1))])
        path_dist = f32(rng.uniform(0, 500))
        m.u.mem_write(T, bytes(0x40)); m.u.mem_write(MW, bytes(0x40)); m.u.mem_write(P, bytes(0x150)); m.u.mem_write(CS, bytes(0x40))
        m.put(T + 4, 'h', status)
        for i, a in enumerate(angles): m.put(T + 8 + 4 * i, 'i', a)
        put3(T + 0x14, pos)
        m.put(P, 'b', pnum); m.put(P + 0x30, 'f', path_dist); m.put(P + 0x64, 'f', speed); m.put(P + 0xF4, 'f', dash)
        m.put(P + 0x90, 'I', CS); m.put(P + 0xB8, 'I', H); put3(CS + 0x1C, normal)
        m.put(STICK + 8 * pnum + 4, 'f', stick)
        m.call(0x80038F40, r3=T, r4=MW, r5=P)
        out['follow'].append({'path': pt, 'status': status, 'angles': angles, 'pos': [fb(x) for x in pos],
                              'speed': fb(speed), 'dash': fb(dash), 'pathDist': fb(path_dist),
                              'normal': [fb(x) for x in normal], 'stick': fb(stick),
                              'outStatus': m.get(T + 4, 'h'), 'outAngles': [m.get(T + 8 + 4 * i, 'i') for i in range(3)],
                              'outSpeed': m.get(P + 0x64, 'I'), 'outPathDist': m.get(P + 0x30, 'I'),
                              'outAim': m.get(MW + 0x1C, 'i')})
print(json.dumps(out))
