"""Run main.dol's player-contact routines (the ones `stage_objects::player_contact`
ports) under the main.dol harness on randomized player, task, collision and
game-state memory, and print JSON. tests/so_contact.rs replays each case.

Each case records every byte the routine reads before writing it (its
initial memory) and the final value of every byte it writes, except the
stack, the Ninja matrix stack (0x81700000) and its pointer words
(0x803AE0C0/C4), and constants (.text, .rodata, .sdata2 and the routines'
jump tables). Engine calls the port leaves to its host are stubbed and
recorded with their return values: SE_Call, fn_80011714 (voice),
Jingle_Play, fn_800BE308/374/408, CreateElementalTask, MAlloc, DestroyTask
and fn_80063E34. Everything else (the Ninja matrix routines, MSL sinf,
GetStageNum, the routines' own callees) runs as compiled.
Usage: python so_contact_ppc.py [cases per routine]
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, STACK_TOP  # noqa: E402
from unicorn import UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE  # noqa: E402

fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))

dol = Dol(os.environ["SA2_MAIN_DOL"])
m = dol.machine()
u = m.u
rng = random.Random(4242)
PER = int(sys.argv[1]) if len(sys.argv) > 1 else 300

PWP_T, MWP_T, TWP_T = 0x801E7728, 0x801E7748, 0x801E7768
GAME, SEARCH, HUDP, STAGE = 0x801CC168, 0x801DF680, 0x803AD860, 0x803AD820
CTRL3D, LOGI, LOGP, TOGGLE = 0x803AE0E8, 0x803AD9B0, 0x803AD9B8, 0x803AD9FC
VPORT, VTASKS = 0x801E5358, 0x801964F0

S = 0x81200000
TASKS = [S + 0x100 * k for k in range(8)]           # task blocks (0x50 used)
TWPS = [S + 0x1000 + 0x80 * k for k in range(8)]    # their twp
CWPS = [S + 0x2000 + 0x100 * k for k in range(8)]   # their cwp
PWPS = [S + 0x3000 + 0x200 * k for k in range(4)]   # player pwp
MWPS = [S + 0x4000 + 0x80 * k for k in range(4)]    # player mwp
PTWP = [S + 0x4400 + 0x80 * k for k in range(4)]    # player twp
WORK = [S + 0x5000 + 0x10 * k for k in range(8)]    # task +0x40 work
HUD = S + 0x6000
LOG = S + 0x7000                                    # 4 * 256 * 12 bytes
NEWT = [S + 0xB000 + 0x100 * k for k in range(4)]   # tasks CreateElementalTask returns
NEWW = [S + 0xB800 + 0x40 * k for k in range(4)]    # their +0x40 / +0x3C blocks
ARGS = S + 0xC000                                   # vectors passed by pointer

EXCL = [(0x80000000, 0x80173180 + 0x7960),  # .init/.text/ctors/rodata
        (0x803AE520, 0x803AE520 + 0x2F40),  # .sdata2
        (0x80196F84, 0x80197038), (0x801B59BC, 0x801B5A24),  # jump tables
        (STACK_TOP - 0x4000, STACK_TOP + 0x200),
        (0x81700000, 0x81701000), (0x803AE0C0, 0x803AE0C8)]


def excluded(a):
    return any(lo <= a < hi for lo, hi in EXCL)


track = {'on': False, 'reads': {}, 'written': set()}


def on_read(uc, access, addr, size, value, _):
    if not track['on']:
        return
    for a in range(addr, addr + size):
        if a in track['written'] or a in track['reads'] or excluded(a):
            continue
        track['reads'][a] = uc.mem_read(a, 1)[0]


def on_write(uc, access, addr, size, value, _):
    if not track['on']:
        return
    for a in range(addr, addr + size):
        if not excluded(a):
            track['written'].add(a)


u.hook_add(UC_HOOK_MEM_READ, on_read)
u.hook_add(UC_HOOK_MEM_WRITE, on_write)

events = []
ret_queue = []


def r(i):
    return m.gpr(i) & 0xffffffff


def stub_void(name, nargs):
    def f(mm):
        events.append([name] + [r(3 + k) for k in range(nargs)])
    return f


def stub_ret(name, nargs, pick):
    def f(mm):
        args = [r(3 + k) for k in range(nargs)]
        v = pick(args) & 0xffffffff
        events.append([name] + args + [v])
        mm.gpr(3, v)
    return f


def pick_task(args):
    return rng.choice([0] + NEWT)


m.stub(0x8006B978, stub_void('se', 1))
m.stub(0x80011714, stub_void('voice', 1))
m.stub(0x800110A4, stub_void('jingle', 1))
for a in (0x800BE308, 0x800BE374, 0x800BE408):
    m.stub(a, stub_ret('be%x' % (a & 0xfff), 1, lambda _: rng.choice([0, 0x81300000, 1])))
m.stub(0x80071E0C, stub_ret('cet', 4, pick_task))
m.stub(0x801197D0, stub_ret('malloc', 1, lambda _: rng.choice([0] + NEWW)))
m.stub(0x80071AF0, stub_void('destroy', 1))
m.stub(0x80063E34, stub_ret('rank', 1, lambda _: rng.choice([0, 1, 2, 3, 4, 5, 6, 7, 8, -1])))


# --- randomized world -----------------------------------------------------------
def put(a, fmt, v):
    m.put(a, fmt, v)


def rbytes(a, n):
    u.mem_write(a, bytes(rng.randrange(256) for _ in range(n)))


def rf(lo=-500.0, hi=500.0):
    x = rng.random()
    if x < 0.08:
        return f32(rng.choice([0.0, -0.0, 1.0, -1.0, 0.0005, -0.0005, 0.001, -0.001, 1e30, -1e30]))
    if x < 0.1:
        return float('nan')
    return f32(rng.uniform(lo, hi))


def pf(a, x):
    put(a, 'I', fb(x) if x == x else 0x7fc00000)


def opt(pool, p0=0.15):
    return 0 if rng.random() < p0 else rng.choice(pool)


def world():
    rbytes(S, 0xD000)
    for k, t in enumerate(TASKS):
        put(t + 8, 'I', 0 if rng.random() < 0.7 else rng.choice(TASKS))
        put(t + 0x34, 'I', opt([TWPS[k]], 0.1))
        put(t + 0x40, 'I', WORK[k])
        put(WORK[k], 'b', rng.choice([-1, 0, 1, 2, 3]))
        w = TWPS[k]
        put(w + 0, 'b', rng.choice([0x3c, 0x3d, 0x40, 0x44, 0x45, 0x46, 0x47, 0x4c, 0x4d, rng.randrange(-128, 128)]))
        put(w + 1, 'b', rng.choice([-1, 0, 1, 2, 3, rng.randrange(-128, 128)]))
        put(w + 2, 'B', rng.choice([0, 1, 2, 3, 4, 5, 6, 7, rng.randrange(256)]))
        put(w + 0x2c, 'I', opt([CWPS[k]], 0.15))
        c = CWPS[k]
        put(c + 0, 'H', rng.choice([0, 0, 1, 1, 2, rng.randrange(0x10000)]))
        put(c + 2, 'h', rng.choice([0, 1, 2, 5, 15, 16, -1, rng.randrange(-0x8000, 0x8000)]))
        put(c + 4, 'H', rng.choice([0, 2, 0xffff, rng.randrange(0x10000)]))
        end = rng.choice([16, 16, rng.randrange(16)])
        for i in range(16):
            rec = c + 0x10 + 8 * i
            put(rec + 1, 'b', -1 if i == end else rng.choice([0, 1, 2, rng.randrange(-128, 128)]))
            put(rec + 2, 'H', rng.choice([0, 2, 0xffff, rng.randrange(0x10000)]))
            put(rec + 4, 'I', opt(TASKS, 0.2))
    for n in range(4):
        p = PWPS[n]
        put(PWP_T + 4 * n, 'I', opt([p], 0.08))
        put(MWP_T + 4 * n, 'I', MWPS[n])
        put(TWP_T + 4 * n, 'I', opt([PTWP[n]], 0.08))
        put(p + 1, 'b', rng.choice([0, 1, 2, 3, 4, 5, 6, 7, 8, rng.randrange(-128, 128)]))
        put(p + 2, 'b', rng.choice([0, 1, 1, 2, rng.randrange(-128, 128)]))
        put(p + 3, 'b', rng.choice(list(range(17)) + [-1, 0x7f]))
        put(p + 0x10, 'H', rng.choice([0, 1, 2, 3, 0x8000, 0x8003, rng.randrange(0x10000)]))
        put(p + 0x24, 'I', rng.choice([0, 8, 0x80, 0x1000, 0x40000, 0x200000, 0x8000000, 0x800000,
                                       rng.randrange(1 << 32)]))
        pf(p + 0x48, rng.choice([0.0, 10.0, 12.0, 9.5, 11.5, 0.5, rf(-3, 15)]))
        put(p + 0xa0, 'I', rng.choice([0] + TASKS))
        put(p + 0xa4, 'I', rng.choice([0] + TASKS))
        for o in (0x176, 0x178, 0x17a):
            put(p + o, 'h', rng.choice([0x5e, 0x63, 0x73, 0xa3, 0, rng.randrange(-0x8000, 0x8000)]))
        pf(p + 0x184, rng.choice([3.0, 2.99, 3.01, 0.0, rf(-6, 6)]))
        t = PTWP[n]
        put(t + 0, 'b', rng.choice([0x3c, 0x3d, 0x41, 0x44, 0x45, 0x46, 0x47, 0x4a, 0x4c, 0x4d, rng.randrange(-128, 128)]))
        put(t + 1, 'B', rng.choice([0x24, 3, 0, rng.randrange(256)]))
        put(t + 4, 'H', rng.choice([0, 0x1000, 0x9000, rng.randrange(0x10000)]))
        for o in (8, 0xc, 0x10):
            put(t + o, 'i', rng.choice([0, 0x4000, -0x4000, 0x8000, rng.randrange(-0x10000, 0x10000),
                                        rng.randrange(-1 << 31, 1 << 31)]))
    rbytes(GAME, 0x100)
    put(GAME + 0, 'b', 0 if rng.random() < 0.8 else rng.randrange(-128, 128))
    put(GAME + 0x4a, 'b', 1 if rng.random() < 0.8 else 0)
    put(GAME + 0x14, 'B', rng.choice([0, 1, 2, 0, 0]))
    put(GAME + 0x1e, 'b', rng.choice([0, 1]))
    put(GAME + 0x12, 'b', rng.choice([0, 1]))
    for p in range(2):
        put(GAME + 0x64 + 2 * p, 'h', rng.choice([0, 1, 98, 99, 100, 0x7fff, -1, rng.randrange(-0x8000, 0x8000)]))
        put(GAME + 0x68 + 2 * p, 'h', rng.choice([rng.randrange(0, 130), 0, 99, 199, 990, 999, -5,
                                                rng.randrange(-0x8000, 0x8000)]))
    put(GAME + 0x90, 'I', rng.choice([0, 1000, 99999990, 99999999, 0xfffffff0, rng.randrange(1 << 32)]))
    put(HUDP, 'I', HUD)
    put(STAGE, 'h', rng.choice([0x46, 0x47, 0x5a, 0x0d, rng.randrange(-0x8000, 0x8000)]))
    put(LOGP, 'I', LOG)
    put(TOGGLE, 'I', rng.randrange(1 << 32))
    put(CTRL3D, 'I', rng.randrange(1 << 32))
    for n in range(4):
        put(LOGI + n, 'B', rng.randrange(256))
        put(VPORT + 4 * n, 'i', rng.choice([-1, 0, 1, 2, 3]))
        put(VTASKS + 4 * n, 'I', 0 if rng.random() < 0.5 else NEWT[n])
        put(NEWT[n] + 0x3c, 'I', NEWW[n])
        put(NEWT[n] + 0x40, 'I', NEWW[(n + 1) % 4])
    # the collision search state: often still on one of the tasks
    put(SEARCH, 'I', rng.choice([0] + TASKS))
    put(SEARCH + 4, 'I', rng.choice([0, 0x800065A8, 0x80006428, 0x800059C8, rng.randrange(1 << 32)]))
    put(SEARCH + 8, 'i', rng.choice([-1, 0, 1, 3, 14, 15, 16, 17, -3, rng.randrange(-20, 20)]))
    put(SEARCH + 0xc, 'i', rng.randrange(-5, 5))


# --- cases --------------------------------------------------------------------------
def player_arg():
    return rng.choice([0, 1, 2, 3, 0, 1, -1, 8, 9])


def ang():
    return [rng.choice([0, 0x4000, rng.randrange(-0x10000, 0x10000), rng.randrange(-1 << 31, 1 << 31)])
            for _ in range(3)]


def vec(lo=-50.0, hi=50.0):
    return [rf(lo, hi) for _ in range(3)]


def full_players():
    for n in range(4):
        put(PWP_T + 4 * n, 'I', PWPS[n])
        put(TWP_T + 4 * n, 'I', PTWP[n])


ROUTINES = {}


def routine(addr):
    def deco(fn):
        ROUTINES[fn.__name__] = (addr, fn)
        return fn
    return deco


@routine(0x80037EDC)
def is_this_task_player():
    t = rng.choice([0] + TASKS + TASKS)
    return [t], dict(r3=t)


def task_args():
    t = rng.choice([0] + TASKS * 3)
    return [t], dict(r3=t)


for name, addr in [('ccl_is_hit_player_ex', 0x800065A8), ('fn_80006428', 0x80006428),
                   ('fn_800059c8', 0x800059C8), ('ccl_is_hit_player', 0x800066F4),
                   ('fn_80006574', 0x80006574), ('fn_80005b04', 0x80005B04),
                   ('fn_800067c0', 0x800067C0)]:
    ROUTINES[name] = (addr, task_args)


@routine(0x80005904)
def fn_80005904():
    t = rng.choice([0] + TASKS * 3)
    i = rng.choice([-1, 0, 1, 7, 15, 16, rng.randrange(-100, 100)])
    return [t, i], dict(r3=t, r4=i & 0xffffffff)


@routine(0x80006D7C)
def ccl_clear_search():
    return [], {}


def attack():
    full_players()
    t, h = rng.choice(TASKS), rng.choice([0] + TASKS * 4)
    # the container's own task needs a twp and cwp
    put(t + 0x34, 'I', TWPS[TASKS.index(t)])
    put(TWPS[TASKS.index(t)] + 0x2c, 'I', CWPS[TASKS.index(t)])
    if rng.random() < 0.5 and h:
        # make the hitter a player with a cwp
        k = TASKS.index(h)
        put(h + 8, 'I', 0)
        put(h + 0x34, 'I', TWPS[k])
        put(TWPS[k] + 2, 'B', rng.choice([1, 2, 3, 4, 6]))
        put(TWPS[k] + 0x2c, 'I', CWPS[k])
        put(WORK[k], 'b', rng.choice([0, 1, 2, 3]))
        n = m.get(WORK[k], 'b')
        if n >= 0:
            p = PWPS[n]
            if rng.random() < 0.5:
                put(p + rng.choice([0xa0, 0xa4]), 'I', t)
            if rng.random() < 0.3:
                put(p + 1, 'b', rng.choice([4, 5]))
    return [t, h], dict(r3=t, r4=h)


ROUTINES['fn_8003699c'] = (0x8003699C, attack)
ROUTINES['fn_800368a0'] = (0x800368A0, attack)


@routine(0x8003A3DC)
def set_velocity_p():
    full_players()
    n, v = rng.choice([0, 1, 2, 3]), vec(-0.01, 0.01) if rng.random() < 0.3 else vec()
    return [n, [fb(x) if x == x else 0x7fc00000 for x in v]], dict(r3=n, f1=v[0], f2=v[1], f3=v[2])


@routine(0x8003A508)
def set_input_p():
    n = rng.choice([0, 1, 2, 3])
    a, b = rng.randrange(256), rng.choice([0x4b, rng.randrange(1 << 32)])
    return [n, a, b], dict(r3=n, r4=a, r5=b)


@routine(0x800399BC)
def fn_800399bc():
    n, v = rng.choice([0, 1, 2, 3]), vec()
    return [n, [fb(x) if x == x else 0x7fc00000 for x in v]], dict(r3=n, f1=v[0], f2=v[1], f3=v[2])


@routine(0x8003A54C)
def fn_8003a54c():
    n = rng.choice([0, 1, 2, 3])
    t = rng.choice([0] + TASKS)
    at = [rng.randrange(1 << 32) for _ in range(3)]
    for k in range(3):
        put(ARGS + 4 * k, 'I', at[k])
    p = rng.choice([0, ARGS])
    b = rng.choice([0, 1, rng.randrange(1 << 32)])
    return [n, t, at if p else None, b], dict(r3=n, r4=t, r5=p, r6=b)


@routine(0x80039EB0)
def fn_80039eb0():
    full_players()
    n, v, a = rng.choice([0, 1, 2, 3]), vec(), ang()
    timer = rng.choice([0, 1, 60, rng.randrange(-0x8000, 0x8000)])
    for k in range(3):
        pf(ARGS + 4 * k, v[k])
        put(ARGS + 0x10 + 4 * k, 'i', a[k])
    return ([n, [fb(x) if x == x else 0x7fc00000 for x in v], a, timer],
            dict(r3=n, r4=ARGS, r5=ARGS + 0x10, r6=timer & 0xffffffff))


@routine(0x8002B2D8)
def off_control_3d():
    f = rng.randrange(1 << 32)
    return [f], dict(r3=f)


@routine(0x8002B2E8)
def on_control_3d():
    f = rng.randrange(1 << 32)
    return [f], dict(r3=f)


@routine(0x8001DD78)
def get_stage_num():
    return [], {}


@routine(0x8006378C)
def add_score():
    n = rng.choice([10, 20, 100, 200, 1000, -5, rng.randrange(-1 << 31, 1 << 31)])
    return [n], dict(r3=n & 0xffffffff)


@routine(0x800641B8)
def fn_800641b8():
    p, c = rng.choice([0, 1, 2, 3, -1]), rng.choice([1, 0, -1, 2, 0x10001, rng.randrange(-200, 200)])
    return [p, c], dict(r3=p & 0xffffffff, r4=c & 0xffffffff)


@routine(0x80035CF0)
def add_mech_hp():
    n = rng.choice([0, 1, 2, 3])
    if rng.random() < 0.6:
        put(PWPS[n] + 1, 'b', rng.choice([6, 7]))
    hp = rng.choice([1.0, 2.0, 12.0, -1.0, -20.0, 0.5, rf(-15, 15)])
    return [n, fb(hp) if hp == hp else 0x7fc00000], dict(r3=n, f1=hp)


@routine(0x800637EC)
def add_num_ring():
    p = rng.choice([0, 1, 2, 3, -1])
    if rng.random() < 0.7:
        put(GAME + 0x68 + 2 * (p & 1), 'h', rng.randrange(0, 130))
    c = rng.choice([1, 5, 10, 20, 40, 100, -5, -200, 0x10005, rng.randrange(-50, 60)])
    return [p, c], dict(r3=p & 0xffffffff, r4=c & 0xffffffff)


def item():
    n = player_arg()
    return [n], dict(r3=n & 0xffffffff)


for name, addr in [('fn_800373b8', 0x800373B8), ('fn_800374e0', 0x800374E0),
                   ('fn_800375fc', 0x800375FC), ('fn_80037718', 0x80037718)]:
    ROUTINES[name] = (addr, item)


@routine(0x8002FB2C)
def fn_8002fb2c():
    p = rng.choice([0, 1, 2, 3])
    a = rng.choice([0, 1, -1, 2, 7, 8, -8, -7, 100, rng.randrange(-20, 20)])
    b = rng.choice([0, 15, 14, 59, 60, 0x30, rng.randrange(-100, 100)])
    c = rng.choice([0, 0, 1, 255, 256, -1, rng.randrange(-300, 300)])
    return [p, a, b, c], dict(r3=p, r4=a & 0xffffffff, r5=b & 0xffffffff, r6=c & 0xffffffff)


def coalesce(d):
    out, run = [], None
    for a in sorted(d):
        if run and a == run[0] + len(run[1]):
            run[1].append(d[a])
        else:
            run = [a, [d[a]]]
            out.append(run)
    return [[a, bytes(b).hex()] for a, b in out]


cases, skipped = [], {}
for name, (addr, gen) in ROUTINES.items():
    done = 0
    tries = 0
    while done < PER and tries < PER * 4:
        tries += 1
        world()
        args, regs = gen()
        events.clear()
        track['reads'], track['written'] = {}, set()
        track['on'] = True
        try:
            m.call(addr, **regs)
        except Exception:
            track['on'] = False
            skipped[name] = skipped.get(name, 0) + 1
            continue
        track['on'] = False
        final = {a: u.mem_read(a, 1)[0] for a in track['written']}
        cases.append({'r': name, 'args': args, 'ret': r(3), 'ev': list(events),
                      'mem': coalesce(track['reads']), 'out': coalesce(final)})
        done += 1

print(json.dumps({'cases': cases, 'skipped': skipped}))
