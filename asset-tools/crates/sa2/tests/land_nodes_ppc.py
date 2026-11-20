"""Local PowerPC oracle for fn_800D0904: the land collision node list.

Loads the GameCube stg11D.rel (Iron Gate) relocated into the main.dol harness,
runs the collision init fn_800D0F3C, then a sequence of frames. Each frame
moves one or two player tasks and calls fn_800D0904, which selects the nearby
land entries (fn_8002334C), drops the nodes whose entries left the list and
builds nodes (transformed triangles, sorted into 128 depth buckets) for the
new ones. Prints JSON: the object trees and models the land table reaches,
and per frame the dynamic register, the selection list and its dynamic and
static subsets, every active node with its triangles, and four collision
queries (fn_800D1020): the re-filtered list, the candidates and each node's
candidate range, and fn_800CEE08's contacts with the
objects that have candidates.
Needs SA2_MAIN_DOL and SA2_STG11_REL.
"""
import hashlib
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import dol_ppc  # noqa: E402
import gekko_fres  # noqa: E402
from dol_ppc import DEFAULT_PAIRED, Dol  # noqa: E402
from rel_load import load_rel  # noqa: E402

R13 = 0x803B52C0
rel = Path(os.environ["SA2_STG11_REL"]).read_bytes()
assert hashlib.sha1(rel).hexdigest() == "8fbe506bcbbe6f012d2bb8240edb5f6c8cb6ece0"
paired = dict(DEFAULT_PAIRED)
paired.update({0x8011BEF0: "fn_8011BEF0", 0x8011BF0C: "njRotateXTbl", 0x8011BF64: "njRotateYTbl",
               0x8011BFE0: "njRotateZTbl", 0x8011C21C: "njUnitMatrixEx"})
m = Dol(sys.argv[1] if len(sys.argv) > 1 else os.environ["SA2_MAIN_DOL"]).machine(paired=paired, boot=False)
gekko_fres.install(m, dol_ppc)  # njScalor and PSMTXInverse use fres
m.boot()
REL_BASE = 0x80800000
addrs = load_rel(m, rel, REL_BASE)
sec5_off = struct.unpack_from(">I", rel, struct.unpack_from(">I", rel, 16)[0] + 5 * 8)[0] & ~1
TABLE = addrs[5] + (653292 - sec5_off)

W = lambda a: m.get(a, "I")

# Synthetic land entries appended to the table, to reach what Iron Gate's own
# models do not use: object trees, triangle and quad mesh sets, winding
# flips, models without materials, flag 0x08000000 and a full triangle pool.
syn = random.Random(0x5EED)
heap = [0x81200000]


def alloc(size):
    a = heap[0]
    heap[0] = (a + size + 0x1F) & ~0x1F
    return a


def put_f(a, *xs):
    m.u.mem_write(a, struct.pack(">%df" % len(xs), *xs))


def make_model(kinds, mats=True, strip_max=12, npoints=24):
    a = alloc(0x28)
    pts = alloc(12 * npoints)
    for i in range(npoints):
        put_f(pts + 12 * i, *[syn.uniform(-80, 80) for _ in range(3)])
    sets = alloc(0x18 * len(kinds))
    nmat = 3
    for k, kind in enumerate(kinds):
        stream = []
        n = syn.randint(1, 4)
        for _ in range(n):
            if kind == 0:
                stream += [syn.randrange(npoints) for _ in range(3)]
            elif kind == 0x4000:
                stream += [syn.randrange(npoints) for _ in range(4)]
            else:
                c = syn.randint(1, strip_max)
                stream += [c | (0x8000 if syn.random() < 0.5 else 0)] + [syn.randrange(npoints) for _ in range(c)]
        sp = alloc(2 * len(stream))
        m.u.mem_write(sp, struct.pack(">%dH" % len(stream), *stream))
        s = sets + 0x18 * k
        m.u.mem_write(s, struct.pack(">HHI", kind | syn.randrange(nmat), n, sp) + bytes(16))
    mp = 0
    if mats:
        mp = alloc(0x14 * nmat)
        for i in range(nmat):
            m.u.mem_write(mp + 0x14 * i, bytes(16) + struct.pack(">I", syn.randrange(1 << 32)))
    m.u.mem_write(a, struct.pack(">IIiIIHH", pts, 0, npoints, sets, mp, len(kinds), nmat))
    put_f(a + 0x18, *[syn.uniform(-20, 20) for _ in range(3)])
    put_f(a + 0x24, 100.0)
    return a


def make_object(model, child=0, sibling=0):
    a = alloc(0x34)
    m.u.mem_write(a, struct.pack(">II", 0, model))
    put_f(a + 8, *[syn.uniform(-60, 60) for _ in range(3)])
    m.u.mem_write(a + 0x14, struct.pack(">3i", *[syn.choice([0, syn.randrange(1 << 16), syn.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]))
    put_f(a + 0x20, *[syn.choice([1.0, syn.uniform(0.5, 2.0), -1.0]) for _ in range(3)])
    m.u.mem_write(a + 0x2C, struct.pack(">II", child, sibling))
    return a


ALL = [0, 0x4000, 0x8000, 0xC000]
models_syn = [make_model([syn.choice(ALL) for _ in range(syn.randint(1, 4))], mats=syn.random() < 0.8) for _ in range(10)]
big = make_model([0xC000, 0x8000, 0xC000], strip_max=0x400, npoints=200)
roots = []
for k in range(8):
    grand = make_object(syn.choice(models_syn)) if k % 2 else 0
    c2 = make_object(syn.choice(models_syn))
    c1 = make_object(syn.choice(models_syn), child=grand, sibling=c2)
    roots.append(make_object(syn.choice(models_syn), child=c1 if k % 4 else 0))
roots += [make_object(big) for _ in range(4)]
old_count = m.get(TABLE, "h")
old_cols = W(TABLE + 0x10)
new_cols = alloc(0x20 * (old_count + len(roots)))
m.u.mem_write(new_cols, bytes(m.u.mem_read(old_cols, 0x20 * old_count)))
syn_centers = []
for k, r in enumerate(roots):
    c = struct.unpack(">3f", bytes(m.u.mem_read(old_cols + 0x20 * syn.randrange(255, old_count), 12)))
    c = [c[j] + syn.uniform(-300, 300) for j in range(3)]
    syn_centers.append(c)
    flags = 0x08000001 if k in (2, 5) else syn.choice([0x1, 0x40000001, 0x20000001])
    e = new_cols + 0x20 * (old_count + k)
    put_f(e, *c, 250.0)
    m.u.mem_write(e + 0x10, struct.pack(">IIII", r, 0, 0, flags))
m.put(TABLE, "h", old_count + len(roots))
m.put(TABLE + 0x10, "I", new_cols)
H = lambda a: m.get(a, "H")
count, start = m.get(TABLE, "h"), m.get(TABLE + 2, "h")
cols_at = m.get(TABLE + 0x10, "I")
objects, models = {}, {}


def dump_model(a):
    if a in models or a == 0:
        return
    points, nb_point = W(a), m.get(a + 8, "i")
    meshsets, mats = W(a + 0xC), W(a + 0x10)
    nb_mesh, nb_mat = H(a + 0x14), H(a + 0x16)
    sets = []
    for k in range(nb_mesh):
        s = meshsets + 0x18 * k
        type_mat, n = H(s), H(s + 2)
        p = W(s + 4)
        kind = type_mat & 0xC000
        if kind in (0, 0x4000):
            length = n * (3 if kind == 0 else 4)
        elif kind in (0x8000, 0xC000):
            length, q = 0, p
            for _ in range(n):
                c = H(q) & 0x7FFF
                assert c <= 0x400
                length += 1 + c
                q += 2 * (1 + c)
        else:
            length = 0
        sets.append({"type_mat": type_mat, "n": n, "meshes": [H(p + 2 * i) for i in range(length)]})
    used = max([(s["type_mat"] & 0x3FFF) for s in sets] + [-1]) + 1
    models[a] = {
        "points": [[W(points + 12 * i + 4 * j) for j in range(3)] for i in range(nb_point)],
        "sets": sets,
        "attrs": None if mats == 0 else [W(mats + 0x14 * i + 0x10) for i in range(max(used, nb_mat))],
        "center": [W(a + 0x18 + 4 * j) for j in range(3)],
        "radius": W(a + 0x24),
    }


def dump_object(a):
    while a and a not in objects:
        model = W(a + 4)
        assert model, f"object {a:#x} has no model"
        dump_model(model)
        objects[a] = {
            "evalflags": W(a),
            "model": model,
            "pos": [W(a + 8 + 4 * j) for j in range(3)],
            "ang": [m.get(a + 0x14 + 4 * j, "i") for j in range(3)],
            "scl": [W(a + 0x20 + 4 * j) for j in range(3)],
            "child": W(a + 0x2C),
            "sibling": W(a + 0x30),
        }
        dump_object(W(a + 0x2C))
        a = W(a + 0x30)


cols = []
for k in range(count):
    e = cols_at + 0x20 * k
    cols.append([W(e + 4 * j) for j in range(4)] + [W(e + 0x10), W(e + 0x1C)])
    if W(e + 0x1C) & 0x2003:
        dump_object(W(e + 0x10))

# Synthetic dynamic land objects (the register movers fill), each with a
# task whose executor, parent and twp flag decide whether it counts.
DEAD_EXEC = 0x80071AF0
dyn_objs = []
for k in range(14):
    o = make_object(syn.choice(models_syn))
    m.put(o, "I", syn.choice([0, 0x4, 0x20, 0x24]))
    dyn_objs.append(o)
    dump_object(o)
tasks = []
for k in range(len(dyn_objs)):
    t = alloc(0x40)
    parent = alloc(0x40)
    twp = alloc(0x40)
    m.u.mem_write(t + 8, struct.pack(">I", parent if k % 3 else 0))
    m.u.mem_write(t + 0x34, struct.pack(">I", twp))
    tasks.append((t, parent, twp))
dyn_flags = [syn.choice([0x1, 0x10000001, 0x2001, 0x2002, 0x10002001, 0x40000001]) for _ in dyn_objs]

OUT, DYN, STATIC, DYNLIST, DEFAULT = 0x81100000, 0x81110000, 0x81120000, 0x81130000, 0x81140000
TASK1, TASK2 = 0x81150000, 0x81150100
QUERY = 0x81160000
PLAYERTWP = 0x801E7768
CANDS = 0x8028F998
HITS = 0x8028FC98
try:
    m.u.mem_map(0, 0x1000)  # an empty node's list head is read through NULL
except Exception:
    pass
m.put(R13 - 0x79F4, "I", OUT)
m.put(R13 - 0x7A00, "I", DYN)
m.put(R13 - 0x79FC, "I", STATIC)
m.put(R13 - 0x79E0, "I", DYNLIST)
m.put(R13 - 0x79E4, "h", 0)
m.put(R13 - 0x79F0, "I", TABLE)
m.put(R13 - 0x7F28, "i", 1)
m.put(0x80175378, "I", DEFAULT)
unit = [W(0x80357880 + 4 * i) for i in range(12)]
m.call(0x800D0F3C)

rng = random.Random(0xD0904)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
centers = [[struct.unpack(">f", struct.pack(">I", c[j]))[0] for j in range(3)] for c in cols]
frames = []
centers += syn_centers * 6
pos = [list(rng.choice(centers)), list(rng.choice(centers))]
default = [f32(rng.uniform(-500, 500)) for _ in range(3)]
m.u.mem_write(DEFAULT, struct.pack(">3f", *default))
col_index = {}
for k, c in enumerate(cols):
    col_index.setdefault(c[4], k)


def read_list(at, total):
    return [[W(at + 12 * k), W(at + 12 * k + 4), W(at + 12 * k + 8)] for k in range(total)]


for frame in range(int(os.environ.get("LAND_NODES_FRAMES", "40"))):
    n = rng.choice([1, 1, 2, 2, 0])
    for p in pos:
        if rng.random() < 0.3:
            p[:] = rng.choice(centers)
        for j in range(3):
            p[j] = f32(p[j] + rng.uniform(-150, 150))
    rng_range = f32(rng.choice([400.0, 400.0, rng.uniform(100, 800)]))
    refresh = 0
    m.put(R13 - 0x7F30, "f", rng_range)
    m.put(PLAYERTWP, "I", TASK1 if n else 0)
    m.put(PLAYERTWP + 4, "I", TASK2 if n == 2 else 0)
    m.u.mem_write(TASK1 + 0x14, struct.pack(">3f", *pos[0]))
    m.u.mem_write(TASK2 + 0x14, struct.pack(">3f", *pos[1]))
    # Move the dynamic objects near the players and register some of them.
    register = []
    near = pos[: max(n, 1)] if n else [default]
    for k, o in enumerate(dyn_objs):
        if rng.random() < 0.3:
            c = rng.choice(near)
            put_f(o + 8, *[f32(c[j] + rng.uniform(-250, 250)) for j in range(3)])
        t, parent, twp = tasks[k]
        m.u.mem_write(t + 0x10, struct.pack(">I", DEAD_EXEC if rng.random() < 0.1 else 0x80001234))
        m.u.mem_write(parent + 0x10, struct.pack(">I", DEAD_EXEC if rng.random() < 0.1 else 0x80001234))
        m.put(twp + 4, "H", rng.choice([0x100, 0x100, 0x100, 0x0, 0x1100]))
        if rng.random() < 0.8:
            register.append(k)
    for i, k in enumerate(register):
        m.u.mem_write(DYNLIST + 12 * i, struct.pack(">III", dyn_flags[k], dyn_objs[k], tasks[k][0]))
    m.put(R13 - 0x79E4, "h", len(register))
    reg_out = []
    for k in register:
        t, parent, twp = tasks[k]
        par = W(t + 8)
        active = W(t + 0x10) != DEAD_EXEC and not (par and W(par + 0x10) == DEAD_EXEC) \
            and m.get(W(t + 0x34) + 4, "H") & 0x100 != 0
        reg_out.append([dyn_flags[k], dyn_objs[k], t, active])
    if frames and rng.random() < 0.2 and frames[-1]["nodes"]:
        refresh = rng.choice(frames[-1]["nodes"])["obj"]
        m.put(R13 - 0x7750, "I", refresh)
    m.call(0x800D0904, count=400_000_000)
    total = m.get(R13 - 0x79A4, "h")
    entries = read_list(OUT, total)
    dyn_sub = read_list(DYN, W(R13 - 0x7A08))
    statics = []
    for k in range(W(R13 - 0x7A04)):
        e = STATIC + 0x20 * k
        idx = col_index[W(e + 0x10)]
        assert [W(e + 4 * j) for j in range(4)] + [W(e + 0x10), W(e + 0x1C)] == cols[idx]
        statics.append(idx)
    nodes, tri_at = [], {}
    node = W(R13 - 0x7738)
    node_addrs = []
    while node:
        tris, t = [], W(node + 0x10)
        while t:
            tri_at[t] = (len(nodes), len(tris))
            tris.append([W(t + 8 + 4 * j) for j in range(14)])
            last = t
            t = W(t)
        assert not tris or last == W(node + 0x14)
        nodes.append({
            "obj": W(node + 8), "flags": W(node + 0xC),
            "dz": W(node + 0x28), "w2c": W(node + 0x2C),
            "center": [W(node + 0x30 + 4 * j) for j in range(3)], "tris": tris,
        })
        node_addrs.append(node)
        node = W(node)
    queries = []
    for _ in range(4):
        with_tris = [x for x in nodes if x["tris"]]
        if with_tris and rng.random() < 0.75:
            t = rng.choice(rng.choice(with_tris)["tris"])
            c = [struct.unpack(">f", struct.pack(">I", t[j]))[0] for j in range(3)]
            spread = rng.choice([5, 30, 120])
        else:
            c, spread = rng.choice(near), 120
        center = [f32(c[j] + rng.uniform(-spread, spread)) for j in range(3)]
        radius = f32(rng.choice([5.0, 10.0, rng.uniform(1, 60), rng.uniform(60, 400)]))
        motion = [0.0, 0.0, 0.0] if rng.random() < 0.3 else [f32(rng.uniform(-30, 30)) for _ in range(3)]
        put_f(QUERY, radius, *center, *motion)
        m.call(0x800D1020, r3=QUERY, count=100_000_000)
        got = m.gpr(3)
        assert got == W(R13 - 0x7758)
        lst = read_list(OUT, m.get(R13 - 0x79A4, "h"))
        cands = [list(tri_at[W(CANDS + 4 * k)]) for k in range(got)]
        per_node = []
        for a in node_addrs:
            first = W(a + 0x1C)
            per_node.append([None if first == 0 else (first - CANDS) // 4, W(a + 0x24)])
        # fn_800CEE08 for the listed objects that have candidates, plus a
        # couple without, in both modes.
        objs = []
        for a, (first, cnt) in zip(node_addrs, per_node):
            if cnt and W(a + 8) not in objs:
                objs.append(W(a + 8))
        objs += [e[1] for e in rng.sample(lst, min(2, len(lst)))]
        contacts = []
        for obj in objs:
            mode = rng.choice([0, 1])
            m.u.mem_write(QUERY + 0x28, bytes([0xFF]) * 0x5C)
            m.call(0x800CEE08, r3=QUERY, r4=obj, r6=mode, count=100_000_000)
            ret = m.gpr(3)
            out = {"obj": obj, "mode": mode, "ret": ret,
                   "stats": [W(R13 - 0x77A8), W(R13 - 0x77AC), W(R13 - 0x77B0)]}
            if ret:
                hits = []
                for k in range(W(R13 - 0x7754)):
                    r = HITS + 0x9C * k
                    hits.append([W(r + 0xC + 4 * j) for j in range(14)] + [W(r + 0x44 + 4 * j) for j in range(22)])
                out.update({"count": m.get(QUERY + 0x28, "H"), "best": m.get(QUERY + 0x2A, "H"),
                            "out": [W(QUERY + 0x34 + 4 * j) for j in range(20)], "hits": hits})
            contacts.append(out)
        queries.append({"radius": fb(radius), "center": [fb(x) for x in center],
                        "motion": [fb(x) for x in motion], "list": lst, "count": got,
                        "candidates": cands, "nodes": per_node, "contacts": contacts})
    frames.append({"positions": [[fb(x) for x in p] for p in pos[:n]], "range": fb(rng_range),
                   "refresh": refresh, "register": reg_out,
                   "moved": [[o, [W(o + 8 + 4 * j) for j in range(3)]] for o in dyn_objs], "entries": entries,
                   "dynamic": dyn_sub, "statics": statics, "nodes": nodes,
                   "refresh_after": W(R13 - 0x7750), "queries": queries})
print(json.dumps({"unit": unit, "head": [count, start], "cols": cols, "objects": objects,
                  "models": models, "default": [fb(x) for x in default], "frames": frames}))
