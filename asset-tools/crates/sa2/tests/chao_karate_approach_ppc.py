"""Local PowerPC oracle: drive the karate approach handler fn_72_D110 frame by frame.

Runs fn_72_C0C4 (motions 4, 7) and fn_72_C3B4 (motions 5, 8) from the user's
GSNE8P ChaoStgKarate.rel (with ChaoMain.rel and main.dol) under Unicorn with a
scripted motion engine (frame, link-in-progress flag, IsMotionStop) and prints
JSON; tests/chao_karate.rs replays it through src/chao_karate.rs.

Environment: SA2_MAIN_DOL, SA2_CHAO_MAIN_REL, SA2_CHAO_KARATE_REL (local only).
"""
import json, os, random, struct, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, f32, bits32  # noqa: E402
from rel_load import load_rel  # noqa: E402

env = os.environ
for k in ("SA2_MAIN_DOL", "SA2_CHAO_MAIN_REL", "SA2_CHAO_KARATE_REL"):
    if not env.get(k) or not Path(env[k]).exists():
        print(f"SKIP: {k} not set", file=sys.stderr)
        sys.exit(0)
dol = Dol(env["SA2_MAIN_DOL"])
main_rel = Path(env["SA2_CHAO_MAIN_REL"]).read_bytes()
kar_rel = Path(env["SA2_CHAO_KARATE_REL"]).read_bytes()
m = dol.machine(boot=True)
cm = load_rel(m, main_rel, 0x80800000, skip=(61,))
ka = load_rel(m, kar_rel, 0x80D00000, {60: cm})


def rel24_targets(data):
    w = lambda o: struct.unpack_from(">I", data, o)[0]
    out = {}
    for k in range(w(0x2C) // 8):
        mod, at = struct.unpack_from(">II", data, w(0x28) + 8 * k)
        sec, off = None, 0
        while True:
            delta, kind, s, add = struct.unpack_from(">HBBI", data, at); at += 8
            if kind == 203: break
            off += delta
            if kind == 202: sec, off = s, 0; continue
            if kind == 201: continue
            if sec == 1 and kind == 10:
                out[off] = add if mod == 0 else (cm if mod == 60 else ka)[s] + add
    return out


calls_at = rel24_targets(kar_rel)
calls, state = [], {"stopped": 0}
hooked = set()


def stub(site, fn):
    target = calls_at[site]
    if target not in hooked:
        hooked.add(target)
        m.stub(target, fn)


# fn_72_D110 call sites (resolved through the REL's own relocations).
yaw_next = [0]
dist = [10.0]
NJ_SIN, NJ_COS = 0x80112398, 0x801123B0
stub(0xD1B0, lambda mm: calls.append(["link", mm.gpr(4), mm.gpr(5) & 0xFFFF]))
stub(0xD1E4, lambda mm: calls.append(["sound", mm.gpr(3)]))
stub(0xD3FC, lambda mm: None)
stub(0xD2D4, lambda mm: mm.put(mm.get(mm.gpr(3) + 0x34, "I") + 0xC, "i", yaw_next[0]))
m.stub(ka[1] + 0x13258, lambda mm: mm.fpr(1, dist[0]))  # fn_72_13258 (same-module call)
for site in (0xD20C, 0xD238, 0xD274, 0xD298):
    if site in calls_at:
        stub(site, lambda mm: mm.gpr(3, 0))

HEAP = 0x81400000
m.u.mem_write(HEAP, bytes(0x2000))
task, twp, mwp, work, aux, chao, ctwp, ctrl = (HEAP + 0x100 * k for k in range(8))
m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp); m.put(task + 0x40, "I", work)
m.put(work + 8, "I", chao); m.put(work + 0xC, "I", aux)
m.put(chao + 0x34, "I", ctwp); m.put(aux + 0x24, "I", ctrl)
m2 = Dol(env["SA2_MAIN_DOL"]).machine(boot=True)

rng = random.Random(int(env.get("SA2_KARATE_SEED", "20261120")))
out = []
for case in range(int(env.get("SA2_KARATE_RUNS", "300"))):
    for o in range(0, 0x30, 4):
        if o != 0x24:
            m.put(aux + o, "I", 0)
    m.put(aux, "b", 3)
    m.put(twp + 4, "H", rng.choice([0, 0x8000]))
    for k in range(6):
        m.put(mwp + 4 * k, "f", f32(rng.uniform(-0.5, 0.5)))
    link_left = rng.randrange(0, 5)
    frames = []
    d = rng.uniform(4, 12)
    for t in range(60):
        linking = link_left > 0
        link_left -= 1
        yaw_next[0] = rng.randrange(-0x10000, 0x10000)
        d -= rng.uniform(0, 0.6)
        dist[0] = f32(d)
        m.put(ctrl, "H", 2 if linking else 0)
        calls.clear()
        vel_in = [bits32(m.get(mwp + 4 * k, "f")) for k in range(6)]
        m.call(ka[1] + 0xD110, r3=task)
        m2.call(NJ_SIN, r3=yaw_next[0] & 0xFFFFFFFF); s = bits32(f32(m2.fpr(1)))
        m2.call(NJ_COS, r3=yaw_next[0] & 0xFFFFFFFF); c = bits32(f32(m2.fpr(1)))
        frames.append({"linking": linking, "dist": bits32(dist[0]), "sin": s, "cos": c, "in": vel_in,
            "calls": list(calls), "aux": [m.get(aux + 1, "b"), m.get(aux + 8, "H"), m.get(aux + 0xC, "i")],
            "twp": m.get(twp + 4, "H"), "out": [bits32(m.get(mwp + 4 * k, "f")) for k in range(6)]})
    out.append({"frames": frames})

# fn_72_B720 (ready/idle motions 0, 1, 2, 13-17, 36, 41).
stub(0xB7B0, lambda mm: calls.append(["link", mm.gpr(4), mm.gpr(5) & 0xFFFF]))
idle = []
for case in range(int(env.get("SA2_KARATE_RUNS", "300"))):
    motion = rng.choice([0, 1, 2, 13, 14, 15, 16, 17, 36, 41])
    for o in range(0, 0x30, 4):
        if o != 0x24:
            m.put(aux + o, "I", 0)
    m.put(aux, "b", motion); m.put(aux + 5, "B", rng.choice([0, 8, 0x10]))
    m.put(twp + 4, "H", rng.choice([0, 0x8000]))
    bits0 = m.get(aux + 5, "B")
    link_left = rng.randrange(1, 5)
    frames = []
    for t in range(8):
        linking = link_left > 0
        link_left -= 1
        m.put(ctrl, "H", 2 if linking else 0)
        calls.clear()
        m.call(ka[1] + 0xB720, r3=task)
        frames.append({"linking": linking, "calls": list(calls), "step": m.get(aux + 1, "b"),
            "bits": m.get(aux + 5, "B"), "row": m.get(aux + 0xC, "i"), "twp": m.get(twp + 4, "H")})
    idle.append({"motion": motion, "bits0": bits0, "frames": frames})
print(json.dumps({"approach": out, "idle": idle}))
