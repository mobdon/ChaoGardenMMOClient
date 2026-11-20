"""Run main.dol's floor and ceiling probe fn_8006779C on synthetic land
objects and print JSON for tests/player_shadow.rs.

Needs SA2_MAIN_DOL (GSNE8P main.dol, never committed) and the local Gekko
tables.
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
from dol_ppc import Dol  # noqa: E402

gekko_tables.require()
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
R13 = 0x803B52C0
NODES, TRIS, OBJS, MODELS, TASKS, LIST, BLOCK, PT = (0x81100000, 0x81110000, 0x81000000, 0x81010000, 0x81020000,
                                                     0x81030000, 0x81040000, 0x81041000)
rng = random.Random(0x6779C)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
stage = [0]
m.stub(0x8001DD78, lambda mm: mm.gpr(3, stage[0]))


def rf(lo, hi):
    return f32(rng.uniform(lo, hi))


cases = []
for _ in range(1500):
    p = [rf(-200, 200), rf(-200, 200), rf(-200, 200)]
    nobj = rng.randint(1, 5)
    objs, nodes, ents = [], [], []
    for k in range(nobj):
        ev = rng.choice([0, 0x20, 4])
        pos = [f32(p[j] + rng.uniform(-60, 60)) for j in range(3)]
        ang = [rng.randrange(-0x4000, 0x4000) for _ in range(3)]
        scl = [rf(0.5, 2) for _ in range(3)]
        center = [rf(-20, 20) for _ in range(3)]
        radius = rf(1, 40)
        objs.append({"ev": ev, "pos": pos, "ang": ang, "scl": scl, "center": center, "radius": radius})
        tris = []
        for _ in range(rng.randint(0, 10)):
            c0 = [f32(p[j] + rng.uniform(-80, 80)) for j in range(3)]
            if rng.random() < 0.6:
                y = f32(p[1] + rng.uniform(-60, 60))
                q = [[f32(c0[0] + rng.uniform(-90, 90)), f32(y + rng.uniform(-5, 5)), f32(c0[2] + rng.uniform(-90, 90))]
                     for _ in range(3)]
            else:
                q = [[f32(c0[j] + rng.uniform(-90, 90)) for j in range(3)] for _ in range(3)]
            tris.append(q)
        nflags = rng.choice([0, 1, 0x10000000])
        nodes.append({"obj": k, "flags": nflags, "tris": tris})
    for _ in range(rng.randint(0, 6)):
        task = rng.choice([None, None, 0, 1, 2])
        ents.append({"flags": rng.choice([0, 1, 0x10000000, 0x10000001]), "obj": rng.randrange(nobj), "task": task})
    tasks = [{"twp": rng.random() < 0.8, "flag": rng.choice([0x100, 0, 0x101]), "blk": rng.random() < 0.6,
              "off": rf(-10, 10)} for _ in range(3)]
    stage[0] = rng.choice([0xB, 0x22])
    prev_blk = [rng.randrange(0, 1 << 32) for _ in range(8)]
    # Memory.
    for k, o in enumerate(objs):
        a, md = OBJS + 0x40 * k, MODELS + 0x40 * k
        m.u.mem_write(a, struct.pack(">II3f3i3f", o["ev"], md, *o["pos"], *o["ang"], *o["scl"]))
        m.u.mem_write(md + 0x18, struct.pack(">4f", *o["center"], o["radius"]))
    prev = R13 - 0x7738
    t = TRIS
    for k, n in enumerate(nodes):
        a = NODES + 0x20 * k
        m.u.mem_write(a, bytes(0x20))
        m.put(prev, "I", a)
        m.put(a + 8, "I", OBJS + 0x40 * n["obj"])
        m.put(a + 0xC, "I", n["flags"])
        tp = a + 0x10
        for q in n["tris"]:
            m.u.mem_write(t, bytes(0x40))
            m.u.mem_write(t + 0x18, struct.pack(">9f", *(x for v in q for x in v)))
            m.put(tp, "I", t)
            tp = t
            t += 0x40
        m.put(tp, "I", 0)
        prev = a
    m.put(prev, "I", 0)
    for k, tk in enumerate(tasks):
        a = TASKS + 0x100 * k
        m.u.mem_write(a, bytes(0x100))
        if tk["twp"]:
            m.put(a + 0x34, "I", a + 0x80)
            m.put(a + 0x80 + 4, "H", tk["flag"])
        if tk["blk"]:
            m.put(a + 0x3C, "I", a + 0xC0)
            m.put(a + 0xC0 + 0x14, "f", tk["off"])
    for k, e in enumerate(ents):
        m.u.mem_write(LIST + 0xC * k, struct.pack(">III", e["flags"], OBJS + 0x40 * e["obj"],
                                                   0 if e["task"] is None else TASKS + 0x100 * e["task"]))
    m.put(R13 - 0x79F4, "I", LIST)
    m.put(R13 - 0x79A4, "h", len(ents))
    m.u.mem_write(BLOCK, struct.pack(">8I", *prev_blk))
    m.u.mem_write(PT, struct.pack(">3f", *p))
    m.call(0x8006779C, r3=PT, r4=BLOCK)
    cases.append({"p": [fb(x) for x in p], "stage": stage[0], "prev": prev_blk,
                  "objs": [{**o, "pos": [fb(x) for x in o["pos"]], "scl": [fb(x) for x in o["scl"]],
                            "center": [fb(x) for x in o["center"]], "radius": fb(o["radius"])} for o in objs],
                  "nodes": [{**n, "tris": [[[fb(x) for x in v] for v in q] for q in n["tris"]]} for n in nodes],
                  "ents": ents, "tasks": [{**tk, "off": fb(tk["off"])} for tk in tasks],
                  "block": [m.get(BLOCK + 4 * k, "I") for k in range(8)]})
print(json.dumps({"cases": cases}))
