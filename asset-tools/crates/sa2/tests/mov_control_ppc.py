"""Local PowerPC oracle for main.dol MOV_Control (0x8002F448).

Runs the user's GSNE8P main.dol under Unicorn on randomized movement blocks
(CCL_IsPushed 0x80006774 scripted) and prints JSON for tests/mov_control.rs.
njScalor uses Gekko's estimates, so the local tables are required.
Environment: SA2_MAIN_DOL; optional SA2_MOV_SEED, SA2_MOV_RUNS.
"""
import json, os, random, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
gekko_tables.require()
from dol_ppc import Dol, f32, bits32  # noqa: E402

if not os.environ.get("SA2_MAIN_DOL"):
    print("SKIP: SA2_MAIN_DOL not set", file=sys.stderr)
    sys.exit(0)
m = Dol(os.environ["SA2_MAIN_DOL"]).machine(boot=False)
pushed = [0]
m.stub(0x80006774, lambda mm: mm.gpr(3, pushed[0]))
task, twp, mwp = 0x81400000, 0x81400100, 0x81400200
m.u.mem_write(task, bytes(0x400))
m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp)
rng = random.Random(int(os.environ.get("SA2_MOV_SEED", "20261120")))
out = []
for _ in range(int(os.environ.get("SA2_MOV_RUNS", "3000"))):
    small = rng.random() < 0.5
    v = [f32(rng.uniform(-0.012, 0.012) if small else rng.uniform(-3, 3)) for _ in range(3)]
    a = [f32(rng.choice([0.0, rng.uniform(-0.01, 0.01), rng.uniform(-1, 1)])) for _ in range(3)]
    p = [f32(rng.uniform(-50, 50)) for _ in range(3)]
    g = f32(rng.choice([0.0, -0.05, rng.uniform(-0.2, 0.0)]))
    flags = rng.choice([0, 8, 0x18, 0x408, 0xC08, 0x418, 0x10, 0x400])
    pushed[0] = rng.choice([0, 0, 1])
    for k in range(3):
        m.put(mwp + 4 * k, "f", v[k]); m.put(mwp + 0xC + 4 * k, "f", a[k]); m.put(twp + 0x14 + 4 * k, "f", p[k])
    m.put(mwp + 0x48, "f", g); m.put(mwp + 0x40, "H", flags)
    m.call(0x8002F448, r3=task)
    out.append({"v": [bits32(x) for x in v], "a": [bits32(x) for x in a], "p": [bits32(x) for x in p],
        "g": bits32(g), "flags": flags, "pushed": pushed[0],
        "vOut": [bits32(m.get(mwp + 4 * k, "f")) for k in range(3)],
        "aOut": [bits32(m.get(mwp + 0xC + 4 * k, "f")) for k in range(3)],
        "pOut": [bits32(m.get(twp + 0x14 + 4 * k, "f")) for k in range(3)],
        "flagsOut": m.get(mwp + 0x40, "H")})
print(json.dumps(out))
