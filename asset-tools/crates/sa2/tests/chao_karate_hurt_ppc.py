"""Local PowerPC oracle: drive the karate hurt motion handler fn_72_C7A8 frame by frame.

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
m = dol.machine(boot=False)
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


flags_in = {"ended": 0, "stopped": 0}
stub(0xC900, lambda mm: calls.append(["link", mm.gpr(4), mm.gpr(5) & 0xFFFF]))
stub(0xC968, lambda mm: None)
stub(0xCA10, lambda mm: calls.append(["sound", mm.gpr(3)]))
stub(0xCC64, lambda mm: mm.gpr(3, flags_in["ended"]))
stub(0xCE6C, lambda mm: mm.gpr(3, flags_in["stopped"]))
for site in (0xC864, 0xC890, 0xC8CC, 0xC8F0):
    stub(site, lambda mm: mm.gpr(3, 0))

HEAP = 0x81400000
m.u.mem_write(HEAP, bytes(0x2000))
task, twp, mwp, work, aux, chao, ctwp, ctrl = (HEAP + 0x100 * k for k in range(8))
m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp); m.put(task + 0x40, "I", work)
m.put(work + 8, "I", chao); m.put(work + 0xC, "I", aux)
m.put(chao + 0x34, "I", ctwp); m.put(aux + 0x24, "I", ctrl)
rng = random.Random(int(env.get("SA2_KARATE_SEED", "20261120")))
out = []
for case in range(int(env.get("SA2_KARATE_RUNS", "400"))):
    motion = rng.choice([9, 10, 11, 29, 30, 31])
    for o in range(0, 0x30, 4):
        if o != 0x24:
            m.put(aux + o, "I", 0)
    m.put(aux, "b", motion)
    knock = [f32(rng.uniform(-1, 1)) for _ in range(3)]
    for k in range(3):
        m.put(aux + 0x18 + 4 * k, "f", knock[k])
    for k in range(6):
        m.put(mwp + 4 * k, "f", f32(rng.uniform(-0.5, 0.5)))
    tw = rng.choice([0, 0x1000, 0x8000])
    m.put(twp + 4, "H", tw)
    wf = rng.choice([0, 0, 1])
    m.put(work + 4, "H", wf)
    link_left = rng.randrange(0, 4)
    frames = []
    for t in range(30):
        linking = link_left > 0
        link_left -= 1
        if rng.random() < 0.15:
            link_left = rng.randrange(0, 3)
        flags_in["ended"] = int(rng.random() < 0.25)
        flags_in["stopped"] = int(rng.random() < 0.25)
        m.put(ctrl, "H", 2 if linking else 0)
        calls.clear()
        vin = [bits32(m.get(mwp + 4 * k, "f")) for k in range(6)]
        m.call(ka[1] + 0xC7A8, r3=task)
        frames.append({"linking": linking, "ended": bool(flags_in["ended"]), "stopped": bool(flags_in["stopped"]),
            "in": vin, "calls": list(calls),
            "aux": [m.get(aux, "b"), m.get(aux + 1, "b"), m.get(aux + 2, "b"), m.get(aux + 8, "H"), m.get(aux + 0xC, "i")],
            "twp": m.get(twp + 4, "H"), "out": [bits32(m.get(mwp + 4 * k, "f")) for k in range(6)]})
    out.append({"motion": motion, "knock": [bits32(x) for x in knock], "twp0": tw, "work": wf, "frames": frames})
print(json.dumps(out))
