"""Local PowerPC oracle for Ninja matrices in main.dol (GSNE8P).

Runs PSMTXConcat, PSMTXMultVecSR, PSMTXMultVec, njRotateX/Y/Z on the
current matrix, and the player-frame helpers fn_8003E9E0 / fn_8003EA6C on
randomized inputs, and prints JSON (all floats as bit patterns).
tests/ninja_matrix_dol.rs replays them in Rust and compares.
Usage: python ninja_matrix_ppc.py [MAIN_DOL]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol

dol = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"])
m = dol.machine()
A, B, C, V, O, T = 0x81000000, 0x81000040, 0x81000080, 0x810000C0, 0x810000D0, 0x81000100
CUR = 0x803B52C0 - 0x71FC  # Ninja's current-matrix pointer
rng = random.Random(5)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]

def rnd():
    k = rng.random()
    if k < 0.05: return rng.choice([0.0, -0.0, 1.0, -1.0])
    if k < 0.1: return bf(rng.randrange(1 << 32) & 0x8fffffff | 0x30000000)
    return struct.unpack('>f', struct.pack('>f', rng.uniform(-3000.0, 3000.0)))[0]

def put_m(addr, mat):
    m.u.mem_write(addr, struct.pack('>12f', *[x for row in mat for x in row]))

def get_m(addr):
    return [m.get(addr + 4 * k, 'I') for k in range(12)]

out = {'concat': [], 'vec': [], 'rotate': [], 'frame': []}
for _ in range(400):
    a = [[rnd() for _ in range(4)] for _ in range(3)]
    b = [[rnd() for _ in range(4)] for _ in range(3)]
    put_m(A, a); put_m(B, b)
    m.call(0x800ED368, r3=A, r4=B, r5=C)
    out['concat'].append([[fb(x) for r in a for x in r], [fb(x) for r in b for x in r], get_m(C)])
    v = [rnd() for _ in range(3)]
    m.u.mem_write(V, struct.pack('>3f', *v))
    m.call(0x800EDB98, r3=A, r4=V, r5=O); sr = [m.get(O + 4 * k, 'I') for k in range(3)]
    m.call(0x800EDAB8, r3=A, r4=V, r5=O); pt = [m.get(O + 4 * k, 'I') for k in range(3)]
    out['vec'].append([[fb(x) for r in a for x in r], [fb(x) for x in v], sr, pt])
for _ in range(600):
    a = [[rnd() for _ in range(4)] for _ in range(3)]
    axis = rng.choice([0, 1, 2])
    angle = rng.choice([0, 0x4000, -0x4000, 0x8000, 0x10000, rng.randrange(-0x20000, 0x20000), rng.randrange(-(1 << 31), 1 << 31)])
    cur = m.get(CUR, 'I'); put_m(cur, a)
    m.call([0x80115C60, 0x80115828, 0x801158A8][axis], r3=0, r4=angle & 0xffffffff)
    out['rotate'].append([[fb(x) for r in a for x in r], axis, angle, get_m(cur)])
for _ in range(600):
    angles = [rng.choice([0, 0x4000, rng.randrange(-0x10000, 0x10000), rng.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]
    v = [rnd() for _ in range(3)]
    for i, x in enumerate(angles): m.put(T + 8 + 4 * i, 'i', x)
    res = []
    for fn in (0x8003E9E0, 0x8003EA6C):
        m.u.mem_write(V, struct.pack('>3f', *v))
        depth = m.get(CUR, 'I')
        m.call(fn, r3=T, r4=V)
        assert m.get(CUR, 'I') == depth
        res.append([m.get(V + 4 * k, 'I') for k in range(3)])
    out['frame'].append([angles, [fb(x) for x in v], res[0], res[1]])
print(json.dumps(out))
