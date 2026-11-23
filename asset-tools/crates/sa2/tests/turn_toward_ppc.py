"""Local PowerPC oracle for main.dol fn_8002F1C0 (turn toward the target).

Environment: SA2_MAIN_DOL. Prints JSON for tests/mov_control.rs.
"""
import json, os, random, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, f32, bits32  # noqa: E402

if not os.environ.get("SA2_MAIN_DOL"):
    print("SKIP: SA2_MAIN_DOL not set", file=sys.stderr)
    sys.exit(0)
m = Dol(os.environ["SA2_MAIN_DOL"]).machine(boot=True)
task, twp, mwp = 0x81400000, 0x81400100, 0x81400200
m.u.mem_write(task, bytes(0x400))
m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp)
rng = random.Random(int(os.environ.get("SA2_MOV_SEED", "20261120")))
out = []
for _ in range(int(os.environ.get("SA2_MOV_RUNS", "3000"))):
    p = [f32(rng.uniform(-30, 30)) for _ in range(3)]
    t = [f32(rng.uniform(-30, 30)) for _ in range(3)]
    if rng.random() < 0.1:
        t[0], t[2] = p[0], p[2]
    yaw = rng.choice([rng.randrange(-0x20000, 0x20000), 0, 0x8000, -0x8000])
    step = rng.choice([0x100, 0x200, 0x800, 0x4000])
    for k in range(3):
        m.put(twp + 0x14 + 4 * k, "f", p[k])
    m.put(mwp + 0x54, "f", t[0]); m.put(mwp + 0x58, "f", t[1]); m.put(mwp + 0x5C, "f", t[2])
    m.put(twp + 0xC, "i", yaw)
    m.call(0x8002F1C0, r3=task, r4=step)
    out.append({"p": [bits32(x) for x in p], "t": [bits32(x) for x in t], "yaw": yaw, "step": step,
        "aim": m.get(mwp + 0x1C, "i"), "yawOut": m.get(twp + 0xC, "i")})
print(json.dumps(out))
