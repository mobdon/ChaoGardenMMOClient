"""Run stg42D.rel's messenger code (MSGER / Omochao and the ribbon-trail
library) under the main.dol harness and print JSON (every float as its bit
pattern). tests/so_messenger.rs replays each case with
`stage_objects::messenger` and compares. Usage: python so_messenger_ppc.py
<trail|messenger|display>
Engine routines and the module helpers the port leaves out are stubbed and
recorded as events; the REL code and main.dol's math (njCalcPoint, njSin,
njScalor, the colour blend, the Ninja matrix routines) run as compiled.
Allocations, tasks and windows are numbered in the order they appear.
"""
import json, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403
import gekko_tables  # noqa: E402
from so_dynland_ppc import get_words  # noqa: E402

if gekko_tables.load() is None:
    sys.stderr.write('needs the local Gekko estimate tables (njScalor)\n')
    sys.exit(0)

which = sys.argv[1]
out = {'object': which, 's5': S5(0), 'cases': []}
CUR = 0x803B52C0 - 0x71FC          # Ninja's current-matrix pointer
GAME_STATE = 0x803B52C0 - 0x7AA6   # fn_8001CD40 reads this halfword
CLOCK = 0x801CC168                 # +0x7C frame, +0x37 reset, +0x11 language
SCREEN = 0x803ADAD0
PLAYERTWP = 0x801E7768
HEAP = 0x81210000
VERTS = 0x80FFFE60
SEED = 0x803B52C0 - 0x7C20         # rand's state (r13 -0x7C20)

ctx = {}


def reset_case():
    ctx.update(heap=HEAP, ids={}, next=1, out=False, made=[], calloc_ok=[], tasks_ok=[], windows=[], bones=[], chara=[])
    events.clear()


def bump(n):
    a = ctx['heap']
    ctx['heap'] = (a + max(n, 4) + 31) & ~31
    assert ctx['heap'] < 0x81700000
    clear(a, max(n, 4))
    return a


def newid(addr):
    ctx['ids'][addr] = ctx['next']
    ctx['next'] += 1
    return ctx['ids'][addr]


def idof(addr):
    return ctx['ids'].get(addr, addr)


def take(key, default):
    q = ctx[key]
    return q.pop(0) if q else default


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def put_vec(a, v):
    for k, x in enumerate(v):
        m.put(a + 4 * k, 'I', x)


def fin(lo=-500.0, hi=500.0):
    """rnd_f without its +-1e30 edge values. Products of those overflow,
    and a NaN made from infinities is 0x7FC00000 on the PowerPC but
    0xFFC00000 on x86, which only that sign tells apart; the game's
    positions stay far from overflow."""
    while True:
        x = rnd_f(lo, hi)
        if abs(x) < 1e29:
            return x


def rnd_bits(lo=-500.0, hi=500.0):
    return fb(fin(lo, hi))


def put_matrix(mat):
    m.u.mem_write(m.get(CUR, 'I'), struct.pack('>12I', *mat))


def rnd_matrix():
    k = rng.random()
    if k < 0.15:
        return [fb(x) for x in (1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0)]
    return [fb(fin(-2.0, 2.0)) if (i % 4) != 3 else fb(fin(-300.0, 300.0)) for i in range(12)]


def get_matrix():
    return [m.get(m.get(CUR, 'I') + 4 * k, 'I') for k in range(12)]


# --- engine stubs ------------------------------------------------------------
def calloc(mm):
    n, size = mm.gpr(3), mm.gpr(4)
    if not take('calloc_ok', True):
        ev('calloc', n, size, 0)
        mm.gpr(3, 0)
        return
    a = bump(n * size if n * size < 0x10000 else 0x10000)
    ev('calloc', n, size, newid(a))
    mm.gpr(3, a)


def new_task(mm, kind, *args):
    if not take('tasks_ok', True):
        ev(kind, *args, 0)
        mm.gpr(3, 0)
        return
    t = bump(0x60)
    twp, mwp = bump(0x40), bump(0x40)
    m.put(t + 0x34, 'I', twp)
    m.put(t + 0x38, 'I', mwp)
    ev(kind, *args, newid(t))
    ctx['made'].append(t)
    mm.gpr(3, t)


def ccl_init(mm):
    t = mm.gpr(3)
    ev('ccl', idof(t), mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6))
    mm.gpr(3, 1)


def msg_open(mm):
    w = take('windows', 0)
    m.put(mm.gpr(7), 'I', w)
    ev('msg_open', mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6) - (1 << 32 if mm.gpr(6) >> 31 else 0),
       idof(mm.gpr(7) - 0x4C), mm.gpr(8))


def bone(mm):
    n, node, o = mm.gpr(3), mm.gpr(4), mm.gpr(5)
    r = take('bones', None)
    ev('bone', n, node, 0 if r is None else 1)
    if r is not None:
        put_vec(o, r)
    mm.gpr(3, 0 if r is None else 1)


def chara(mm):
    v = take('chara', 0)
    ev('chara', mm.gpr(3), v)
    mm.gpr(3, v)


def draw_plain(mm):
    v, n = mm.gpr(3), mm.gpr(4)
    ev('draw', [vec(v + 16 * k) + [m.get(v + 16 * k + 12, 'I')] for k in range(n)], mm.gpr(5))


def draw_tex(mm):
    v, n = mm.gpr(3), mm.gpr(4)
    ev('draw_tex', [[m.get(v + 24 * k + 4 * j, 'I') for j in range(6)] for k in range(n)], mm.gpr(5))


m.stub(0x801197FC, calloc)
m.stub(0x80119828, lambda mm: ev('free', idof(mm.gpr(3))))
m.stub(0x8011A688, draw_plain)
m.stub(0x80116E34, lambda mm: ev('settexnum', mm.gpr(3)))
m.stub(0x8011E368, draw_tex)
m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), ctx['out'])[1]))
m.stub(0x8000F8B8, ccl_init)
m.stub(0x8000F4EC, lambda mm: ev('entry', idof(mm.gpr(3))))
m.stub(0x800709F8, lambda mm: new_task(mm, 'child', mm.gpr(3), mm.gpr(4) - S1(0), idof(mm.gpr(5))))
m.stub(0x80071E0C, lambda mm: new_task(mm, 'elem', mm.gpr(3), mm.gpr(4), mm.gpr(5) - S1(0), mm.gpr(6) - S5(0)))
m.stub(0x80071AF0, lambda mm: ev('destroy', idof(mm.gpr(3))))
m.stub(0x800E00E8, msg_open)
m.stub(0x8006B978, lambda mm: ev('se', mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6)))
m.stub(0x8011C2BC, lambda mm: ev('fog', 0))
m.stub(0x8011C2B0, lambda mm: ev('fog', 1))
m.stub(0x8011C310, lambda mm: ev('setfog'))
m.stub(0x801179F8, lambda mm: ev('blend', mm.gpr(3), mm.gpr(4)))
m.stub(0x80116E18, lambda mm: ev('settexture', mm.gpr(3) - S5(0)))
m.stub(0x80121BDC, lambda mm: ev('model', mm.gpr(3) - S5(0), get_matrix()))
# Module helpers the port leaves out (the shadow texture).
m.stub(S1(0x7344), lambda mm: ev('shadow_init', idof(mm.gpr(3) - 0x84), mm.gpr(4)))
m.stub(S1(0x7400), lambda mm: ev('shadow_free', idof(mm.gpr(3) - 0x84)))
# The movable set-up / release 0x4B44 / 0x4BA8 (ObjectMovableInitialize
# included) and the placement 0x14608 / 0x148A4 run as compiled; the body
# comes from the syCalloc stub above. The player queries they make:
m.stub(0x80038008, bone)
m.stub(0x80038108, chara)
m.stub(S1(0x6F94), lambda mm: ev('shadow_capture', idof(mm.gpr(3) - 0x84), fb(f32(mm.fpr(1))), vec(mm.gpr(4)),
                                   mm.gpr(5), mm.gpr(6), fb(f32(mm.fpr(2)))))
m.stub(S1(0x72D0), lambda mm: ev('shadow_end', idof(mm.gpr(3) - 0x84)))
m.stub(S1(0x7440), lambda mm: ev('shadow_draw', mm.gpr(3), vec(mm.gpr(4)), fb(f32(mm.fpr(1))), idof(mm.gpr(5) - 0x84)))


# --- trails ------------------------------------------------------------------
def make_trail(cap, fill):
    """A trail buffer in memory with a random plausible state."""
    hdr = bump(0x14)
    pts = bump(12 * cap)
    tid, pid = newid(hdr), newid(pts)
    m.put(hdr, 'I', pts)
    m.put(hdr + 4, 'i', cap)
    if fill:
        count = rng.choice([0, 1, 2, cap, cap, rng.randrange(0, cap + 1)])
        head = rng.randrange(cap)
        spread = rng.choice([0.5, 3.0, 50.0, 400.0])
        base = [fin() for _ in range(3)]
        for k in range(cap):
            p = [f32(base[j] + rng.uniform(-spread, spread)) for j in range(3)]
            if rng.random() < 0.05:
                p[rng.randrange(3)] = rng.choice([0.0, -0.0, 1e6])
            put_vec(pts + 12 * k, [fb(x) for x in p])
        m.put(hdr + 8, 'i', head)
        m.put(hdr + 12, 'i', count)
        m.put(hdr + 16, 'i', rng.randrange(-20, 1000))
    return hdr, tid, pid


def trail_state(hdr):
    if hdr == 0:
        return None
    pts, cap = m.get(hdr, 'I'), m.get(hdr + 4, 'i')
    return {'id': idof(hdr), 'points_id': idof(pts),
            'points': [vec(pts + 12 * k) for k in range(cap)],
            'head': m.get(hdr + 8, 'i'), 'count': m.get(hdr + 12, 'i'), 'last': m.get(hdr + 16, 'i')}


def put_desc(a, d):
    clear(a, 0x20)
    m.put(a, 'I', d['flags'])
    m.put(a + 4, 'I', d['head_w'])
    m.put(a + 8, 'I', d['tail_w'])
    m.put(a + 0xC, 'I', d['head_c'])
    m.put(a + 0x10, 'I', d['tail_c'])
    m.put(a + 0x18, 'I', d['tex'])


def rnd_desc():
    return {'flags': rng.choice([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xA, 0xC, 0xD, 0xF, rng.randrange(1 << 32)]),
            'head_w': rnd_bits(-5.0, 30.0), 'tail_w': rnd_bits(-5.0, 30.0),
            'head_c': rng.randrange(1 << 32), 'tail_c': rng.randrange(1 << 32), 'tex': rng.randrange(1 << 32)}


def set_clock(frame):
    m.put(GAME_STATE, 'h', frame['state'])
    m.put(CLOCK + 0x7C, 'i', frame['frame'])
    m.put(CLOCK + 0x37, 'b', frame['reset'])


def rnd_frame(last):
    return {'state': rng.choice([0, 0, 0, 0x11, 0x10, 0x12, -1]),
            'frame': (last if last is not None else 0) + rng.choice([0, 0, 1, 1, 1, 2, 3, 4, 5, 9, -1, -3, 100]),
            'reset': rng.choice([0, 0, 1, -1]),
            'point': [rnd_bits() for _ in range(3)],
            'matrix': rnd_matrix()}


def trail():
    D = SCRATCH + 0x100
    P = SCRATCH + 0x140
    # Allocation and free, with each syCalloc failing or not.
    for _ in range(40):
        reset_case()
        n = rng.choice([1, 2, 8, 8, 12, 0x20])
        plan = [rng.random() > 0.25 for _ in range(2)]
        ctx['calloc_ok'] = list(plan)
        m.call(S1(0x635C), r3=D, r4=n)
        hdr = m.gpr(3)
        alloc_ev = list(events)
        st = trail_state(hdr)
        events.clear()
        m.call(S1(0x6EA4), r3=D, r4=hdr)
        out['cases'].append({'kind': 'alloc', 'n': n, 'calloc_ok': plan, 'alloc_events': alloc_ev,
                             'trail': st, 'free_events': list(events)})
    # Drop-oldest.
    for _ in range(40):
        reset_case()
        cap = rng.choice([1, 2, 5, 8])
        hdr, _, _ = make_trail(cap, True)
        st0 = trail_state(hdr)
        m.call(S1(0x6EE4), r3=hdr)
        out['cases'].append({'kind': 'drop', 'trail': st0, 'ret': m.gpr(3), 'after': trail_state(hdr)})
    # Steps: a run of frames on one buffer.
    for c in range(260):
        reset_case()
        cap = rng.choice([1, 2, 3, 4, 8, 8, 8, 12])
        hdr, _, _ = make_trail(cap, rng.random() < 0.8)
        d = rnd_desc() if c % 4 else {'flags': 8, 'head_w': m.get(S5(0x173C0), 'I'),
                                      'tail_w': m.get(S5(0x173C4), 'I'), 'head_c': m.get(S5(0x173C8), 'I'),
                                      'tail_c': m.get(S5(0x173CC), 'I'), 'tex': 0}
        no_desc = rng.random() < 0.05
        put_desc(D, d)
        st0 = trail_state(hdr)
        frames = []
        last = st0['last']
        for _ in range(rng.choice([1, 3, 8, 12])):
            fr = rnd_frame(last)
            last = fr['frame']
            set_clock(fr)
            put_matrix(fr['matrix'])
            put_vec(P, fr['point'])
            events.clear()
            m.call(S1(0x6E50), r3=0 if no_desc else D, r4=hdr, r5=P)
            assert get_matrix() == fr['matrix']
            fr['events'] = list(events)
            fr['after'] = trail_state(hdr)
            frames.append(fr)
        out['cases'].append({'kind': 'step', 'desc': None if no_desc else d, 'trail': st0, 'frames': frames})
    # The REL's own descriptor (stg42D section 5 +0x173BC).
    out['messenger_desc'] = [m.get(S5(0x173BC) + 4 * k, 'I') for k in range(7)]


# --- Omochao -----------------------------------------------------------------
def make_task(mode, has_mwp=True):
    t = bump(0x60)
    tid = newid(t)
    twp = bump(0x40)
    m.put(t + 0x34, 'I', twp)
    if has_mwp:
        m.put(t + 0x38, 'I', bump(0x40))
    m.put(t + 0x4C, 'i', mode)
    m.put(twp + 2, 'B', rng.randrange(256))
    m.put(twp + 4, 'h', rng.choice([0, 0x8000 - 0x10000, 0x10, rng.randrange(-0x8000, 0x8000)]))
    m.put(twp + 6, 'H', rng.randrange(0x10000))
    for k in range(3):
        m.put(twp + 8 + 4 * k, 'i', rng.choice([0, 0x4000, rng.randrange(-(1 << 31), 1 << 31)]))
    put_vec(twp + 0x14, [rnd_bits() for _ in range(3)])
    put_vec(twp + 0x20, [rnd_bits(), fb(rng.choice([0.0, 0.5, 1.0, 2.0, -1.0, float('nan')])), rnd_bits()])
    return t, tid


def task_state(t):
    twp, mwp, cwp, awp = (m.get(t + o, 'I') for o in (0x34, 0x38, 0x3C, 0x40))
    cb = [m.get(t + o, 'I') for o in (0x10, 0x14, 0x18, 0x20, 0x2C)]
    s = {'callbacks': [x - S1(0) if x else 0 for x in cb], 'mode': m.get(t + 0x4C, 'i'),
         'mwp_30': m.get(mwp + 0x30, 'I') if mwp else None,
         'land': get_words(cwp) if cwp else None,
         'twp': {'mode': m.get(twp, 'B'), 'flag1': m.get(twp + 1, 'B'), 'id': m.get(twp + 2, 'B'),
                 'flags': m.get(twp + 4, 'h'), 'wtimer': m.get(twp + 6, 'H'),
                 'ang': [m.get(twp + 8 + 4 * k, 'i') for k in range(3)], 'pos': vec(twp + 0x14),
                 'scale': vec(twp + 0x20)}}
    if awp:
        s['work'] = {'id': idof(awp), 'pos': vec(awp), 'anchor': vec(awp + 0x10), 'bob': m.get(awp + 0x24, 'i'), 'f30': m.get(awp + 0x30, 'I'),
                     'sub_task': idof(m.get(awp + 0x3C, 'I')), 'home': vec(awp + 0x40), 'w4c': m.get(awp + 0x4C, 'i'),
                     'w50': m.get(awp + 0x50, 'I'), 'player': m.get(awp + 0x54, 'i'),
                     'trails': [trail_state(m.get(awp + 0x74 + 4 * k, 'I')) for k in range(4)]}
    else:
        s['work'] = None
    return s


def plans(n_calloc=8):
    ctx['calloc_ok'] = [rng.random() > 0.08 for _ in range(n_calloc)]
    ctx['tasks_ok'] = [rng.random() > 0.15 for _ in range(2)]
    ctx['windows'] = [rng.choice([0, rng.randrange(0x1000, 1 << 32)])]
    ctx['bones'] = [rng.choice([None, [rnd_bits() for _ in range(3)]]) for _ in range(2)]
    ctx['chara'] = [rng.choice([0, 1, 6, 7]) for _ in range(2)]
    p = {k: list(ctx[k]) for k in ('calloc_ok', 'tasks_ok', 'windows', 'bones', 'chara')}
    # Player 0 for 0x14608's fallback, and the rand state for 0x148A4.
    p['player0'] = None
    m.put(PLAYERTWP, 'I', 0)
    if rng.random() < 0.8:
        pt = bump(0x40)
        p['player0'] = [rnd_bits() for _ in range(3)]
        put_vec(pt + 0x14, p['player0'])
        m.put(PLAYERTWP, 'I', pt)
    p['seed'] = rng.randrange(1 << 32)
    m.put(SEED, 'I', p['seed'])
    return p


def bank():
    count = rng.choice([-1, 0, 3, 10])
    entries = [rng.randrange(1 << 32) for _ in range(max(count, 0))]
    b = bump(4 * max(len(entries), 1))
    for k, e in enumerate(entries):
        m.put(b + 4 * k, 'I', e)
    m.put(S5(0x1747C), 'i', count)
    m.put(S6(0x698), 'I', b)
    return count, entries


def messenger():
    # Object-list set-up (0x182AC) then the destructor (0x1850C).
    for _ in range(150):
        reset_case()
        mode = rng.choice([0, 0, 0, 1, 2])
        has_mwp = rng.random() > 0.1
        t, tid = make_task(mode, has_mwp)
        ctx['out'] = rng.random() < 0.2
        p = plans()
        before = task_state(t)
        m.call(S1(0x182AC), r3=t)
        created = task_state(t)
        children = [[idof(c), vec(m.get(c + 0x34, 'I') + 0x14), m.get(m.get(c + 0x34, 'I') + 2, 'B')]
                    for c in ctx['made']]
        ev_create = list(events)
        events.clear()
        awp = m.get(t + 0x40, 'I')
        sub = 0
        if awp and rng.random() < 0.5:
            sub = bump(0x60)
            newid(sub)
            m.put(awp + 0x3C, 'I', sub)
        ev_destroy = None
        after = None
        if awp:
            m.call(S1(0x1850C), r3=t)
            ev_destroy = list(events)
            after = m.get(t + 0x40, 'I')
        out['cases'].append({'kind': 'create', 'id': tid, 'out': ctx['out'], 'has_mwp': has_mwp, 'plans': p,
                             'before': before, 'events': ev_create, 'created': created, 'children': children,
                             'sub_task': idof(sub) if sub else 0, 'destroy_events': ev_destroy, 'after': after,
                             'next_id': ctx['next'], 'seed_after': m.get(SEED, 'I')})
    # The spawner (0x18620).
    for _ in range(200):
        reset_case()
        parent = 0
        if rng.random() < 0.5:
            parent = bump(0x60)
            newid(parent)
        mode = rng.choice([0, 1, 2, 2, 2, -1, 3])
        p = plans()
        if mode == 2:
            # A spawned Omochao without its work would write through NULL.
            ctx['calloc_ok'][0] = p['calloc_ok'][0] = True
        count, entries = bank()
        lang = rng.choice([0, 1, 5, -1])
        m.put(CLOCK + 0x11, 'b', lang)
        P = bump(12)
        pos = [rnd_bits() for _ in range(3)]
        put_vec(P, pos)
        args = {'pos': pos, 'yaw': rng.randrange(-(1 << 31), 1 << 31), 'w50': rng.randrange(1 << 32),
                'message': rng.choice([-1, 0, 1, 2, 5, 9, 10, 200]), 'mode': mode,
                'parent': idof(parent) if parent else None}
        first = ctx['next']
        m.call(S1(0x18620), r3=P, r4=args['yaw'] & 0xffffffff, r5=args['w50'], r6=args['message'] & 0xffffffff,
               r7=mode & 0xffffffff, r8=parent)
        r = m.gpr(3)
        case = {'kind': 'spawn', 'args': args, 'plans': p, 'count': count, 'entries': entries, 'language': lang,
                'first_id': first, 'events': list(events), 'ret': idof(r) if r else 0,
                'seed_after': m.get(SEED, 'I')}
        if r and mode == 2:
            case['task'] = task_state(r)
            case['children'] = [[idof(c), vec(m.get(c + 0x34, 'I') + 0x14), m.get(m.get(c + 0x34, 'I') + 2, 'B')]
                                for c in ctx['made'] if c != r]
        elif r:
            case['window'] = m.get(r + 0x4C, 'I')
            # The message task's exec and destructor (0x185C8, 0x185F4).
            events.clear()
            m.call(S1(0x185C8), r3=r)
            case['exec_events'] = list(events)
            events.clear()
            m.call(S1(0x185F4), r3=r)
            case['dest_events'] = list(events)
        out['cases'].append(case)
    # The collision child's exec (0x1826C).
    for _ in range(40):
        reset_case()
        t = bump(0x60)
        tid = newid(t)
        kind = rng.choice(['none', 'dying', 'alive'])
        if kind != 'none':
            par = bump(0x60)
            newid(par)
            m.put(par + 0x10, 'I', 0x80071AF0 if kind == 'dying' else rng.choice([S1(0x156CC), 0, 0x80071AF4]))
            m.put(t + 8, 'I', par)
            pexec = m.get(par + 0x10, 'I')
        else:
            pexec = None
        m.call(S1(0x1826C), r3=t)
        out['cases'].append({'kind': 'child', 'id': tid, 'parent_exec': pexec, 'events': list(events)})


def display():
    for c in range(240):
        reset_case()
        t, tid = make_task(rng.choice([0, 2]))
        awp = bump(0xDC)
        m.put(t + 0x40, 'I', awp)
        newid(awp)
        put_vec(awp, [rnd_bits() for _ in range(3)])
        m.put(awp + 0x24, 'i', rng.choice([0, 0x4000, rng.randrange(-(1 << 31), 1 << 31)]))
        player = rng.choice([0, 1])
        m.put(awp + 0x54, 'i', player)
        for k in range(4):
            # (A NULL trail here would be read through: the step does not check.)
            hdr, _, _ = make_trail(8, rng.random() < 0.8)
            m.put(awp + 0x74 + 4 * k, 'I', hdr)
        pang = []
        for n in range(2):
            ptwp = bump(0x40)
            a = [rng.randrange(-(1 << 31), 1 << 31) for _ in range(3)]
            for k in range(3):
                m.put(ptwp + 8 + 4 * k, 'i', a[k])
            m.put(PLAYERTWP + 4 * n, 'I', ptwp)
            pang.append(a)
        screen = rng.choice([0, 1])
        m.put(SCREEN, 'i', screen)
        fr = rnd_frame(None)
        fr['frame'] = rng.randrange(-20, 1000)
        set_clock(fr)
        mat = rnd_matrix()
        put_matrix(mat)
        twp = m.get(t + 0x34, 'I')
        # Bring the clip depth into play: often put the point near z = 100.
        if rng.random() < 0.6:
            put_vec(twp + 0x14, [rnd_bits(-60, 60) for _ in range(3)])
        before = task_state(t)
        fn = 0x18040 if c % 3 else 0x188FC
        m.call(S1(fn), r3=t)
        assert get_matrix() == mat
        out['cases'].append({'kind': 'display', 'fn': fn, 'id': tid, 'player_angles': pang, 'screen': screen,
                             'clock': fr, 'matrix': mat, 'before': before, 'events': list(events),
                             'after': task_state(t)})


{'trail': trail, 'messenger': messenger, 'display': display}[which]()
print(json.dumps(out))
