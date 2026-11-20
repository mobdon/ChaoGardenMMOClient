"""Run stg42D.rel's ring code (RING, Tobitiri, the ring model list and the
ring-group link helpers) under the main.dol harness and print JSON (every
float as its bit pattern). tests/so_ring.rs replays each frame with
`stage_objects::ring` and compares. Usage: python so_ring_ppc.py

Each frame starts from a state written into memory (task, task work, fwk,
ring objects, the module's BSS) and records it as the frame's input, so the
Rust side loads the same state, runs the same routine and compares the
events and the resulting state. Engine routines are stubbed and recorded;
the REL's own code and main.dol's Ninja math (njSin/njCos, the matrix
stack, njDistanceP2P, njScalor, SqMag, CalcAdvanceAsPossible) run as
compiled. The REL-local MakeParticle (0xC56C) is a hook.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
gekko_tables.require()  # njScalor and njUnitVector need Gekko's estimates
from so_harness import *  # noqa: E402,F401,F403

TASK, TWP, CWP, AWP = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x280
MWK = SCRATCH + 0x300
PWP = [SCRATCH + 0x1000, SCRATCH + 0x1100]
PTWP = [SCRATCH + 0x1200, SCRATCH + 0x1300]
PCWP = [SCRATCH + 0x1400, SCRATCH + 0x1500]
PINFO = [SCRATCH + 0x1600, SCRATCH + 0x1700]
ORIGIN = SCRATCH + 0x1800
NEWTASKS = SCRATCH + 0x4000
HEAP = SCRATCH + 0x20000
MODEL_TASK = SCRATCH + 0x3000
CURMTX = 0x803AE0C4
PLAYERPWP, PLAYERTWP = 0x801E7728, 0x801E7768
GRAVITY, GANG_Z, GANG_X, ORIGIN_PTR, GAME = 0x801E5624, 0x803AD914, 0x803AD918, 0x803ADB04, 0x801CC168
BSS = {'fog': S6(0x630), 'colli_timer': S6(0x6a8), 'head': S6(0x6ac), 'cleanup': S6(0x6b0), 'ring_tp': S6(0x6b4)}
NLINKS = 4
ONE = 0x3f800000

st = {'heap': HEAP, 'objects': set(), 'tasks': NEWTASKS, 'created': []}
fr = {}
out = {'cases': []}


def bits(a, n=3):
    return [m.get(a + 4 * k, 'I') for k in range(n)]


def put_bits(a, vs):
    for k, v in enumerate(vs):
        m.put(a + 4 * k, 'I', v)


def q(name, v):
    fr.setdefault(name, []).append(v)
    return v


# --- stubs -------------------------------------------------------------------
def calloc(mm):
    n, size = mm.gpr(3), mm.gpr(4)
    assert n == 1
    fail = rng.random() < fr['p_fail'].get(size, 0.0)
    addr = 0
    if not fail:
        addr = st['heap']
        st['heap'] += 0x50
        clear(addr, 0x50)
        if size == 0x48:
            st['objects'].add(addr)
    ev('calloc', size)
    q('calloc', addr)
    mm.gpr(3, addr)


def free(mm):
    a = mm.gpr(3)
    st['objects'].discard(a)
    ev('free', a)


def init_share(mm):
    assert mm.gpr(3) == TASK and mm.gpr(4) == S5(0x1acfc) and mm.gpr(5) == 1
    ok = rng.random() >= fr['p_noinit']
    q('init', ok)
    ev('init_share', mm.gpr(6))
    if ok:
        clear(CWP, 0x10)
        m.put(TWP + 0x2c, 'I', CWP)


def is_hit(mm):
    assert mm.gpr(3) == TASK
    h = rng.random() < fr['p_hit']
    q('hit', h)
    ev('is_hit')
    mm.gpr(3, 0x81234560 if h else 0)


def which(mm):
    assert mm.gpr(3) == 0x81234560
    # Only a present player can be credited (the original reads its work
    # unchecked).
    have = [i for i in (0, 1) if fr['g']['players'][i]]
    v = rng.choice(have * 3 + [-1, 2])
    q('which', v)
    ev('which')
    mm.gpr(3, v)


def shadow(mm):
    assert mm.gpr(3) == TWP + 8
    p = [fb(f32(mm.fpr(k))) for k in (1, 2, 3)]
    y = q_y = rng.choice([fb(f32(bf(p[1]) + rng.uniform(-12, 12))), fb(rnd_f()), fb(-1e6)])
    ang = bits(TWP + 8)
    if rng.random() < 0.5:
        ang[0] = rng.randrange(-0x8000, 0x8000) & 0xffffffff
        ang[2] = rng.randrange(-0x8000, 0x8000) & 0xffffffff
        put_bits(TWP + 8, ang)
    q('shadow', [q_y, [sext(a) for a in ang]])
    ev('shadow', p)
    mm.fpr(1, bf(y))


def sext(v):
    return v - (1 << 32) if v & 0x80000000 else v


def shadow_xyz(mm):
    a = mm.gpr(3)
    ev('shadow_xyz', bits(a))
    hits = []
    for k in range(6):
        h = a + 0xc + 0x20 * k
        if rng.random() < 0.6:
            e = [fb(rnd_f(-300, 300)), rng.randrange(-0x4000, 0x4000), rng.randrange(-0x4000, 0x4000)]
            m.put(h, 'I', 1)
            m.put(h + 8, 'i', e[1])
            m.put(h + 0xc, 'i', e[2])
            m.put(h + 0x10, 'I', e[0])
            hits.append(e)
        else:
            m.put(h, 'I', 0)
            hits.append(None)
    q('xyz', hits)


def create(mm):
    lvl, slot, exe, name = mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6)
    t = 0
    if rng.random() >= fr['p_nocreate']:
        t = st['tasks']
        st['tasks'] += 0x100
        clear(t, 0x100)
        m.put(t + 0x34, 'I', t + 0x80)
        st['created'].append(t)
    ev('create', lvl, slot, exe - S1(0), name - S5(0))
    q('create', t)
    mm.gpr(3, t)


def rand(mm):
    v = rng.choice([0, 32767, 16384, rng.randrange(32768), rng.randrange(32768)])
    q('rand', v)
    ev('rand')
    mm.gpr(3, v)


def draw(mm):
    cur = m.get(CURMTX, 'I')
    ev('draw', mm.gpr(3) - S5(0), bits(cur, 12))


m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), fr['out'])[1]))
m.stub(0x801197FC, calloc)
m.stub(0x80119828, free)
m.stub(0x8000F7D8, init_share)
m.stub(0x800066F4, is_hit)
m.stub(0x80037EDC, which)
m.stub(0x80067AF0, shadow)
m.stub(0x80067210, shadow_xyz)
m.stub(0x80071E0C, create)
m.stub(0x8010DC88, rand)
m.stub(0x8006378C, lambda mm: ev('score', sext(mm.gpr(3))))
m.stub(0x800637EC, lambda mm: ev('num_ring', sext(mm.gpr(3)), sext(mm.gpr(4))))
m.stub(0x80035CF0, lambda mm: ev('mech', sext(mm.gpr(3)), fb(f32(mm.fpr(1)))))
m.stub(0x8006A98C, lambda mm: ev('se_ring', sext(mm.gpr(3))))
m.stub(S1(0xC56C), lambda mm: ev('particle', bits(mm.gpr(3)), bits(mm.gpr(4)), fb(f32(mm.fpr(1)))))
m.stub(0x8000F4EC, lambda mm: (assert_task(mm), ev('entry', bits(TWP + 0x14))))
m.stub(0x800653B0, lambda mm: ev('dead_out'))
m.stub(0x80071AE0, lambda mm: ev('free_task'))
m.stub(0x80071AF0, lambda mm: ev('destroy', 0 if mm.gpr(3) == TASK else mm.gpr(3)))
m.stub(0x80116E18, lambda mm: ev('texture', mm.gpr(3) - S5(0)))
m.stub(0x8011C2BC, lambda mm: ev('fog_off'))
m.stub(0x8011C2B0, lambda mm: ev('fog_on'))
m.stub(0x8011C310, lambda mm: ev('set_fog'))
m.stub(0x80121BDC, draw)


def assert_task(mm):
    assert mm.gpr(3) == TASK


# --- state in memory -----------------------------------------------------------
def read_task():
    cwp = m.get(TWP + 0x2c, 'I')
    mwp = m.get(TASK + 0x38, 'I')
    fwp = m.get(TASK + 0x3c, 'I')
    assert cwp in (0, CWP) and (mwp == 0 or (mwp - MWK) % 8 == 0)
    return {
        'mode': m.get(TWP, 'b'), 'smode': m.get(TWP + 1, 'b'), 'id': m.get(TWP + 2, 'B'),
        'btimer': m.get(TWP + 3, 'B'), 'wtimer': m.get(TWP + 6, 'H'),
        'ang': [m.get(TWP + 8 + 4 * k, 'i') for k in range(3)],
        'pos': bits(TWP + 0x14), 'scl': bits(TWP + 0x20),
        'cwp': [m.get(CWP + 4, 'H'), m.get(CWP + 8, 'I')] if cwp else None,
        'awp': bits(AWP, 2),
        'mwp': (mwp - MWK) // 8 if mwp else None,
        'fwp': fwp,
        'fwk': [m.get(fwp + 4, 'I'), m.get(fwp + 8, 'I'), m.get(fwp + 0xc, 'i'), m.get(fwp + 0x10, 'i')] if fwp else [0, 0, 0, 0],
        'work': m.get(TASK + 0x4c, 'I'),
        'ocp': m.get(TASK + 0x30, 'I') != 0,
        'dest': m.get(TASK + 0x18, 'I') == S1(0x1aecc),
    }


def write_task(t):
    clear(TASK, 0x50); clear(TWP, 0x40); clear(AWP, 0x10)
    m.put(TASK + 0x34, 'I', TWP)
    m.put(TASK + 0x40, 'I', AWP)
    m.put(TWP, 'b', t['mode']); m.put(TWP + 1, 'b', t['smode']); m.put(TWP + 2, 'B', t['id'])
    m.put(TWP + 3, 'B', t['btimer']); m.put(TWP + 6, 'H', t['wtimer'])
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', t['ang'][k])
    put_bits(TWP + 0x14, t['pos']); put_bits(TWP + 0x20, t['scl'])
    if t['cwp'] is not None:
        clear(CWP, 0x10)
        m.put(TWP + 0x2c, 'I', CWP)
        m.put(CWP + 4, 'H', t['cwp'][0]); m.put(CWP + 8, 'I', t['cwp'][1])
    put_bits(AWP, t['awp'])
    if t['mwp'] is not None:
        m.put(TASK + 0x38, 'I', MWK + 8 * t['mwp'])
    m.put(TASK + 0x3c, 'I', t['fwp'])
    if t['fwp']:
        m.put(t['fwp'] + 4, 'I', t['fwk'][0]); m.put(t['fwp'] + 8, 'I', t['fwk'][1])
        m.put(t['fwp'] + 0xc, 'i', t['fwk'][2]); m.put(t['fwp'] + 0x10, 'i', t['fwk'][3])
    m.put(TASK + 0x4c, 'I', t['work'])
    m.put(TASK + 0x30, 'I', 1 if t['ocp'] else 0)
    m.put(TASK + 0x18, 'I', S1(0x1aecc) if t['dest'] else 0)


def read_world():
    objs = []
    for a in sorted(st['objects']):
        objs.append([a, m.get(a, 'I'), m.get(a + 4, 'I') != 0, bits(a + 8),
                     [m.get(a + 0x14 + 4 * k, 'i') for k in range(3)], m.get(a + 0x30, 'I'),
                     [m.get(a + 0x38 + 4 * k, 'i') for k in range(3)], m.get(a + 0x44, 'I')])
    links = [[m.get(MWK + 8 * k, 'I'), m.get(MWK + 8 * k + 4, 'I')] for k in range(NLINKS)]
    return {'colli_timer': m.get(BSS['colli_timer'], 'i'), 'head': m.get(BSS['head'], 'I'),
            'cleanup': m.get(BSS['cleanup'], 'i'), 'ring_tp': m.get(BSS['ring_tp'], 'I'),
            'objects': objs, 'links': links}


def write_globals(g):
    put_bits(GRAVITY, g['gravity'])
    m.put(GANG_Z, 'i', g['gravity_angles'][0]); m.put(GANG_X, 'i', g['gravity_angles'][1])
    m.put(GAME + 0x37, 'b', g['pause']); m.put(GAME + 0x3d, 'b', g['hide'])
    m.put(BSS['fog'], 'i', g['fog'])
    for i in range(2):
        p = g['players'][i]
        m.put(PLAYERPWP + 4 * i, 'I', PWP[i] if p else 0)
        if p:
            clear(PWP[i], 0x80)
            m.put(PWP[i] + 0x10, 'h', p[0]); put_bits(PWP[i] + 0x64, p[1])
        t = g['player_tasks'][i]
        m.put(PLAYERTWP + 4 * i, 'I', PTWP[i] if t else 0)
        if t:
            clear(PTWP[i], 0x40)
            put_bits(PTWP[i] + 0x14, t[0])
            m.put(PTWP[i] + 0x2c, 'I', PCWP[i]); m.put(PCWP[i] + 0xc, 'I', PINFO[i])
            put_bits(PINFO[i] + 8, t[1])
    m.put(ORIGIN_PTR, 'I', ORIGIN)
    put_bits(ORIGIN + 0x14, g['origin'])


def matrix_state():
    cur = m.get(CURMTX, 'I')
    return cur, bits(cur, 12)


def set_matrix(mtx):
    cur = m.get(CURMTX, 'I')
    put_bits(cur, mtx)


# --- random inputs ---------------------------------------------------------------
def rnd_f(lo=-500.0, hi=500.0):  # noqa: F811 (finite: see rnd_vec)
    if rng.random() < 0.1:
        return rng.choice([0.0, -0.0, 1.0, -10.0, 0.5])
    return f32(rng.uniform(lo, hi))


def rnd_vec(lo=-500, hi=500):
    # Finite positions only: x86 and Gekko give NaNs of different signs, so
    # inf - inf would compare the hosts, not the code.
    return [fb(rng.choice([0.0, -0.0, f32(rng.uniform(lo, hi))]) if rng.random() < 0.1
               else f32(rng.uniform(lo, hi))) for _ in range(3)]


def rnd_gravity():
    c = rng.random()
    if c < 0.5:
        return [fb(0.0), fb(-1.0), fb(0.0)]
    if c < 0.8:
        v = [0.0, 0.0, 0.0]
        v[rng.randrange(3)] = rng.choice([1.0, -1.0])
        return [fb(x) for x in v]
    return [fb(f32(rng.uniform(-1, 1))) for _ in range(3)]


def rnd_globals(near=None):
    near = near or [0.0, 0.0, 0.0]
    def player():
        if rng.random() < 0.15:
            return None
        item = rng.choice([0, 0, 2, 2, 0x4000, 0x4002, rng.randrange(-0x8000, 0x8000)])
        return [item, [fb(rnd_f(-6, 6)) for _ in range(3)]]
    def ptask():
        p = [fb(f32(near[k] + rng.uniform(-80, 80))) for k in range(3)]
        if rng.random() < 0.1:
            p = rnd_vec()
        c = [fb(f32(bf(p[k]) + rng.uniform(-10, 10))) for k in range(3)]
        return [p, c]
    pl = [player(), player()]
    return {
        'gravity': rnd_gravity(),
        'gravity_angles': [rng.choice([0, 0, rng.randrange(-0x8000, 0x8000)]), rng.choice([0, 0, rng.randrange(-0x8000, 0x8000)])],
        'pause': rng.choice([0, 0, 0, 1, -1]),
        'hide': rng.choice([0, 0, 0, 1]),
        'fog': rng.choice([0, 1]),
        'players': pl,
        'player_tasks': [ptask(), ptask()],
        'origin': [fb(f32(near[k] + rng.uniform(-600, 600) * rng.choice([1, 0.3]))) for k in range(3)],
    }


def new_frame(p_fail=None, p_hit=0.3):
    fr.clear()
    # The fwk (0x14) of a RING and the collision work are used unchecked
    # after a failure, so only the ring object (0x48) and Tobitiri's fwk
    # (retried next frame) may fail.
    fr['p_fail'] = p_fail or {0x14: 0.0, 0x48: 0.15}
    fr['p_noinit'] = 0.0
    fr['p_hit'] = p_hit
    fr['p_nocreate'] = 0.2
    fr['out'] = rng.random() < 0.08
    events.clear()
    st['created'] = []


def run(op, arg=0, globals_=None, view=None):
    """One recorded frame: the state in memory is the input."""
    g = globals_
    fr['g'] = g
    write_globals(g)
    if view is not None:
        set_matrix(view)
    cur0, mtx0 = matrix_state()
    rec = {'op': op, 'arg': arg, 'globals': g, 'out': fr['out'], 'task_in': read_task(),
           'world_in': read_world(), 'matrix_in': mtx0}
    entry = {'ring': S1(0x196e8), 'tobitiri': S1(0x19ff0), 'ring_end': S1(0x1aecc),
             'model_exec': S1(0x1b3a4), 'model_dest': S1(0x1b318), 'disp': S1(0x1b478),
             'disp2': S1(0x1b5ec), 'link': S1(0x19550), 'unlink': S1(0x1957c)}[op]
    regs = {'r3': TASK}
    if op in ('model_exec', 'model_dest'):
        regs = {'r3': arg}
    if op == 'link':
        regs = {'r3': TASK, 'r4': MWK + 8 * arg}
    m.call(entry, **regs)
    for t in st['created']:
        if m.get(t + 0x10, 'I') == S1(0x1b3a4):
            assert [m.get(t + o, 'I') for o in (0x14, 0x18, 0x1c)] == [S1(0x1b478), S1(0x1b318), S1(0x1b5ec)]
        else:
            tw = t + 0x80
            ev('spawn', t, bits(tw + 0x14), m.get(tw + 0xc, 'i'))
    cur1, mtx1 = matrix_state()
    for k in ('calloc', 'init', 'hit', 'which', 'shadow', 'xyz', 'create', 'rand'):
        rec[k] = fr.get(k, [])
    rec.update(events=list(events), task=read_task(), world=read_world(),
               depth=(cur1 - cur0) // 0x30, matrix=mtx1)
    return rec


def reset_world(colli_timer=None):
    for k in ('colli_timer', 'head', 'cleanup', 'ring_tp'):
        m.put(BSS[k], 'I', 0)
    m.put(BSS['colli_timer'], 'i', rng.randrange(0, 9) if colli_timer is None else colli_timer)
    st['objects'] = set()
    for k in range(NLINKS):
        m.put(MWK + 8 * k, 'I', rng.choice([0, 1, 2, 3, 0x10]))
        m.put(MWK + 8 * k + 4, 'I', 0)


def fresh_task(mode=0):
    pos = rnd_vec(-2000, 2000)
    t = {'mode': mode, 'smode': 0, 'id': 0, 'btimer': rng.choice([0, 0, 49, 50, 200]), 'wtimer': 0,
         'ang': [rng.choice([0, 1, 0x10001, rng.randrange(-0x8000, 0x8000)]), rng.randrange(-0x8000, 0x8000), rng.randrange(-0x4000, 0x4000)],
         'pos': pos, 'scl': [rng.choice([fb(-1.0), fb(-1.0), fb(0.0), fb(rnd_f())]), fb(rnd_f()), fb(rnd_f())],
         'cwp': None, 'awp': [0, 0], 'mwp': None, 'fwp': 0, 'fwk': [0, 0, 0, 0], 'work': 0,
         'ocp': rng.random() < 0.5, 'dest': False}
    return t


# --- cases ----------------------------------------------------------------------
def ring_cases(n):
    for _ in range(n):
        reset_world()
        t = fresh_task(rng.choice([0, 0, 2]))
        if t['mode'] == 2 or rng.random() < 0.3:
            t['mwp'] = rng.randrange(NLINKS)
        write_task(t)
        frames = []
        for f in range(rng.randrange(4, 14)):
            near = [bf(x) for x in bits(TWP + 0x14)]
            g = rnd_globals(near)
            new_frame(p_hit=rng.choice([0.0, 0.1, 0.4]))
            # Occasionally poke the running state toward the rarer branches.
            if m.get(TWP, 'b') == 1 and rng.random() < 0.3:
                m.put(TWP + 1, 'b', rng.choice([0, 0, 0, 1, -1, 8]))
                m.put(TWP + 6, 'H', rng.choice([0, 1, 5]))
                m.put(CWP + 4, 'H', rng.choice([0, 1, 0x41]))
                if rng.random() < 0.3:
                    m.put(TWP, 'b', rng.choice([3, 4, -1, 2]))
            elif m.get(TWP, 'b') == 1 and rng.random() < 0.3:
                # The ground probe: due now, no magnet, the origin near.
                m.put(TWP + 1, 'b', 0)
                if m.get(TWP + 0x24, 'I') == fb(-1e6):
                    m.put(TWP + 0x24, 'I', m.get(TWP + 0x18, 'I'))
                g['players'] = [p and [p[0] & ~2, p[1]] for p in g['players']]
                g['origin'] = [fb(f32(near[k] + rng.uniform(-300, 300))) for k in range(3)]
            frames.append(run('ring', globals_=g))
            if m.get(TWP, 'b') == 3 and rng.random() < 0.5:
                break
        if rng.random() < 0.7:
            new_frame()
            frames.append(run('ring_end', globals_=rnd_globals()))
        out['cases'].append({'kind': 'ring', 'frames': frames})


def tobitiri_cases(n):
    for _ in range(n):
        reset_world()
        t = fresh_task(0)
        write_task(t)
        g = rnd_globals([bf(x) for x in t['pos']])
        frames = []
        for f in range(rng.randrange(6, 20)):
            new_frame(p_hit=rng.choice([0.0, 0.0, 0.05, 0.3]))
            if rng.random() < 0.3:
                g = dict(g, pause=rng.choice([0, 1]))
            mode = m.get(TWP, 'b')
            if mode in (1, 2) and rng.random() < 0.35:
                # Jump the timers toward their thresholds.
                m.put(TWP + 6, 'H', rng.choice([238, 240, 241, 178, 179, 180, 1, 2, 3]))
                m.put(TWP + 1, 'b', rng.choice([44, 45, 46, 47, 127, -128]))
                m.put(AWP + 4, 'I', fb(rng.choice([0.5, 1.0, 0.0, -1.0, 7.0])))
                if rng.random() < 0.2:
                    m.put(TWP, 'b', rng.choice([1, 2, 3, 4, 5, -1]))
            if mode in (1, 2) and rng.random() < 0.3:
                m.put(CWP + 4, 'H', rng.choice([0, 1, 0x41]))
            if mode == 1 and rng.random() < 0.3:
                # A fast fall toward the floor, or a tilted floor.
                put_bits(TWP + 0x20, [fb(rnd_f(-6, 6)) for _ in range(3)])
                m.put(AWP, 'I', fb(f32(bf(m.get(TWP + 0x18, 'I')) - rng.uniform(-2, 8))))
                m.put(TWP + 8, 'i', rng.choice([0, rng.randrange(-0x2000, 0x2000)]))
                m.put(TWP + 0x10, 'i', rng.choice([0, rng.randrange(-0x2000, 0x2000)]))
            frames.append(run('tobitiri', globals_=g))
            if m.get(TWP, 'b') in (3, 4) and rng.random() < 0.3:
                new_frame()
                frames.append(run('tobitiri', globals_=g))
                break
        if m.get(TASK + 0x18, 'I') and rng.random() < 0.8:
            new_frame()
            frames.append(run('ring_end', globals_=g))
        out['cases'].append({'kind': 'tobitiri', 'frames': frames})


def rnd_view():
    # A camera-like matrix: rotation and translation (the draw tests only
    # need the transform order to be honoured).
    m.call(0x801153E4, r3=0)
    m.call(0x80115C60, r3=0, r4=rng.randrange(-0x8000, 0x8000))
    m.call(0x80115828, r3=0, r4=rng.randrange(-0x8000, 0x8000))
    m.call(0x80115BD8, r3=0, f1=rnd_f(-300, 300), f2=rnd_f(-300, 300), f3=rnd_f(-300, 300))
    return matrix_state()[1]


def model_cases(n):
    for _ in range(n):
        reset_world()
        frames = []
        # Build a list through the real CreateRingObject (via Ring's start).
        for k in range(rng.randrange(1, 7)):
            write_task(fresh_task(0))
            new_frame(p_fail={0x14: 0.0, 0x48: 0.1})
            fr['out'] = False
            run('ring', globals_=rnd_globals())
            a = m.get(TASK + 0x4c, 'I')
            if a and rng.random() < 0.9:
                # Spread the objects around and give them shadows.
                put_bits(a + 8, [fb(rnd_f(-200, 200)) for _ in range(3)])
                m.put(a + 0x18, 'i', rng.randrange(-0x8000, 0x8000))
                for j in range(3):
                    m.put(a + 0x38 + 4 * j, 'i', rng.randrange(-0x4000, 0x4000))
                m.put(a + 0x44, 'I', rng.choice([fb(-1e6), fb(1e6), fb(rnd_f()), fb(rnd_f())]))
                m.put(a + 0, 'I', rng.choice([0x14, 0x14, 0x1c, 0x8, 0]))
            if a and rng.random() < 0.35:
                new_frame()
                frames.append(run('ring_end', globals_=rnd_globals()))
        tp = m.get(BSS['ring_tp'], 'I') or MODEL_TASK
        for f in range(rng.randrange(2, 6)):
            new_frame()
            g = rnd_globals()
            op = rng.choice(['disp', 'disp', 'disp2', 'disp2', 'model_exec', 'model_exec', 'model_dest'])
            arg = rng.choice([tp, tp, MODEL_TASK + 0x10]) if op in ('model_exec', 'model_dest') else 0
            if op == 'model_exec' and rng.random() < 0.5:
                m.put(BSS['cleanup'], 'i', rng.choice([1, 0, -1]))
            frames.append(run(op, arg, g, rnd_view() if op.startswith('disp') else None))
        out['cases'].append({'kind': 'model', 'frames': frames})


def link_cases(n):
    for _ in range(n):
        reset_world()
        t = fresh_task(rng.choice([0, 0, 1, 2, -1]))
        if rng.random() < 0.5:
            t['mwp'] = rng.randrange(NLINKS)
        write_task(t)
        new_frame()
        op = rng.choice(['link', 'unlink'])
        frames = [run(op, rng.randrange(NLINKS), rnd_globals())]
        out['cases'].append({'kind': 'link', 'frames': frames})


def face_cases(n):
    vs = []
    for _ in range(n):
        v = [rng.choice([0.9, -0.9, f32(0.9000001), 1.0, -1.0, 0.0, -0.0, f32(rng.uniform(-1.2, 1.2)),
                         float('nan')]) for _ in range(3)]
        put_bits(SCRATCH + 0x2000, [fb(x) if x == x else 0x7fc00000 for x in v])
        m.call(S1(0x19f48), r3=SCRATCH + 0x2000)
        vs.append([bits(SCRATCH + 0x2000), m.gpr(3)])
    out['faces'] = vs


D = int(sys.argv[1]) if len(sys.argv) > 1 else 1  # a divisor for quick runs
ring_cases(220 // D)
tobitiri_cases(220 // D)
model_cases(120 // D)
link_cases(80 // D)
face_cases(400 // D)
print(json.dumps(out))
