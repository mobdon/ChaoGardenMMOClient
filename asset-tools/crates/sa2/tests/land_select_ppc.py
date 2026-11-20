"""Local PowerPC oracle for fn_8002334C's static land-table pass.

Loads the GameCube stg11D.rel (Iron Gate) relocated into the main.dol
harness, points the current LandTable at objLandTable0011, places one or two
player tasks and runs the routine. Prints JSON: the COL entries (center,
radius and flags bits), the head, and per case the positions, range and
selected indices. Needs SA2_MAIN_DOL and SA2_STG11_REL.
"""
import hashlib
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402
import gekko_tables  # noqa: E402

gekko_tables.require()  # bit-exact frsqrte
from rel_load import load_rel  # noqa: E402

R13 = 0x803B52C0
rel = Path(os.environ["SA2_STG11_REL"]).read_bytes()
assert hashlib.sha1(rel).hexdigest() == "8fbe506bcbbe6f012d2bb8240edb5f6c8cb6ece0"
m = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"]).machine()
REL_BASE = 0x80800000
addrs = load_rel(m, rel, REL_BASE)
sec5_off = struct.unpack_from(">I", rel, struct.unpack_from(">I", rel, 16)[0] + 5 * 8)[0] & ~1
TABLE = addrs[5] + (653292 - sec5_off)
count, start = m.get(TABLE, "h"), m.get(TABLE + 2, "h")
cols_at = m.get(TABLE + 0x10, "I")
cols = []
for k in range(count):
    a = cols_at + 0x20 * k
    cols.append([m.get(a + 4 * j, "I") for j in range(4)] + [m.get(a + 0x1C, "I")])

OUT, DYN, STATIC, DYNLIST, DEFAULT = 0x81100000, 0x81110000, 0x81120000, 0x81130000, 0x81140000
TASK1, TASK2 = 0x81150000, 0x81150100
PLAYERTWP = 0x801E7768
m.put(R13 - 0x79F4, "I", OUT)
m.put(R13 - 0x7A00, "I", DYN)
m.put(R13 - 0x79FC, "I", STATIC)
m.put(R13 - 0x79E0, "I", DYNLIST)
m.put(R13 - 0x79E4, "h", 0)
m.put(R13 - 0x79F0, "I", TABLE)
m.put(R13 - 0x7F28, "i", 1)
m.put(0x80175378, "I", DEFAULT)
m.u.mem_write(DEFAULT, struct.pack(">3f", 0.0, 0.0, 0.0))

rng = random.Random(0x2334C)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
cases = []
centers = [[struct.unpack(">f", struct.pack(">I", c[j]))[0] for j in range(3)] for c in cols]
for _ in range(300):
    n = rng.choice([1, 2])
    pos = []
    for _ in range(n):
        c = rng.choice(centers)
        spread = rng.choice([60, 300, 900])
        pos.append([f32(c[j] + rng.uniform(-spread, spread)) for j in range(3)])
    rng_range = f32(rng.choice([400.0, 400.0, rng.uniform(0, 800)]))
    m.put(R13 - 0x7F30, "f", rng_range)
    m.put(PLAYERTWP, "I", TASK1)
    m.put(PLAYERTWP + 4, "I", TASK2 if n == 2 else 0)
    m.u.mem_write(TASK1 + 0x14, struct.pack(">3f", *pos[0]))
    if n == 2:
        m.u.mem_write(TASK2 + 0x14, struct.pack(">3f", *pos[1]))
    m.call(0x8002334C)
    total = m.get(R13 - 0x79A4, "h")
    statics = m.get(R13 - 0x7A04, "I")
    picked = []
    for k in range(total):
        obj = m.get(OUT + 12 * k + 4, "I")
        picked.append(obj)
    # Map each output object pointer back to the first matching COL index in order.
    objs = [m.get(cols_at + 0x20 * k + 0x10, "I") for k in range(count)]
    idx, cursor = [], 0
    for obj in picked:
        while objs[cursor] != obj:
            cursor += 1
        idx.append(cursor)
        cursor += 1
    cases.append({"positions": [[fb(x) for x in p] for p in pos], "range": fb(rng_range), "picked": idx, "statics": statics, "total": total})
print(json.dumps({"head": [count, start], "cols": cols, "cases": cases}))
