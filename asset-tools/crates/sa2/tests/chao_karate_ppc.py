"""Local PowerPC oracle: run the GameCube Chao Karate routines under Unicorn.

Loads the user's own GSNE8P main.dol, ChaoMain.rel (module 60) and
ChaoStgKarate.rel (module 72), relocated against each other, then runs the
karate fighter routines with randomized fighter records and a scripted rand().
Prints JSON cases; tests/chao_karate.rs replays the same inputs through
src/chao_karate.rs and compares every output bit for bit.

Environment: SA2_MAIN_DOL, SA2_CHAO_MAIN_REL, SA2_CHAO_KARATE_REL (all local,
never committed). Optional: SA2_KARATE_SEED, SA2_KARATE_RUNS.
Addresses are ChaoStgKarate .text offsets unless prefixed CM: (ChaoMain).
"""
import json, os, random, struct, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, f32, bits32  # noqa: E402
from rel_load import load_rel  # noqa: E402
from unicorn import ppc_const as R  # noqa: E402

MAIN_BASE, KARATE_BASE = 0x80800000, 0x80D00000
RAND, SE_CALL = 0x8010DC88, 0x8006B978
CM_GET_SKILL, CM_22E10 = 0x20934, 0x22E10
# ChaoStgKarate .text
INIT_SKILLS, INIT_FLAGS_END = 0x112DC, 0x11A3C
TAKE_HIT, HIT_CHECK, ZEAL, CHARGE, ATTACK, LEADER = 0x1010C, 0xEA20, 0xEDE8, 0xFAB8, 0xFE9C, 0x133CC
E9C4, B620, F131EC, E9F8, F137D4, F13258 = 0xE9C4, 0xB620, 0x131EC, 0xE9F8, 0x137D4, 0x13258
NJ_UNIT, F2F1C0 = 0x80115D24, 0x8002F1C0
# ChaoStgKarate .bss: lbl_72_bss_8 (match manager pointer), lbl_72_bss_C (stage manager pointer)
BSS8, BSSC = 0x8, 0xC

env = os.environ
for k in ("SA2_MAIN_DOL", "SA2_CHAO_MAIN_REL", "SA2_CHAO_KARATE_REL"):
    if not env.get(k) or not Path(env[k]).exists():
        print(f"SKIP: {k} not set", file=sys.stderr)
        sys.exit(0)
dol = Dol(env["SA2_MAIN_DOL"])
main_rel = Path(env["SA2_CHAO_MAIN_REL"]).read_bytes()
kar_rel = Path(env["SA2_CHAO_KARATE_REL"]).read_bytes()
assert struct.unpack_from(">I", main_rel, 0)[0] == 60 and struct.unpack_from(">I", kar_rel, 0)[0] == 72

m = dol.machine(boot=False)
cm = load_rel(m, main_rel, MAIN_BASE, skip=(61,))
ka = load_rel(m, kar_rel, KARATE_BASE, {60: cm})
T = lambda off: ka[1] + off
BSS = ka[6] if 6 in ka else None
assert BSS is not None

# Scratch memory for task records (inside MEM1, above the loaded modules).
HEAP = 0x81400000
cursor = [HEAP]
def alloc(n):
    a = cursor[0]; cursor[0] += (n + 0x3F) & ~0x1F
    m.u.mem_write(a, bytes(n)); return a

rands = []
calls = []
def stub_rand(mm):
    mm.gpr(3, rands.pop(0))
def ret(value=None, f=None):
    def fn(mm):
        if value is not None: mm.gpr(3, value)
        if f is not None: mm.fpr(1, f)
    return fn
skills = [0] * 8
def stub_skill(mm):
    mm.gpr(3, skills[mm.gpr(4) & 0xFFFF] if (mm.gpr(4) & 0xFFFF) < 8 else 0)
def record(name):
    def fn(mm): calls.append([name, mm.gpr(4)])
    return fn
attacker = [0]
leader = [0]
distance = [0.0]
m.stub(RAND, stub_rand)
m.stub(SE_CALL, ret())
m.stub(cm[1] + CM_GET_SKILL, stub_skill)
m.stub(cm[1] + CM_22E10, ret())
m.stub(T(E9C4), lambda mm: mm.gpr(3, attacker[0]))
m.stub(T(B620), record("motion"))
m.stub(T(F131EC), ret())
m.stub(T(E9F8), ret(0))
m.stub(T(F137D4), ret())
m.stub(T(F13258), lambda mm: mm.fpr(1, distance[0]))
m.stub(NJ_UNIT, ret())
m.stub(T(LEADER), lambda mm: mm.gpr(3, leader[0]))
m.stub(F2F1C0, ret())

def run_range(begin, end, **regs):
    m.gpr(1, 0x803C1450 - 0x400)
    m.gpr(2, 0x803B6520); m.gpr(13, 0x803B52C0)
    for k, v in regs.items(): m.gpr(int(k[1:]), v)
    m.u.emu_start(begin, end, count=2_000_000)
    assert m.u.reg_read(R.UC_PPC_REG_PC) == end

def fput(a, v): m.put(a, "f", v)
def fget(a): return bits32(m.get(a, "f"))

def fighter():
    """A fighter task: +0x34 twp (state record), +0x38 mwp, +0x40 work."""
    task, twp, mwp, work, aux = alloc(0x60), alloc(0x40), alloc(0x80), alloc(0x60), alloc(0x40)
    m.put(task + 0x34, "I", twp); m.put(task + 0x38, "I", mwp); m.put(task + 0x40, "I", work)
    m.put(work + 0xC, "I", aux)
    return task, twp, mwp, work, aux

seed = int(env.get("SA2_KARATE_SEED", "20261120"))
runs = int(env.get("SA2_KARATE_RUNS", "200"))
rng = random.Random(seed)
def skill():
    return rng.choice([0, 1, 9, 10, 100, 500, 999, 1000, 1500, 1599, 1600, 2000, 3999, 4000, 9999]) if rng.random() < 0.3 else rng.randrange(0, 4001)
def rnd():
    return rng.choice([0, 1, 3276, 16383, 16384, 29490, 32767]) if rng.random() < 0.2 else rng.randrange(0, 32768)
def fval(lo, hi):
    return f32(rng.uniform(lo, hi))

out = {"seed": seed, "init": [], "hit": [], "check": [], "zeal": [], "charge": [], "attack": [], "leader": [], "rand": []}
mgr = alloc(0x40)
stage = alloc(0x40); stage_phase = alloc(0x10)
m.put(BSS + BSS8, "I", mgr); m.put(BSS + BSSC, "I", stage)
m.put(stage + 0x34, "I", stage_phase)

for case in range(runs):
    # --- fighter setup arithmetic, 0x112DC..0x11A3C --------------------------------
    cursor[0] = HEAP + 0x10000
    task, twp, mwp, work, aux = fighter()
    chao, ctwp, param = alloc(0x40), alloc(0x40), alloc(0x200)
    m.put(chao + 0x34, "I", ctwp); m.put(ctwp + 0x5C, "I", param)
    m.put(work + 8, "I", chao)
    cpu = rng.random() < 0.6
    m.put(work + 4, "H", 4 if cpu else 0)
    ctype = rng.randrange(0, 6); d6 = rng.choice([0, 0, 1])
    m.put(param + 0x80, "B", ctype); m.put(param + 0xD6, "B", d6)
    skills[:] = [skill() for _ in range(7)] + [0]
    rr = [rnd() for _ in range(6)]
    rands[:] = list(rr)
    run_range(T(INIT_SKILLS), T(INIT_FLAGS_END), r31=work, r29=task)
    out["init"].append({"skills": skills[:7], "cpu": cpu, "type": ctype, "d6": d6, "rand": rr,
        "params": [fget(work + o) for o in range(0x20, 0x48, 4)], "flags": m.get(work + 4, "H")})

    # --- take hit, fn_72_1010C (sub 0 then sub 1) -----------------------------------
    cursor[0] = HEAP + 0x20000
    d_task, d_twp, d_mwp, d_work, d_aux = fighter()
    a_task, a_twp, a_mwp, a_work, a_aux = fighter()
    attacker[0] = a_task
    power = fval(1200, 5200) if rng.random() < 0.9 else rng.choice([1200.0, 1600.0, 5200.0])
    fput(a_work + 0x2C, power)
    swim = fval(1000, 5000); hpmax = fval(1000, 5000)
    hp = f32(hpmax * rng.choice([1.0, rng.random(), 0.05, 0.01]))
    zeal = fval(0, 1000) if rng.random() < 0.8 else rng.choice([0.0, 1000.0, 10.0])
    runf = f32((1000 + 1.5 * rng.randrange(0, 4001)) / 1000)
    pwr_d = fval(1200, 5200) if rng.random() < 0.7 else rng.choice([1599.0, 1600.0, 1601.0])
    charge = fval(0, 500)
    kind = rng.choice([-1, -1, -1, 1, 2, 3, 4, 5, 6])
    for o, v in ((0x20, swim), (0x28, runf), (0x2C, pwr_d), (0x38, zeal), (0x3C, hp), (0x44, hpmax), (0x48, charge)):
        fput(d_work + o, v)
    m.put(d_work + 2, "h", 1); m.put(d_work + 6, "h", kind)
    tflags = rng.choice([0, 0x800, 0x400, 0x1000])
    m.put(d_twp + 4, "h", tflags); m.put(d_twp + 1, "B", 0)
    phase = rng.choice([2, 2, 3])
    m.put(mgr + 4, "B", phase); m.put(mgr + 3, "B", 0)
    rr = [rnd() for _ in range(6)]
    rands[:] = list(rr); calls.clear()
    m.call(T(TAKE_HIT), r3=d_task)
    used0 = 6 - len(rands)
    mid = {"hp": fget(d_work + 0x3C), "zeal": fget(d_work + 0x38), "flags": m.get(d_twp + 4, "H"),
           "sub": m.get(d_twp + 1, "B"), "mgr3": m.get(mgr + 3, "B"), "motions": [c[1] for c in calls]}
    motion_end = rng.random() < 0.7
    if motion_end:
        m.put(d_twp + 4, "H", m.get(d_twp + 4, "H") | 0x8000)
    m.put(d_twp + 0, "B", 6)
    calls.clear()
    m.call(T(TAKE_HIT), r3=d_task)
    out["hit"].append({"power": bits32(power), "swim": bits32(swim), "hpmax": bits32(hpmax), "hp": bits32(hp),
        "zeal": bits32(zeal), "run": bits32(runf), "defPower": bits32(pwr_d), "charge": bits32(charge),
        "kind": kind, "twpFlags": tflags, "phase": phase, "rand": rr[:6 - len(rands)], "used0": used0, "motionEnd": motion_end,
        "after0": mid, "after1": {"charge": fget(d_work + 0x48), "state": m.get(d_twp, "B"),
            "sub": m.get(d_twp + 1, "B"), "flags": m.get(d_twp + 4, "H")}})

    # --- hit check, fn_72_EA20 ------------------------------------------------------
    cursor[0] = HEAP + 0x30000
    t, tw, mw, wk, ax = fighter()
    st = rng.choice([2, 2, 2, 4, 7, 3, 6])
    kind = rng.choice([-1, -1, 5, 1, 3])
    fly = fval(1000, 5000); acc = fval(500, 4000) if rng.random() < 0.9 else 0.0
    distance[0] = f32(rng.choice([0.0, 1.0, 4.9, 5.0, 5.1, 9.0, rng.uniform(0, 8)]))
    cflags = rng.choice([0, 0x200, 0x100])
    m.put(tw, "B", st); m.put(tw + 4, "h", cflags); m.put(wk + 6, "h", kind)
    fput(wk + 0x24, fly)
    m.put(stage_phase, "B", rng.choice([0, 1, 9]))
    rr = [rnd()]
    rands[:] = list(rr)
    m.call(T(HIT_CHECK), r3=t, f1=acc)
    out["check"].append({"state": st, "kind": kind, "fly": bits32(fly), "acc": bits32(acc),
        "distance": bits32(distance[0]), "flags0": cflags, "rand": rr, "used": 1 - len(rands), "hit": m.gpr(3),
        "flags": m.get(tw + 4, "H")})

    # --- zeal drain and give-up, fn_72_EDE8 (states other than 7) --------------------
    cursor[0] = HEAP + 0x40000
    t, tw, mw, wk, ax = fighter()
    st = rng.choice([2, 2, 2, 3, 4, 6, 0])
    sub = rng.choice([0, 0, 1])
    flags0 = rng.choice([0, 0x800])
    m.put(tw, "B", st); m.put(tw + 3, "B", sub); m.put(tw + 4, "h", flags0)
    intel = f32(rng.randrange(0, 4001)); z = fval(0, 1000) if rng.random() < 0.8 else rng.choice([0.0, 0.1, 499.0, 1000.0])
    m.put(wk + 2, "h", rng.choice([0, 1]))
    fput(wk + 0x34, intel); fput(wk + 0x38, z); fput(wk + 0x40, f32(rng.randrange(0, 4001)))
    leader_v = leader[0] = rng.choice([0, 1, 2])
    m.put(stage_phase, "B", rng.choice([0, 1]))
    rr = [rnd()]
    rands[:] = list(rr)
    m.call(T(ZEAL), r3=t)
    out["zeal"].append({"state": st, "frame": sub, "flags0": flags0,
        "intel": bits32(intel), "zeal0": bits32(z), "side": m.get(wk + 2, "h"), "leader": leader_v,
        "rand": rr, "used": 1 - len(rands), "zeal": fget(wk + 0x38), "flags": m.get(tw + 4, "H"),
        "stateAfter": m.get(tw, "B")})

    # --- ready charge, fn_72_FAB8 sub 1 ---------------------------------------------
    cursor[0] = HEAP + 0x50000
    t, tw, mw, wk, ax = fighter()
    a_task, a_twp, *_ = fighter()
    attacker[0] = a_task
    m.put(tw, "B", 2); m.put(tw + 1, "B", 1)
    runf = f32((1000 + 1.5 * rng.randrange(0, 4001)) / 1000)
    pw = fval(1200, 5200) if rng.random() < 0.8 else rng.choice([1599.0, 1600.0])
    ch = fval(0, 420) if rng.random() < 0.9 else rng.choice([347.0, 348.0, 399.9, 400.0])
    fput(wk + 0x28, runf); fput(wk + 0x2C, pw); fput(wk + 0x48, ch)
    rr = [rnd()]
    rands[:] = list(rr)
    m.call(T(CHARGE), r3=t)
    out["charge"].append({"run": bits32(runf), "power": bits32(pw), "charge0": bits32(ch), "rand": rr,
        "charge": fget(wk + 0x48), "state": m.get(tw, "B"), "sub": m.get(tw + 1, "B")})

    # --- attack outcome, fn_72_FE9C sub 3 ---------------------------------------------
    cursor[0] = HEAP + 0x60000
    t, tw, mw, wk, ax = fighter()
    a_task, *_ = fighter()
    attacker[0] = a_task
    m.put(tw, "B", 4); m.put(tw + 1, "B", 3)
    fl = rng.choice([0x8000, 0x8400, 0x400, 0])
    m.put(tw + 4, "h", struct.unpack(">h", struct.pack(">H", fl))[0])
    z = fval(0, 1000) if rng.random() < 0.8 else rng.choice([0.0, 990.0, 1000.0])
    luck = f32(rng.randrange(0, 4001))
    fput(wk + 0x38, z); fput(wk + 0x40, luck); fput(wk + 0x48, fval(0, 400))
    m.call(T(ATTACK), r3=t)
    out["attack"].append({"flags": fl, "zeal0": bits32(z), "luck": bits32(luck), "zeal": fget(wk + 0x38),
        "charge": fget(wk + 0x48), "state": m.get(tw, "B"), "sub": m.get(tw + 1, "B"), "timer": m.get(tw + 6, "H")})

    # --- HP-percentage leader, fn_72_133CC (with fn_72_11BB8 and njRoundOff) ----------
    cursor[0] = HEAP + 0x70000
    f0, f1t = fighter(), fighter()
    vals = []
    for (tk, _tw, _mw, wk_, _ax) in (f0, f1t):
        mx = fval(1000, 5000)
        hpv = f32(mx * rng.choice([1.0, 0.0, rng.random(), 0.505, 0.495]))
        fput(wk_ + 0x3C, hpv); fput(wk_ + 0x44, mx); vals += [bits32(hpv), bits32(mx)]
    out["leader"].append({"values": vals})

# The leader routine is stubbed above for the zeal cases; run it natively here.
m2 = dol.machine(boot=False)
cm2 = load_rel(m2, main_rel, MAIN_BASE, skip=(61,))
ka2 = load_rel(m2, kar_rel, KARATE_BASE, {60: cm2})
mgr2 = 0x81400000
m2.u.mem_write(mgr2, bytes(0x100000))
m2.put(ka2[6] + BSS8, "I", mgr2)
for c in out["leader"]:
    tasks = []
    for k in range(2):
        tk = mgr2 + 0x100 + k * 0x100; wk = tk + 0x80
        m2.put(tk + 0x40, "I", wk)
        m2.put(wk + 0x3C, "I", c["values"][2 * k]); m2.put(wk + 0x44, "I", c["values"][2 * k + 1])
        tasks.append(tk)
    m2.put(mgr2 + 0x10, "I", tasks[0]); m2.put(mgr2 + 0x14, "I", tasks[1])
    m2.put(mgr2 + 0x20, "I", 1); m2.put(mgr2 + 0x24, "I", 1)
    m2.call(ka2[1] + LEADER)
    c["leader"] = struct.unpack(">i", struct.pack(">I", m2.gpr(3)))[0]

# main.dol rand(): next starts at 1 (.sdata next = 1).
m2.put(0x803AD6A0, "I", 1)
for _ in range(64):
    m2.call(RAND)
    out["rand"].append(m2.gpr(3))
print(json.dumps(out))
