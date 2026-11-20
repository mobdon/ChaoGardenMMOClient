"""Local PowerPC oracle: run the GameCube Chao Race routines under Unicorn.

Loads the user's own GSNE8P main.dol, ChaoMain.rel (module 60) and
ChaoStgRace.rel (module 76), relocated against each other, and runs the race
movement, stamina and trip routines with randomized Chao records and a scripted
rand(), plus ChaoMain's race progress recorder. Prints JSON cases that
tests/chao_race.rs replays through src/chao_race.rs.

Environment: SA2_MAIN_DOL, SA2_CHAO_MAIN_REL, SA2_CHAO_RACE_REL (local only).
Optional: SA2_RACE_SEED, SA2_RACE_RUNS.
"""
import json, os, random, struct, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, f32, bits32  # noqa: E402
from rel_load import load_rel  # noqa: E402

MAIN_BASE, RACE_BASE = 0x80800000, 0x80D00000
RAND = 0x8010DC88
# ChaoMain .text
CM_GET_SKILL = 0x20934
CM_PROGRESS_SET, CM_PROGRESS_GET = 0x24ABC, 0x24B6C
CM_RACE_BSS = 0x3A4E8  # ChaoMain .bss lbl_60_bss_3A4E8
# ChaoStgRace .text
RUN, FLY, SWIM, CLIMB, RUN_BHV, RUN_BHV2, START_STAMINA = 0xC3BC, 0xC500, 0xC6EC, 0xC864, 0xD2B4, 0xD578, 0x1974

env = os.environ
for k in ("SA2_MAIN_DOL", "SA2_CHAO_MAIN_REL", "SA2_CHAO_RACE_REL"):
    if not env.get(k) or not Path(env[k]).exists():
        print(f"SKIP: {k} not set", file=sys.stderr)
        sys.exit(0)
dol = Dol(env["SA2_MAIN_DOL"])
main_rel = Path(env["SA2_CHAO_MAIN_REL"]).read_bytes()
race_rel = Path(env["SA2_CHAO_RACE_REL"]).read_bytes()
assert struct.unpack_from(">I", main_rel, 0)[0] == 60 and struct.unpack_from(">I", race_rel, 0)[0] == 76


m = dol.machine(boot=False)
cm = load_rel(m, main_rel, MAIN_BASE, skip=(61,))
rc = load_rel(m, race_rel, RACE_BASE, {60: cm}, skip=(77, 78, 79))  # race land modules
T = lambda off: rc[1] + off
CM = lambda off: cm[1] + off

# The REL's own relocations name the callee of each `bl`; read them so the
# stubs hit the exact targets (main.dol imports are absolute addresses).
def rel_targets(data, addrs, section=1):
    w = lambda o: struct.unpack_from(">I", data, o)[0]
    own = w(0); out = {}
    for k in range(w(0x2C) // 8):
        mod, at = struct.unpack_from(">II", data, w(0x28) + 8 * k)
        sec, off = None, 0
        while True:
            delta, kind, s, add = struct.unpack_from(">HBBI", data, at); at += 8
            if kind == 203: break
            off += delta
            if kind == 202: sec, off = s, 0; continue
            if kind == 201: continue
            if sec == section and kind == 10:
                out[off] = (mod, s, add)
    return out
calls_at = rel_targets(race_rel, rc)
def callee(site):
    mod, s, add = calls_at[site]
    return add if mod == 0 else (cm[s] + add if mod == 60 else rc[s] + add)

HEAP = 0x81400000
cursor = [HEAP]
def alloc(n):
    a = cursor[0]; cursor[0] += (n + 0x3F) & ~0x1F
    m.u.mem_write(a, bytes(n)); return a

rands, events = [], []
skill = [0]
trig = [0.0, 0.0]
def stub(addr, fn): m.stub(addr, fn)
def noop(mm): pass
stub(RAND, lambda mm: mm.gpr(3, rands.pop(0)))
stub(CM(CM_GET_SKILL), lambda mm: mm.gpr(3, skill[0]))
stub(callee(0xC4A0), lambda mm: mm.fpr(1, trig[0]))   # njSin
stub(callee(0xC4B0), lambda mm: mm.fpr(1, trig[1]))   # njCos
stub(callee(0xC4C0), lambda mm: mm.fpr(1, 0.0))       # njScalor (motion speed only)
stub(callee(0xC4D8), lambda mm: events.append(["spd", bits32(f32(mm.fpr(1)))]))  # AL_SetMotionSpd
stub(callee(0xD44C), lambda mm: events.append(["bhv", mm.gpr(4) - rc[1]]))       # AL_SetBehavior
for target in {callee(site) for site in (0xD314, 0xD330, 0xD35C, 0xD4D8, 0xD500, 0xD5E4, 0xD600, 0xD724, 0xD74C)}:
    stub(target, noop)  # AL_misc_2, face, SetMotionLink, MOV_ClearVelo, fn_8002F1C0
stub(T(0xF4B8), noop)

def fput(a, v): m.put(a, "f", v)
def fget(a): return bits32(m.get(a, "f"))

def chao(stamina, turbo):
    """task: +0x34 chao work (+0xC yaw, +0x5C param, +0x88 stamina, +0xA0 flags), +0x38 movement."""
    task, work, mwp, param = alloc(0x60), alloc(0x300), alloc(0x100), alloc(0x200)
    m.put(task + 0x34, "I", work); m.put(task + 0x38, "I", mwp)
    m.put(work + 0x5C, "I", param); m.put(work + 0x88, "i", stamina)
    m.put(work + 0xA0, "I", 0x100 if turbo else 0)
    return task, work, mwp, param

seed = int(env.get("SA2_RACE_SEED", "20261120"))
runs = int(env.get("SA2_RACE_RUNS", "300"))
rng = random.Random(seed)
def sk():
    return rng.choice([0, 1, 150, 151, 3000, 9999, 10000, 12000, 29997]) if rng.random() < 0.3 else rng.randrange(0, 30000)
def fv(lo, hi): return f32(rng.uniform(lo, hi))
out = {"seed": seed, "move": [], "behaviour": [], "progress": []}
for case in range(runs):
    cursor[0] = HEAP
    kind = rng.choice(["run", "fly", "swim", "climb"])
    stamina = rng.choice([0, 0, 1, 20, 10000, rng.randrange(0, 160000)])
    turbo = rng.random() < 0.4
    task, work, mwp, param = chao(stamina, turbo)
    skill[0] = sk()
    trig[:] = [fv(-1, 1), fv(-1, 1)]
    vel = [fv(-2, 2) for _ in range(3)]; pos = [fv(-100, 100) for _ in range(3)]
    for k in range(3): fput(mwp + 4 * k, vel[k]); fput(work + 0x14 + 4 * k, pos[k])
    g = fv(0, 0.2); fput(mwp + 0x48, g)
    events.clear()
    m.call(T({"run": RUN, "fly": FLY, "swim": SWIM, "climb": CLIMB}[kind]), r3=task)
    out["move"].append({"kind": kind, "skill": skill[0], "stamina": stamina, "turbo": turbo,
        "sin": bits32(trig[0]), "cos": bits32(trig[1]), "vel": [bits32(v) for v in vel], "pos": [bits32(v) for v in pos],
        "gravity": bits32(g), "accOut": [fget(mwp + o) for o in (0xC, 0x10, 0x14)],
        "velOut": [fget(mwp + o) for o in (0, 4, 8)], "posOut": [fget(work + 0x14 + o) for o in (0, 4, 8)],
        "motionSpeed": [e[1] for e in events if e[0] == "spd"]})

    # Run behaviours (fn_76_D2B4 / fn_76_D578), past their first step.
    cursor[0] = HEAP
    which = rng.choice([RUN_BHV, RUN_BHV2])
    stamina = rng.choice([0, 1, 19, 20, 21, 5000])
    task, work, mwp, param = chao(stamina, False)
    bhv = work + 0x1B4
    m.put(bhv + 2, "H", 2)
    counter = rng.choice([0, 6, 12, 50, 100, 600]) if rng.random() < 0.5 else rng.randrange(0, 2000)
    m.put(bhv + 0xC, "i", counter)
    dd = rng.choice([0, 3])
    m.put(param + 0xDD, "B", dd)
    m.put(mwp + 0x40, "H", 0)
    skill[0] = sk()
    rr = [rng.choice([0, 1, 32, 33, 100, 32767]) if rng.random() < 0.5 else rng.randrange(0, 32768)]
    rands[:] = list(rr); events.clear()
    m.call(T(which), r3=task)
    out["behaviour"].append({"which": "D2B4" if which == RUN_BHV else "D578", "skill": skill[0], "counter": counter,
        "dd": dd, "stamina": stamina, "rand": rr, "used": 1 - len(rands),
        "trip": any(e[0] == "bhv" for e in events), "staminaOut": m.get(work + 0x88, "i"),
        "counterOut": m.get(bhv + 0xC, "i")})

# Race start stamina, fn_76_1764 0x1974..0x199C (GetSkill(4) * 5 + 10000).
out["start"] = []
for s in [0, 1, 999, 3000, 9999, 29997, 65535]:
    cursor[0] = HEAP
    task, work, mwp, param = chao(0, False)
    skill[0] = s
    m.gpr(1, 0x803C1450 - 0x400); m.gpr(2, 0x803B6520); m.gpr(13, 0x803B52C0)
    m.gpr(30, task)
    m.u.emu_start(T(START_STAMINA), T(0x19A0), count=10000)
    out["start"].append([s, m.get(work + 0x88, "i"), m.get(work + 0x8C, "i")])

# ChaoMain race progress: fn_60_24ABC(course, level) then fn_60_24B6C(course).
bss = cm[6] + CM_RACE_BSS
for case in range(runs):
    course = rng.randrange(-1, 14)
    old = rng.randrange(-1, 13)
    level = rng.randrange(-1, 14)
    if 0 <= course < 13:
        m.put(bss + 0x13 + course, "b", old)
    m.call(CM(CM_PROGRESS_SET), r3=course & 0xFFFFFFFF, r4=level & 0xFFFFFFFF)
    m.call(CM(CM_PROGRESS_GET), r3=course & 0xFFFFFFFF)
    got = struct.unpack(">i", struct.pack(">I", m.gpr(3)))[0]
    out["progress"].append({"course": course, "old": old, "level": level, "after": got})
print(json.dumps(out))
