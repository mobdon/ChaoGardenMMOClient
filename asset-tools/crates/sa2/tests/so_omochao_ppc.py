"""Run stg42D.rel's Omochao code (the exec 0x156CC and the main display
0x17468, with their helpers) under the main.dol harness and print JSON
(every float as its bit pattern). tests/so_omochao.rs replays each case
with `stage_objects::messenger::omochao` and compares. Usage: python
so_omochao_ppc.py <exec|display>

The REL code (with the movable-object update 0x4BE0) runs as compiled, with main.dol's rand, atan2f, njSin,
njScalor, njDistanceP2P, fn_80037C84 and the Ninja matrix routines.
Engine and player routines are stubbed and recorded as events, with the
values they return; so is the module's trail effect 0x1DFE0, which the port leaves to the
host. Tasks are numbered in the order they appear.
"""
import json, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403
from so_dynland_ppc import dyn_stubs, get_words, rnd_body  # noqa: E402
import gekko_tables  # noqa: E402

if gekko_tables.load() is None:
    sys.stderr.write('needs the local Gekko estimate tables (njScalor)\n')
    sys.exit(0)

which = sys.argv[1]
out = {'object': which, 's5': S5(0), 'cases': []}
CUR = 0x803B52C0 - 0x71FC          # Ninja's current-matrix pointer
CLOCK = 0x801CC168                 # +0x11 language, +0x37 pause, +0x42, +0x7C frame
SEED = 0x803B52C0 - 0x7C20         # rand's state (r13 -0x7C20)
PLAYERTWP = 0x801E7768
PLAYERTP = 0x801E7788
PLAYERPWP = 0x801E7728             # the entries 0x6300 and GetPlayerCharacter read
LIGHT = 0x801E5870
HEAP = 0x81210000
CALLBACK = 0x81200000              # a force object's callback (stubbed)
TASK = 0x81201000                  # the task
LAND = 0x81202000                  # its movable body (task +0x3C)

NODES = [0x14a1c, 0x13894, 0x16e54, 0x16a74, 0x16094, 0x15b54, 0x152b4, 0x14e34, 0x156d4,
         0x1462c, 0x14324, 0x13ac0, 0x136ec, 0x13038, 0x12cac, 0x13360]
# Model words the display reads or writes: each node's 0x2C bytes, the
# clipped-model table and the head model.
MODEL_WORDS = sorted({n + 4 * k for n in NODES for k in range(11)} |
                     {0x173ec + k for k in (0xc, 0x1c, 0x3c, 0x5c, 0x6c, 0x7c, 0x8c)} | {0x17338})

ctx = {}


def reset_case():
    ctx.update(heap=HEAP, ids={}, next=1, q={}, forces={})
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
    q = ctx['q'].get(key, [])
    return q.pop(0) if q else default


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def put_vec(a, v):
    for k, x in enumerate(v):
        m.put(a + 4 * k, 'I', x)


def fin(lo=-500.0, hi=500.0):
    while True:
        x = rnd_f(lo, hi)
        if abs(x) < 1e29:
            return x


def rb(lo=-500.0, hi=500.0):
    return fb(fin(lo, hi))


def put_matrix(mat):
    m.u.mem_write(m.get(CUR, 'I'), struct.pack('>12I', *mat))


def get_matrix():
    return [m.get(m.get(CUR, 'I') + 4 * k, 'I') for k in range(12)]


def rnd_matrix():
    if rng.random() < 0.3:
        return [fb(x) for x in (1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0)]
    return [fb(fin(-2.0, 2.0)) if (i % 4) != 3 else fb(fin(-300.0, 300.0)) for i in range(12)]


def s32(x):
    return x - (1 << 32) if x >> 31 else x


def rang():
    return rng.choice([0, 0, 0x10000, 0x4000, -0x4000, 0x7fff, -0x8000, 0x27d, rng.randrange(-0x3000, 0x3000),
                       rng.randrange(-(1 << 31), 1 << 31)])


# --- stubs ------------------------------------------------------------------------
def q_ret(key, default, name):
    def stub(mm):
        v = take(key, default)
        args = [idof(mm.gpr(3))] if name in ('isHit', 'f63f4', 'f6574', 'movable') else []
        ev(name, *args, v)
        mm.gpr(3, (idof_rev(v) if name == 'isHit' else v) & 0xffffffff)
    return stub


def idof_rev(v):
    # Task ids handed out as results are turned back into their addresses.
    for a, i in ctx['ids'].items():
        if i == v and v:
            return a
    return v


def msg_e8(mm):
    w = take('win', 0)
    m.put(mm.gpr(7), 'I', w)
    ev('e8', s32(mm.gpr(3)), mm.gpr(4), mm.gpr(5), s32(mm.gpr(6)), w)


def msg_0118(mm):
    w = take('win', 0)
    m.put(mm.gpr(7), 'I', w)
    ev('0118', s32(mm.gpr(3)), mm.gpr(4), mm.gpr(5), w)


def node_pos(mm):
    n, node, o = mm.gpr(3), mm.gpr(4), mm.gpr(5)
    r = take('bone', None)
    if r is None:
        ev('bone', s32(n), node, 0)
        mm.gpr(3, 0)
    else:
        put_vec(o, r)
        ev('bone', s32(n), node, 1)
        mm.gpr(3, 1)


def palloc(mm):
    if not take('palloc', True):
        ev('palloc', mm.gpr(3) - S5(0), 0)
        mm.gpr(3, 0)
        return
    p = bump(0x40)
    ctx['particle'] = p
    ev('palloc', mm.gpr(3) - S5(0), 1)
    mm.gpr(3, p)


def force_cb(mm):
    node, pos, buf = mm.gpr(3), mm.gpr(4), mm.gpr(5)
    f = ctx['forces'][node]
    ev('force', f['i'], vec(pos))
    m.put(buf, 'I', m.get(buf, 'I') | f['flags'])
    if f['vector'] is not None:
        put_vec(buf + 4, f['vector'])
    if f['direction'] is not None:
        put_vec(buf + 0x10, f['direction'])


m.stub(0x800655F4, ret(lambda mm: (ev('range_out', ctx['out']), ctx['out'])[1]))
m.stub(0x800E018C, q_ret('onscreen', 0, 'onscreen'))
m.stub(0x800E00E8, msg_e8)
m.stub(0x800E0118, msg_0118)
m.stub(0x8006B978, lambda mm: ev('se', mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6)))
m.stub(0x8006B7EC, lambda mm: ev('se_pos', mm.gpr(3), mm.gpr(4), mm.gpr(5), mm.gpr(6), vec(mm.gpr(7))))
m.stub(0x8006AFFC, lambda mm: ev('se_timer', mm.gpr(3), ctx['twp2id'].get(mm.gpr(4), 0), mm.gpr(5), mm.gpr(6),
                                 mm.gpr(7), vec(mm.gpr(8))))
m.stub(0x800E2828, lambda mm: ev('vanish', vec(mm.gpr(3)), vec(mm.gpr(4)), fb(f32(mm.fpr(1))), fb(f32(mm.fpr(2)))))
m.stub(0x80071AE0, lambda mm: ev('freetask', idof(mm.gpr(3))))
m.stub(0x800063F4, q_ret('f63f4', 0, 'f63f4'))
m.stub(0x800066F4, q_ret('isHit', 0, 'isHit'))
m.stub(0x80006574, q_ret('f6574', 0, 'f6574'))
m.stub(0x8001DD78, q_ret('stage', 0, 'stage'))
m.stub(0x80038108, lambda mm: (lambda v: (ev('chara', s32(mm.gpr(3)), v), mm.gpr(3, v & 0xffffffff)))(take('chara', 0)))
m.stub(0x80038008, node_pos)
m.stub(0x80032B78, palloc)
# The movable helpers (0x4BE0 here) run as compiled; their land probes, syCalloc /
# syFree and CCL_Entry are so_dynland_ppc.py's stubs.
dyn_stubs(TASK, LAND, {'calloc': True})
m.stub(S1(0x1DFE0), lambda mm: ev('trail_fx', vec(mm.gpr(3)), vec(mm.gpr(4)), fb(f32(mm.fpr(1))), fb(f32(mm.fpr(2)))))
m.stub(CALLBACK, force_cb)
# Rendering.
for a in (0x8011F518, 0x8011E154, 0x8011DF1C):
    m.stub(a, (lambda a: lambda mm: ev('light', a, [fb(f32(mm.fpr(1))), fb(f32(mm.fpr(2)))]))(a))
for a in (0x8011E150, 0x8011F51C, 0x8011DF18):
    m.stub(a, (lambda a: lambda mm: ev('light', a, [fb(f32(mm.fpr(k))) for k in (1, 2, 3)]))(a))
m.stub(0x8011C2BC, lambda mm: ev('fog', 0))
m.stub(0x8011C2B0, lambda mm: ev('fog', 1))
m.stub(0x8011C310, lambda mm: ev('setfog'))
m.stub(0x8011E244, lambda mm: ev('disc', mm.gpr(3), get_matrix()))
m.stub(0x8002B2E8, lambda mm: ev('ctrl3d', 1, mm.gpr(3)))
m.stub(0x8002B2D8, lambda mm: ev('ctrl3d', 0, mm.gpr(3)))
m.stub(0x80116E18, lambda mm: ev('settexture', mm.gpr(3) - S5(0)))
for a in (0x80121BDC, 0x8011F514, 0x8011E158):
    m.stub(a, (lambda a: lambda mm: ev('draw', a, mm.gpr(3), get_matrix()))(a))
m.stub(S1(0x6294), lambda mm: ev('draw', 0x6294, mm.gpr(3), get_matrix()))
m.stub(0x80024CB8, lambda mm: ev('light_reset', mm.gpr(3)))

PRISTINE = {o: m.get(S5(o), 'I') for o in MODEL_WORDS}


# --- the task -------------------------------------------------------------------
def trail_hdr(cap):
    hdr, pts = bump(0x14), bump(12 * cap)
    newid(hdr)
    newid(pts)
    m.put(hdr, 'I', pts)
    m.put(hdr + 4, 'i', cap)
    m.put(hdr + 8, 'i', rng.randrange(cap))
    m.put(hdr + 12, 'i', rng.choice([0, 1, 2, cap, rng.randrange(cap + 1)]))
    m.put(hdr + 16, 'i', rng.randrange(1000))
    return hdr


def trail_state(hdr):
    return {'id': idof(hdr), 'points_id': idof(m.get(hdr, 'I')), 'cap': m.get(hdr + 4, 'i'),
            'head': m.get(hdr + 8, 'i'), 'count': m.get(hdr + 12, 'i'), 'last': m.get(hdr + 16, 'i')}


WORK_I = {'speed': (0xC, 'I'), 'spin': (0x1C, 'i'), 'w20': (0x20, 'i'), 'bob': (0x24, 'i'), 'swing': (0x28, 'i'),
          'tilt': (0x2C, 'i'), 'scale': (0x30, 'I'), 'timer': (0x34, 'i'), 'talk': (0x38, 'i'),
          'w4c': (0x4C, 'i'), 'follow': (0x50, 'i'), 'player': (0x54, 'i'), 'flash': (0x58, 'I'),
          'flash_target': (0x5C, 'I'), 'chatter': (0x60, 'i'), 'away': (0x64, 'i'), 'shake': (0x68, 'I'),
          'quiet': (0x6C, 'i'), 'ridden': (0x70, 'i')}


def make_task():
    # The task and its movable body sit at fixed addresses (the body stubs
    # from so_dynland_ppc.py are installed once for them).
    t = TASK
    clear(t, 0x60)
    tid = newid(t)
    twp, mwp, awp = bump(0x40), bump(0x40), bump(0xDC)
    newid(awp)
    ctx['twp2id'] = {twp: tid}
    for o, a in ((0x34, twp), (0x38, mwp), (0x3C, LAND), (0x40, awp)):
        m.put(t + o, 'I', a)
    m.put(t + 0x4C, 'i', rng.choice([0, 0, 2]))
    if rng.random() < 0.7:
        cwp = bump(0x40)
        m.put(cwp + 4, 'H', rng.choice([0, 0x10, 1, rng.randrange(0x10000)]))
        m.put(twp + 0x2C, 'I', cwp)
    # twp
    m.put(twp, 'b', rng.choice([0, 0, 1, 1, 2, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, -1]))
    m.put(twp + 1, 'B', rng.randrange(8))
    m.put(twp + 3, 'B', rng.choice([0, 0, 1, 2, 9, 255]))
    m.put(twp + 4, 'h', rng.choice([0, 0, 4, -0x8000, -0x8000 + 4, rng.randrange(-0x8000, 0x8000)]))
    m.put(twp + 6, 'H', rng.randrange(0x10000))
    for k in range(3):
        m.put(twp + 8 + 4 * k, 'i', rang())
    home = [rb() for _ in range(3)]
    pos = [rb() for _ in range(3)] if rng.random() < 0.5 else \
        [fb(f32(bf(home[k]) + rng.choice([0.0, rng.uniform(-30, 30), rng.uniform(-3, 3)]))) for k in range(3)]
    put_vec(twp + 0x14, pos)
    for k, w in enumerate(rnd_body(pos)):
        m.put(LAND + 4 * k, 'I', w)
    put_vec(twp + 0x20, [fb(f32(rng.choice([0.0, 1.0, 2.0, 5.0, -1.0, 3.7, 200.0]))),
                         fb(rng.choice([0.0, 0.5, 1.0, 1.5])), rb()])
    # mwp velocity
    put_vec(mwp, [fb(fin(-3, 3) if rng.random() < 0.8 else rng.choice([0.0, 0.01, -0.0])) for _ in range(3)])
    # work
    spread = rng.choice([None, 20.0, 12.0, 3.0])
    goal = [rb() for _ in range(3)] if spread is None else \
        [fb(f32(bf(pos[k]) + rng.uniform(-spread, spread))) for k in range(3)]
    if rng.random() < 0.15:
        # Arriving above home (mode 1).
        home[1] = fb(f32(bf(pos[1]) - rng.uniform(0, 5)))
    put_vec(awp, goal)
    put_vec(awp + 0x10, [rb() for _ in range(3)])
    put_vec(awp + 0x40, home)
    m.put(awp + 0xC, 'I', fb(f32(rng.choice([0.0, 0.1, 0.5, rng.uniform(0, 2)]))))
    for k in ('spin', 'w20', 'bob', 'swing', 'tilt'):
        m.put(awp + WORK_I[k][0], 'i', rang())
    m.put(awp + 0x30, 'I', fb(f32(rng.choice([0.0, 0.03, 0.5, 0.95, 1.0, 1.2]))))
    m.put(awp + 0x34, 'i', rng.choice([0, 0, 0, 1, 2, 0x6e, 0x6e, 0x6f, 0x78, 300]))
    m.put(awp + 0x38, 'i', rng.choice([0, 0, 1, 2, 300]))
    m.put(awp + 0x3C, 'I', rng.choice([0, 0, rng.randrange(1, 1 << 32)]))
    m.put(awp + 0x4C, 'i', rng.choice([-1, 0, 3]))
    m.put(awp + 0x50, 'i', rng.choice([0, 1]))
    m.put(awp + 0x54, 'i', rng.choice([0, 1]))
    m.put(awp + 0x58, 'I', fb(f32(rng.choice([0.0, 0.005, 0.02, 0.5, 1.0]))))
    m.put(awp + 0x5C, 'I', fb(f32(rng.choice([0.0, 0.5, 1.0]))))
    m.put(awp + 0x60, 'i', rng.choice([0, 1, 2, 2, 2, 3, 4]))
    m.put(awp + 0x64, 'i', rng.choice([0, 0, 10, 2998, 2999, 3000, 5998, 6000, 6001]))
    m.put(awp + 0x68, 'I', fb(fin(-10, 10)))
    m.put(awp + 0x6C, 'i', rng.choice([0, 150, 151, 300, 599, 600, 600]))
    m.put(awp + 0x70, 'i', rng.choice([0, 0, 1, 5, 999, 1000]))
    for k in range(4):
        m.put(awp + 0x74 + 4 * k, 'I', trail_hdr(8))
    # collision child
    if rng.random() < 0.8:
        c = bump(0x60)
        newid(c)
        ctwp, ccwp = bump(0x40), bump(0x40)
        m.put(c + 0x34, 'I', ctwp)
        m.put(ctwp + 4, 'h', rng.choice([0, 4, rng.randrange(-0x8000, 0x8000)]))
        put_vec(ctwp + 0x14, [rb() for _ in range(3)])
        m.put(ctwp + 0x2C, 'I', ccwp)
        n = rng.choice([0, 1, 1, 2])
        infos = bump(0x30 * max(n, 1))
        m.put(ccwp + 6, 'H', n)
        m.put(ccwp + 0xC, 'I', infos)
        for k in range(n):
            m.put(infos + 0x30 * k + 4, 'I', rng.randrange(1 << 32))
        m.put(t + 0xC, 'I', c)
    return t, tid


def task_state(t):
    twp, mwp, awp = (m.get(t + o, 'I') for o in (0x34, 0x38, 0x40))
    s = {'mode': m.get(t + 0x4C, 'i'), 'vel': vec(mwp), 'land': get_words(LAND),
         'cwp_flag': m.get(m.get(twp + 0x2C, 'I') + 4, 'H') if m.get(twp + 0x2C, 'I') else None,
         'twp': {'mode': m.get(twp, 'b'), 'flag1': m.get(twp + 1, 'B'), 'btimer': m.get(twp + 3, 'B'),
                 'flags': m.get(twp + 4, 'h'), 'wtimer': m.get(twp + 6, 'H'),
                 'ang': [m.get(twp + 8 + 4 * k, 'i') for k in range(3)], 'pos': vec(twp + 0x14),
                 'scale': vec(twp + 0x20)}}
    w = {'id': idof(awp), 'goal': vec(awp), 'anchor': vec(awp + 0x10), 'home': vec(awp + 0x40),
         'window': m.get(awp + 0x3C, 'I'), 'trails': [trail_state(m.get(awp + 0x74 + 4 * k, 'I')) for k in range(4)]}
    for k, (o, f) in WORK_I.items():
        w[k] = m.get(awp + o, f)
    s['work'] = w
    c = m.get(t + 0xC, 'I')
    if c:
        ctwp = m.get(c + 0x34, 'I')
        ccwp = m.get(ctwp + 0x2C, 'I')
        infos = m.get(ccwp + 0xC, 'I')
        s['child'] = {'id': idof(c), 'flags': m.get(ctwp + 4, 'h'), 'pos': vec(ctwp + 0x14),
                      'attrs': [m.get(infos + 0x30 * k + 4, 'I') for k in range(m.get(ccwp + 6, 'H'))]}
    else:
        s['child'] = None
    return s


def players():
    ps = []
    for n in range(8):
        m.put(PLAYERTWP + 4 * n, 'I', 0)
        m.put(PLAYERTP + 4 * n, 'I', 0)
    for n in range(2):
        m.put(PLAYERPWP + 4 * n, 'I', 0)
        if n == 1 and rng.random() < 0.3:
            ps.append(None)
            continue
        p = bump(0x40)
        ptask = bump(0x60)
        newid(ptask)
        ang = [rang() for _ in range(3)]
        pos = [rb() for _ in range(3)]
        for k in range(3):
            m.put(p + 8 + 4 * k, 'i', ang[k])
        put_vec(p + 0x14, pos)
        m.put(PLAYERTWP + 4 * n, 'I', p)
        m.put(PLAYERTP + 4 * n, 'I', ptask)
        ps.append({'ang': ang, 'pos': pos, 'task': idof(ptask)})
    return ps


def globals_(t, ps):
    g = {}
    g['language'] = rng.choice([0, 1, -1])
    g['pause'] = rng.choice([0, 0, 0, 0, 1])
    g['b42'] = rng.choice([0, 0, 1])
    g['frame'] = rng.choice([0, 200, 400, 10, 15, rng.randrange(-100, 100000)])
    m.put(CLOCK + 0x11, 'b', g['language'])
    m.put(CLOCK + 0x37, 'b', g['pause'])
    m.put(CLOCK + 0x42, 'b', g['b42'])
    m.put(CLOCK + 0x7C, 'i', g['frame'])
    g['seed'] = rng.randrange(1 << 32)
    m.put(SEED, 'I', g['seed'])
    for key, cnt_off, tab_off in (('messages', 0x1747C, 0x698), ('hints', 0x17480, 0x69C)):
        cnt = rng.choice([-1, 0, 5, 0x20, 0x70])
        ent = [rng.randrange(1 << 32) for _ in range(max(cnt, 0))]
        b = bump(4 * max(len(ent), 1))
        for k, e in enumerate(ent):
            m.put(b + 4 * k, 'I', e)
        m.put(S5(cnt_off), 'i', cnt)
        m.put(S6(tab_off), 'I', b)
        g[key] = [cnt, ent]
    g['requests'] = [rng.choice([0, 0, 1, 2, 3, 4, -1]) for _ in range(2)]
    for k in range(2):
        m.put(S5(0x1748C + 4 * k), 'i', g['requests'][k])
    # The players' 0x801E7728 entries (+0x94 holds a task).
    g['holds'] = []
    for n in range(2):
        r = rng.random()
        if r < 0.3 or ps[n] is None:  # (0x6300's player is read through)
            g['holds'].append(None)
            continue
        e = bump(0xA0)
        h = t if r < 0.7 else rng.randrange(1, 1 << 32)
        m.put(e + 0x94, 'I', h)
        m.put(PLAYERPWP + 4 * n, 'I', e)
        g['holds'].append(idof(h) if h == t else h)
    # Force objects.
    nodes = []
    prev = S6(0x628)
    m.put(prev, 'I', 0)
    for i in range(rng.choice([0, 0, 1, 2])):
        node = bump(0x48)
        m.put(node + 0x38, 'I', CALLBACK)
        f = {'i': i, 'flags': rng.choice([0, 1, 2, 3, 8, 0xA, rng.randrange(16)]),
             'vector': [rb(-2, 2) for _ in range(3)] if rng.random() < 0.7 else None,
             'direction': [rb(-2, 2) for _ in range(3)] if rng.random() < 0.7 else None}
        ctx['forces'][node] = f
        nodes.append(f)
        m.put(prev if prev == S6(0x628) else prev + 0x40, 'I', node)
        prev = node
    g['forces'] = nodes
    g['fog'] = rng.choice([0, 1])
    g['alt'] = rng.choice([0, 0, 1])
    g['alt_model'] = rng.choice([0, rng.randrange(1, 1 << 32)])
    m.put(S6(0x630), 'i', g['fog'])
    m.put(S6(0x690), 'i', g['alt'])
    m.put(S6(0x694), 'I', g['alt_model'])
    g['light'] = [rb(-2, 2) for _ in range(8)]
    for k in range(8):
        m.put(LIGHT + 0x40 + 4 * k, 'I', g['light'][k])
    return g


def queues(ps):
    tasks = [0] + [p['task'] for p in ps if p]
    q = {'onscreen': [rng.choice([0, 0, 0, 1, 0x1234]) for _ in range(12)],
         'win': [rng.choice([0, rng.randrange(1, 1 << 32)]) for _ in range(4)],
         'f63f4': [rng.choice([0, 0, 0, 1])],
         'isHit': [rng.choice(tasks + [0, 0])],
         'f6574': [rng.choice([0, 0, 0, 1]) for _ in range(4)],
         'stage': [rng.choice([0, 2, 96, 97, rng.randrange(100), rng.choice([3, 4, 6, 8, 10, 13, 14, 16, 18, 22, 25, 26, 28, 30, 32, 34, 36, 38, 40, 44, 94, 95])])],
         'chara': [rng.choice([0, 6, 7, 1, 7]) for _ in range(6)],
         'bone': [rng.choice([None, [rb() for _ in range(3)]]) for _ in range(6)],
         'palloc': [rng.random() > 0.2]}
    ctx['q'] = {k: list(v) for k, v in q.items()}
    return q


def exec_cases():
    for _ in range(int(sys.argv[2]) if len(sys.argv) > 2 else 20000):
        reset_case()
        ctx['particle'] = 0
        t, tid = make_task()
        ps = players()
        g = globals_(t, ps)
        ctx['out'] = rng.random() < 0.1
        q = queues(ps)
        awp, twp = m.get(t + 0x40, 'I'), m.get(t + 0x34, 'I')
        fol = m.get(awp + 0x50, 'i')
        if ps[fol] and rng.random() < 0.3:
            # Following without a new place: the anchor is where 0x14608 puts
            # the player point (no bones), the goal 8..18 away.
            q['bone'] = ctx['q']['bone'] = [None] * 6
            put_vec(awp + 0x10, ps[fol]['pos'])
            m.put(twp + 1, 'B', m.get(twp + 1, 'B') | rng.choice([3, 3, 7]))
            put_vec(awp, [fb(f32(bf(x) + rng.choice([-1, 1]) * rng.uniform(5, 11))) for x in vec(twp + 0x14)])
        before = task_state(t)
        m.call(S1(0x156CC), r3=t)
        p = ctx['particle']
        out['cases'].append({'id': tid, 'out': ctx['out'], 'players': ps, 'globals': g, 'queues': q,
                             'before': before, 'events': list(events), 'after': task_state(t),
                             'seed_after': m.get(SEED, 'I'),
                             'requests_after': [m.get(S5(0x1748C + 4 * k), 'i') for k in range(2)],
                             'particle': [m.get(p + 4 * k, 'I') for k in range(16)] if p else None})


def display_cases():
    for c in range(int(sys.argv[2]) if len(sys.argv) > 2 else 1500):
        reset_case()
        for o, v in PRISTINE.items():
            m.put(S5(o), 'I', v)
        t, tid = make_task()
        ps = players()
        g = globals_(t, ps)
        if ps[1] is None:
            # (A ridden Omochao's player is read through.)
            m.put(m.get(t + 0x40, 'I') + 0x54, 'i', 0)
        twp = m.get(t + 0x34, 'I')
        if rng.random() < 0.7:
            put_vec(twp + 0x14, [rb(-150, 150) for _ in range(3)])
        # Some nodes get other angles (njRotateEx skips zero angles).
        for n in NODES:
            if rng.random() < 0.3:
                for k in range(3):
                    m.put(S5(n + 0x14 + 4 * k), 'i', rang())
        m.put(S5(0x13894), 'I', m.get(S5(0x13894), 'I') ^ rng.choice([0, 8]))
        mat = rnd_matrix()
        put_matrix(mat)
        before = task_state(t)
        model = {o: m.get(S5(o), 'I') for o in MODEL_WORDS}
        m.call(S1(0x17468), r3=t)
        assert get_matrix() == mat
        out['cases'].append({'id': tid, 'players': ps, 'globals': g, 'matrix': mat, 'before': before,
                             'model': [[o, v] for o, v in model.items()], 'events': list(events),
                             'after': task_state(t),
                             'model_after': [[o, m.get(S5(o), 'I')] for o in MODEL_WORDS]})


{'exec': exec_cases, 'display': display_cases}[which]()
print(json.dumps(out))
