"""Local PowerPC oracle for fn_8003EAFC, the walker ground pass.

Sets up Iron Gate's land (the GameCube stg11D.rel relocated into the main.dol
harness) with synthetic object trees and movers, then for each case builds
one frame of land selection and nodes around a randomized player and runs
fn_8003EAFC. fn_8004E560 and GetStageNum are stubbed; the movers' ride
callback is a hook that pushes the player's task. Prints JSON for
tests/walker_ground.rs. Needs SA2_MAIN_DOL and SA2_STG11_REL.
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


# --- fn_8003EAFC: the walker ground pass -------------------------------------
# Each case runs one frame of land selection and nodes (fn_800D0904) around
# the player, then the ground pass with a randomized player.
# Ride records for the movers (task +0x3C; a second record for player 2 at
# +0x1C). The callback is a hook that moves the player's task.
CALLBACK = 0x81170000
m.put(CALLBACK, "I", 0x4E800020)
RIDE_PUSH = [0.75, -0.5, 1.25]


def ride_callback(mm):
    t = mm.gpr(4)
    for j in range(3):
        mm.put(t + 0x14 + 4 * j, "f", f32(mm.get(t + 0x14 + 4 * j, "f") + RIDE_PUSH[j]))


m.stub(CALLBACK, ride_callback)
records = []
for k, (t, parent, twp) in enumerate(tasks):
    rec = alloc(0x38)
    for p in range(2):
        r = rec + 0x1C * p
        m.put(r, "I", CALLBACK if syn.random() < 0.5 else 0)
        m.u.mem_write(r + 4, struct.pack(">3i", *[syn.choice([0, syn.randrange(-0x300, 0x300)]) for _ in range(3)]))
        put_f(r + 0x10, *[syn.choice([0.0, syn.uniform(-2, 2)]) for _ in range(3)])
    m.put(t + 0x3C, "I", rec if k % 4 else 0)
    records.append(rec if k % 4 else 0)

analog = [0, 0.0]


def analog_stub(mm):
    mm.gpr(3, analog[0])
    out = mm.gpr(6)
    if out:
        mm.put(out, "f", analog[1])


m.stub(0x8004E560, analog_stub)
stage = [11]
m.stub(0x8001DD78, lambda mm: mm.gpr(3, stage[0]))

# WALKER_PASS picks fn_8003EAFC (ground) or fn_80043ED4 (floor).
PASS = {"ground": 0x8003EAFC, "floor": 0x80043ED4}[os.environ.get("WALKER_PASS", "ground")]
TASK, VEL, PW, Q, MW = TASK1, 0x81180000, 0x81181000, 0x81182000, 0x81183000
PHYS = 0x80196578
GLOBAL = 0x801E5624
rows = [[m.get(PHYS + 0x84 * k + o, "I") for o in (0x3C, 0x58, 0x74, 0x78)] for k in range(16)]
put_f(GLOBAL, 0.0, -1.0, 0.0)


def rec_out(t):
    if not t:
        return None
    out = []
    for p in range(2):
        r = t + 0x1C * p
        out.append([W(r), [m.get(r + 4 + 4 * j, "i") for j in range(3)], [W(r + 0x10 + 4 * j) for j in range(3)]])
    return out


def floor_tris():
    best = []
    node = W(R13 - 0x7738)
    while node:
        t = W(node + 0x10)
        while t:
            best.append(t)
            t = W(t)
        node = W(node)
    return best


cases = []
# WG_TRACE lists hex addresses whose execution is logged per case.
trace = []
for a in filter(None, os.environ.get("WG_TRACE", "").split(",")):
    m.u.hook_add(dol_ppc.UC_HOOK_CODE, lambda uc, pc, size, _: trace.append(pc), begin=int(a, 16), end=int(a, 16))
pos = list(rng.choice(centers))
for case in range(int(os.environ.get("WALKER_GROUND_CASES", "400"))):
    # Pick a spot near land, then build this frame's nodes around it.
    tris = floor_tris()
    if tris and rng.random() < 0.85:
        tr = rng.choice(tris)
        c = [m.get(tr + 8 + 4 * j, "f") for j in range(3)]
        pos = [f32(c[0] + rng.uniform(-5, 5)), f32(c[1] + rng.uniform(-2, 4)), f32(c[2] + rng.uniform(-5, 5))]
    else:
        c = rng.choice(centers)
        pos = [f32(c[j] + rng.uniform(-100, 100)) for j in range(3)]
    register = []
    for k, o in enumerate(dyn_objs):
        if rng.random() < 0.3:
            put_f(o + 8, *[f32(pos[j] + rng.uniform(-60, 60)) for j in range(3)])
        t, parent, twp = tasks[k]
        m.u.mem_write(t + 0x10, struct.pack(">I", 0x80001234))
        m.u.mem_write(parent + 0x10, struct.pack(">I", 0x80001234))
        m.put(twp + 4, "H", rng.choice([0x100, 0x100, 0x0]))
        if rng.random() < 0.7:
            register.append(k)
    for i, k in enumerate(register):
        m.u.mem_write(DYNLIST + 12 * i, struct.pack(">III", dyn_flags[k], dyn_objs[k], tasks[k][0]))
    m.put(R13 - 0x79E4, "h", len(register))
    m.put(R13 - 0x7F30, "f", 400.0)
    m.put(PLAYERTWP, "I", TASK)
    m.put(PLAYERTWP + 4, "I", 0)
    m.u.mem_write(TASK + 0x14, struct.pack(">3f", *pos))
    m.call(0x800D0904, count=400_000_000)
    frame_entries = read_list(OUT, m.get(R13 - 0x79A4, "h"))

    # The player.
    pnum = rng.choice([0, 0, 1])
    stage[0] = rng.choice([11, 11, 30])
    task = {"mode": rng.choice([0, 1, 0x4C]), "flag": rng.choice([0, 0, 1, 1, 3, 2, 0x2001]),
            "angles": [rng.choice([0, 0, rng.randrange(-0x1000, 0x1000)]), rng.randrange(0, 0x10000),
                       rng.choice([0, 0, rng.randrange(-0x1000, 0x1000)])], "pos": pos}
    vel = {"v": [f32(rng.uniform(-4, 4)), f32(rng.choice([rng.uniform(-6, 1), -2.0])), f32(rng.uniform(-4, 4))],
           "angles": [rng.randrange(0, 0x10000) for _ in range(3)]}
    objs = [e[1] for e in frame_entries]
    dyn_tasks = [e[2] for e in frame_entries if e[2]]
    pw = {"pnum": pnum, "character": rng.choice([7, 6, 7, 0, 1]), "accel_row": rng.choice([6, 7]),
          "h12": 0x1234, "h14": rng.choice([0, 0, 1]), "status": rng.choice([0, 1, 1, 3, 0x4001]),
          "dotp": f32(rng.uniform(0.5, 1)), "f40": 1.0,
          "v4c": [0.0, 0.0, 0.0] if rng.random() < 0.7 else [f32(rng.uniform(-1, 1)) for _ in range(3)],
          "spd": [f32(rng.choice([0.0, rng.uniform(-1, 8)])), f32(rng.choice([0.0, 0.0, -1.0, rng.uniform(-3, 3)])),
                  f32(rng.choice([0.0, rng.uniform(-1, 1)]))],
          "wall": [0.0, 0.0, 0.0], "floor_normal": [0.0, 1.0, 0.0],
          "attr": rng.randrange(1 << 32), "last_attr": rng.choice([0, 0x100, 0x8, 0x80000]),
          "ride": rng.choice([0] + dyn_tasks[:2]), "landed_on": rng.choice([0, 0] + objs[:3]),
          "w_c0": rng.randrange(1 << 32), "height": f32(rng.uniform(4, 12)),
          "jog": f32(rng.uniform(0.3, 1.5)), "run": f32(rng.uniform(1.5, 4)), "f_ec": f32(rng.uniform(4, 6)),
          "f_f0": f32(rng.uniform(6, 10)), "f_f4": f32(rng.uniform(8, 14)), "run_accel": 0.05, "f118": 0.0,
          "f134": f32(rng.uniform(10, 40)), "weight": 0.08, "flags_1a8": rng.choice([0, 0, 2]),
          "height_1b0": f32(pos[1] + rng.uniform(-50, 50))}
    q = {"radius": 1.0, "center": [0.0] * 3, "motion": [0.0] * 3, "up": [0.0, 1.0, 0.0], "count": 0, "best": 0,
         "ang_x": 0, "ang_z": 0, "normal": [0.0] * 3, "facing": [0.0] * 3, "foot": [0.0] * 3, "head": [0.0] * 3,
         "along": [0.0] * 3, "across": [0.0] * 3}
    mw = {"spd": [f32(rng.uniform(-3, 3)) for _ in range(3)], "aim": rng.randrange(-0x10000, 0x10000)}
    stuck = rng.choice([0, 0, 1])
    analog[0], analog[1] = rng.choice([0, 1]), f32(rng.choice([0.5, 1.0]))
    f26 = f32(rng.choice([1000.0, 0.0, 30.0]))

    m.put(TASK, "B", task["mode"]); m.put(TASK + 4, "H", task["flag"] & 0xFFFF)
    m.u.mem_write(TASK + 8, struct.pack(">3i", *task["angles"]))
    put_f(TASK + 0x14, *task["pos"])
    put_f(VEL, *vel["v"]); m.u.mem_write(VEL + 0x18, struct.pack(">3i", *vel["angles"]))
    m.put(PW, "b", pw["pnum"]); m.put(PW + 1, "b", pw["character"]); m.put(PW + 3, "b", pw["accel_row"])
    m.put(PW + 0x12, "H", pw["h12"]); m.put(PW + 0x14, "H", pw["h14"]); m.put(PW + 0x1C, "H", pw["status"])
    m.put(PW + 0x34, "f", pw["dotp"]); m.put(PW + 0x40, "f", pw["f40"]); put_f(PW + 0x4C, *pw["v4c"])
    put_f(PW + 0x64, *pw["spd"]); put_f(PW + 0x70, *pw["wall"]); put_f(PW + 0x7C, *pw["floor_normal"])
    m.put(PW + 0x88, "I", pw["attr"]); m.put(PW + 0x8C, "I", pw["last_attr"])
    m.put(PW + 0x90, "I", Q)
    m.put(PW + 0xA0, "I", pw["ride"]); m.put(PW + 0xB0, "I", pw["landed_on"])
    m.put(PW + 0xC0, "I", pw["w_c0"]); m.put(PW + 0xC4, "f", pw["height"])
    for o, k in ((0xE4, "jog"), (0xE8, "run"), (0xEC, "f_ec"), (0xF0, "f_f0"), (0xF4, "f_f4"), (0xFC, "run_accel"),
                 (0x118, "f118"), (0x134, "f134"), (0x138, "weight"), (0x1B0, "height_1b0")):
        m.put(PW + o, "f", pw[k])
    m.put(PW + 0x1A8, "I", pw["flags_1a8"])
    m.put(Q, "f", q["radius"]); put_f(Q + 4, *q["center"]); put_f(Q + 0x10, *q["motion"]); put_f(Q + 0x1C, *q["up"])
    m.put(Q + 0x28, "H", 0); m.put(Q + 0x2A, "H", 0)
    m.u.mem_write(Q + 0x34, bytes(0x50))
    m.put(0x801E7748 + 4 * pnum, "I", MW)
    m.put(0x801E7728 + 4 * pnum, "I", PW)
    put_f(MW, *mw["spd"]); m.put(MW + 0x1C, "i", mw["aim"])
    m.put(R13 - 0x78F4, "I", stuck)
    probe = rng.choice([0, 0, 0, 1])
    m.put(R13 - 0x7900, "I", probe)
    trace.clear()
    m.call(PASS, r3=TASK, r4=VEL, r5=PW, f26=f26, count=400_000_000)
    ret = m.gpr(3)
    after = {
        "ret": 3 if ret == VEL else ret,
        "task": [m.get(TASK, "B"), m.get(TASK + 4, "h"), [m.get(TASK + 8 + 4 * j, "i") for j in range(3)],
                 [W(TASK + 0x14 + 4 * j) for j in range(3)]],
        "vel": [[W(VEL + 4 * j) for j in range(3)], [m.get(VEL + 0x18 + 4 * j, "i") for j in range(3)]],
        "pw": {"h12": m.get(PW + 0x12, "h"), "status": m.get(PW + 0x1C, "h"), "f40": W(PW + 0x40),
               "spd": [W(PW + 0x64 + 4 * j) for j in range(3)], "wall": [W(PW + 0x70 + 4 * j) for j in range(3)],
               "floor_normal": [W(PW + 0x7C + 4 * j) for j in range(3)], "attr": W(PW + 0x88),
               "ride": W(PW + 0xA0), "landed_on": W(PW + 0xB0), "run_accel": W(PW + 0xFC), "f118": W(PW + 0x118),
               "weight": W(PW + 0x138)},
        "q": {"radius": W(Q), "center": [W(Q + 4 + 4 * j) for j in range(3)], "motion": [W(Q + 0x10 + 4 * j) for j in range(3)],
              "up": [W(Q + 0x1C + 4 * j) for j in range(3)], "count": m.get(Q + 0x28, "H"), "best": m.get(Q + 0x2A, "H"),
              "ang": [m.get(Q + 0x34, "i"), m.get(Q + 0x38, "i")],
              "rest": [W(Q + 0x3C + 4 * j) for j in range(18)]},
        "mw": [[W(MW + 4 * j) for j in range(3)], m.get(MW + 0x1C, "i")],
        "stuck": W(R13 - 0x78F4),
        "stats": [W(R13 - 0x77A8), W(R13 - 0x77AC), W(R13 - 0x77B0)],
    }
    fl = lambda v: [fb(x) for x in v]
    cases.append({"register": [[dyn_flags[k], dyn_objs[k], tasks[k][0], True] for k in register],
                  "moved": [[o, [W(o + 8 + 4 * j) for j in range(3)]] for o in dyn_objs],
                  "twp_flags": [m.get(tw + 4, "H") for (_, _, tw) in tasks],
                  "player": fl(pos), "pnum": pnum, "stage": stage[0],
                  "task": {**task, "pos": fl(task["pos"])}, "vel": {"v": fl(vel["v"]), "angles": vel["angles"]},
                  "pw": {k: (fl(v) if isinstance(v, list) else (fb(v) if isinstance(v, float) else v)) for k, v in pw.items()},
                  "mw": {"spd": fl(mw["spd"]), "aim": mw["aim"]}, "stuck": stuck, "analog": [analog[0], fb(analog[1])],
                  "f26": fb(f26), "probe": probe, "after": after, "trace": [hex(x) for x in trace]})
print(json.dumps({"head": [count, start], "cols": cols, "objects": objects, "models": models, "rows": rows,
                  "tasks": [[t, rec_out(records[k])] for k, (t, _, _) in enumerate(tasks)], "cases": cases}))
