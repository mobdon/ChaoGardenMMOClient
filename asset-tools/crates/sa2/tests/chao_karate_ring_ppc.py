"""Local PowerPC oracle: karate arena edge fn_72_F5B0 and distance fn_72_13258.

Environment: SA2_MAIN_DOL, SA2_CHAO_MAIN_REL, SA2_CHAO_KARATE_REL (local only);
needs the local Gekko tables (njScalor). Prints JSON for tests/chao_karate.rs.
"""
import json, os, random, struct, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
gekko_tables.require()
from dol_ppc import Dol, f32, bits32  # noqa: E402
from rel_load import load_rel  # noqa: E402

env = os.environ
for k in ("SA2_MAIN_DOL", "SA2_CHAO_MAIN_REL", "SA2_CHAO_KARATE_REL"):
    if not env.get(k) or not Path(env[k]).exists():
        print(f"SKIP: {k} not set", file=sys.stderr)
        sys.exit(0)
m = Dol(env["SA2_MAIN_DOL"]).machine(boot=True)
cm = load_rel(m, Path(env["SA2_CHAO_MAIN_REL"]).read_bytes(), 0x80800000, skip=(61,))
ka = load_rel(m, Path(env["SA2_CHAO_KARATE_REL"]).read_bytes(), 0x80D00000, {60: cm})
NJ_SIN, NJ_COS = 0x80112398, 0x801123B0
trig = {}
H = 0x81400000
m.u.mem_write(H, bytes(0x4000))
task, twp, mwp, work, body, mgr, stage, phase = (H + 0x200 * k for k in range(8))
other, otwp = H + 0x1000, H + 0x1100
m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp); m.put(task + 0x40, "I", work)
m.put(work + 0x18, "I", body)
m.put(ka[6] + 8, "I", mgr); m.put(ka[6] + 0xC, "I", stage); m.put(stage + 0x34, "I", phase)
m.put(other + 0x34, "I", otwp)
m.put(mgr + 0x10, "I", task); m.put(mgr + 0x14, "I", other)
rng = random.Random(int(env.get("SA2_KARATE_SEED", "20261120")))
out = {"edge": [], "distance": []}
for _ in range(int(env.get("SA2_KARATE_RUNS", "2000"))):
    flags = rng.choice([0, 2, 2, 0x102])
    pos = [f32(rng.uniform(-20, 20)), f32(rng.choice([rng.uniform(-1, 12), 7.0, 0.04, 0.05, 10.0])), f32(rng.uniform(-5, 5))]
    bx = f32(rng.choice([rng.uniform(-18, 18), 15.8, 16.0, -16.5]))
    vel = [f32(rng.choice([rng.uniform(-0.2, 0.2), 0.06, 0.1, 0.05])) for _ in range(3)]
    acc = [f32(rng.uniform(-0.1, 0.1)) for _ in range(3)]
    yaw = rng.randrange(0, 0x10000)
    ph = rng.choice([0, 3, 11, 12, 13])
    fighting = rng.choice([2, 2, 3])
    for k in range(3):
        m.put(twp + 0x14 + 4 * k, "f", pos[k]); m.put(mwp + 4 * k, "f", vel[k]); m.put(mwp + 0xC + 4 * k, "f", acc[k])
    m.put(twp + 0xC, "i", yaw); m.put(twp + 4, "H", 0)
    m.put(work + 4, "H", flags); m.put(body, "f", bx)
    m.put(phase, "b", ph); m.put(mgr + 4, "B", fighting); m.put(mgr + 3, "B", 0)
    m.call(ka[1] + 0xF5B0, r3=task)
    ret = m.gpr(3)
    # njSin/njCos of yaw + 0x8000, as the routine computes them, for the replay.
    m.call(NJ_SIN, r3=(yaw + 0x8000) & 0xFFFFFFFF); s = bits32(f32(m.fpr(1)))
    m.call(NJ_COS, r3=(yaw + 0x8000) & 0xFFFFFFFF); c = bits32(f32(m.fpr(1)))
    out["edge"].append({"flags": flags, "pos": [bits32(x) for x in pos], "bodyX": bits32(bx),
        "vel": [bits32(x) for x in vel], "acc": [bits32(x) for x in acc], "phase": ph,
        "fighting": fighting == 2, "sin": s, "cos": c, "ret": ret,
        "flagsOut": m.get(work + 4, "H"), "twpOut": m.get(twp + 4, "H"), "mgr3": m.get(mgr + 3, "B"),
        "velOut": [bits32(m.get(mwp + 4 * k, "f")) for k in range(3)],
        "accOut": [bits32(m.get(mwp + 0xC + 4 * k, "f")) for k in range(3)]})
    b = [f32(rng.uniform(-20, 20)) for _ in range(3)]
    for k in range(3):
        m.put(otwp + 0x14 + 4 * k, "f", b[k])
    m.call(ka[1] + 0x13258)
    out["distance"].append({"a": [bits32(x) for x in pos], "b": [bits32(x) for x in b], "d": bits32(f32(m.fpr(1)))})
print(json.dumps(out))
