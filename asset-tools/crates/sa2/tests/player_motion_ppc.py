"""Run main.dol's player motion step fn_8003D8D0 on random motion works and
action tables and print JSON for tests/player_motion.rs. The blend
routines (fn_8003D678, fn_8003C908) and the allocator are stubbed and
logged.

Needs SA2_MAIN_DOL (GSNE8P main.dol, never committed).
"""
import json
import os
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402

m = Dol(os.environ["SA2_MAIN_DOL"]).machine(boot=False)
MW, ACTS, SA, SB, MOTS, BUF = 0x81100000, 0x81110000, 0x81120000, 0x81120010, 0x81130000, 0x81140000
ANIMS, MODELS = 0x801E5D28, 0x801E668C
rng = random.Random(0x3D8D0)
fb = lambda x: struct.unpack(">I", struct.pack(">f", x))[0]
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
log = []


def d678(mm):
    log.append(["blend", mm.get(MW + 4, "h"), mm.get(MW + 2, "h")])


def c908(mm):
    log.append(["free", mm.get(MW + 2, "h")])


def malloc(mm):
    log.append(["malloc", mm.gpr(3)])
    mm.gpr(3, BUF)


def calloc(mm):
    log.append(["calloc", mm.gpr(3), mm.gpr(4)])
    mm.gpr(3, BUF + 0x100)


def free(mm):
    log.append(["syfree", mm.gpr(3)])


m.stub(0x8003D678, d678)
m.stub(0x8003C908, c908)
m.stub(0x801197D0, malloc)
m.stub(0x801197FC, calloc)
m.stub(0x80119828, free)
NMOT, NMOD, NACT = 6, 3, 10
cases = []
for _ in range(20000):
    frames = [rng.choice([2, 3, 10, 30, 60, rng.randrange(2, 200)]) for _ in range(NMOT)]
    keys = [rng.choice([1, 2]) for _ in range(NMOD)]
    acts = []
    for _ in range(NACT):
        acts.append([rng.randrange(NMOT), rng.randrange(NMOD), rng.choice(list(range(16)) + [3, 4, 5, 9, 0xA, 0xB]),
                     rng.choice([rng.randrange(NACT), rng.randrange(NACT) | 0x100]),
                     f32(rng.choice([1.0, rng.uniform(0.05, 0.6)])), f32(rng.choice([1.0, 0.5, rng.uniform(-2, 3)]))])
    cur = rng.randrange(NACT)
    req = rng.choice([cur, cur, rng.randrange(NACT)])
    nf = frames[acts[cur][0]]
    w = {"mode": rng.choice(list(range(16)) + [1, 2, 3, 3, 0x10]), "req": req, "act": cur, "last": rng.randrange(NACT),
         "next": rng.randrange(NACT), "timer": rng.randrange(0, 5), "flags": rng.choice([0, 4, 1, 7]),
         "frame": f32(rng.choice([0.0, rng.uniform(-3, nf + 3), nf - 1, rng.uniform(0, 1.2)])),
         "last_frame": f32(rng.uniform(0, 9)), "start": f32(rng.uniform(0, 9)),
         "sa": f32(rng.uniform(0, 3)), "sb": f32(rng.uniform(0, 3)), "buf": rng.random() < 0.8}
    for k, fr in enumerate(frames):
        m.put(ANIMS + 8 * k + 4, "I", MOTS + 0x10 * k)
        m.put(MOTS + 0x10 * k + 4, "I", fr)
    for k, kv in enumerate(keys):
        m.put(MODELS + 8 * k, "I", kv)
    for k, a in enumerate(acts):
        # Entries 0x100 + k mirror k, for the unmasked +6 reads.
        m.u.mem_write(ACTS + 0x10 * k, struct.pack(">4h2f", *a))
        m.u.mem_write(ACTS + 0x10 * (0x100 + k), struct.pack(">4h2f", *a))
    m.u.mem_write(MW, struct.pack(">6hI3fIII", w["mode"], w["req"], w["act"], w["last"], w["next"], w["timer"],
                                  (w["flags"] & 0xFFFF) << 16, w["frame"], w["last_frame"], w["start"], SA, SB, ACTS))
    m.put(MW + 0x28, "I", BUF if w["buf"] else 0)
    m.put(BUF, "I", BUF + 0x100)
    m.put(SA, "f", w["sa"])
    m.put(SB, "f", w["sb"])
    log.clear()
    m.call(0x8003D8D0, r3=MW)
    out = struct.unpack(">6hhh3I", m.u.mem_read(MW, 0x1C))
    cases.append({"frames": frames, "keys": keys, "acts": [a[:4] + [fb(a[4]), fb(a[5])] for a in acts],
                  "w": {**w, "frame": fb(w["frame"]), "last_frame": fb(w["last_frame"]), "start": fb(w["start"]),
                        "sa": fb(w["sa"]), "sb": fb(w["sb"])},
                  "out": list(out[:7]) + list(out[8:]), "buf_after": m.get(MW + 0x28, "I") != 0, "log": log[:]})
print(json.dumps({"cases": cases}))
