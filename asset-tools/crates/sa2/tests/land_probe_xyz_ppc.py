"""Run main.dol's six-ray land probe fn_800CFE6C on synthetic collision
nodes and print JSON: each case's triangles, point, the block it fills
(0xCC bytes as words) and its return value. tests/land_probe_xyz.rs runs
land_probe::six_way_probe on the same nodes.

Needs SA2_MAIN_DOL (GSNE8P main.dol, never committed).
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import dol_ppc  # noqa: E402
import gekko_tables  # noqa: E402
from dol_ppc import Dol  # noqa: E402

gekko_tables.require()  # bit-exact estimates need the local tables
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
ACTIVE = 0x803B52C0 - 0x7738
NODE, TRIS, BLOCK, OBJ = 0x81100000, 0x81110000, 0x81000000, 0x810F0000
rng = random.Random(int(os.environ.get("SA2_PROBE_SEED", "3")))
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
f32 = lambda x: struct.unpack('>f', struct.pack('>f', x))[0]


def rf(lo, hi):
    return f32(rng.uniform(lo, hi))


cases = []
for c in range(int(os.environ.get("SA2_PROBE_CASES", "1500"))):
    p = [rf(-300, 300), rf(-300, 300), rf(-300, 300)]
    n = rng.randint(1, 24)
    tris = []
    for _ in range(n):
        kind = rng.random()
        if kind < 0.25:
            # Axis-aligned walls and floors around the point.
            ax = rng.randrange(3)
            off = rf(-100, 100)
            q = [[0.0] * 3 for _ in range(3)]
            for i in range(3):
                for k in range(3):
                    q[i][k] = f32(p[k] + rf(-150, 150))
                q[i][ax] = f32(p[ax] + off)
        else:
            c0 = [f32(p[k] + rf(-150, 150)) for k in range(3)]
            q = [[f32(c0[k] + rf(-120, 120)) for k in range(3)] for _ in range(3)]
        if rng.random() < 0.03:
            q[2] = list(q[1])  # degenerate
        tris.append(q)
    flags = rng.choice([0, 1, 0x2002, rng.randrange(1 << 31)])
    obj = OBJ + 0x40 * rng.randrange(4)
    m.put(ACTIVE, 'I', NODE)
    m.u.mem_write(NODE, bytes(0x40))
    m.put(NODE + 8, 'I', obj)
    m.put(NODE + 0xC, 'I', flags)
    prev = NODE + 0x10
    for i, q in enumerate(tris):
        t = TRIS + 0x40 * i
        m.u.mem_write(t, bytes(0x40))
        m.u.mem_write(t + 0x18, struct.pack('>9f', *(x for v in q for x in v)))
        m.put(prev, 'I', t)
        prev = t
    m.put(prev, 'I', 0)
    asked = obj if rng.random() < 0.95 else OBJ + 0x400
    m.u.mem_write(BLOCK, bytes(0xD0))
    m.u.mem_write(BLOCK, struct.pack('>3f', *p))
    m.call(0x800CFE6C, r3=BLOCK, r4=asked)
    cases.append({
        'p': [fb(x) for x in p],
        'tris': [[[fb(x) for x in v] for v in q] for q in tris],
        'flags': flags,
        'found': int(asked == obj),
        'ret': m.gpr(3),
        'block': [m.get(BLOCK + 4 * k, 'I') for k in range(0xCC // 4)],
    })
print(json.dumps({'cases': cases}))
