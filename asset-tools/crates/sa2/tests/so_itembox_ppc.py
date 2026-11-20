"""Run stg42D.rel's item boxes (ITEMBOX 0x12028, ITEMBOXAIR 0x12FD0), the
item box HUD queue (itemBoxManager, 0x9EA0..0xA67C) and the bomb's
explosion manager (ExpManExec, 0x12C18..0x12FD0) under the main.dol
harness and print JSON (floats as bit patterns). tests/so_itembox.rs
replays every case with `stage_objects::itembox` and compares.

Usage: python so_itembox_ppc.py ground|air|queue|explosion|display

Engine routines (task and memory allocation, collision, SET state, score
and player routines, sound, rendering) are stubbed and recorded as events;
a stub that returns something draws the answer at random and records it as
the event's last element, so the replay can give the same answer. All the
stage REL code (the item functions, the HUD queue's callback, ExpMan) and
main.dol's matrix code for ITEMBOXAIR's collision centre run as compiled.
"""
import json, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403
from unicorn import UC_HOOK_CODE  # noqa: E402

T, PARENT, TWP, MWP = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x300
CWP, INFO, PWP, HIT = SCRATCH + 0x400, SCRATCH + 0x500, SCRATCH + 0x600, SCRATCH + 0x800
ARENA, ARENA_END = SCRATCH + 0x10000, SCRATCH + 0xF0000
GB, PLAYERPWP, HOOK, SCREEN, SCREENS = 0x801CC168, 0x801E7728, 0x803AD7E8, 0x803ADAD0, 0x803ADAD4
SLOTS, EXPMAN, FOG = S5(0xD9CC), S6(0x688), S6(0x630)
OTHER_HOOK = 0x80001234
which = sys.argv[1]
out = {'object': which, 'cases': []}
st = {'arena': ARENA, 'fail_create': 0.0, 'fail_calloc': 0.0}


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def alloc(n):
    a = st['arena']
    st['arena'] = (a + n + 0x1F) & ~0x1F
    assert st['arena'] < ARENA_END
    clear(a, n)
    return a


def who(a):
    return {T: 0, PARENT: 1}.get(a, 2)


def hook_class():
    v = m.get(HOOK, 'I')
    return 0 if v == 0 else 1 if v == S1(0x12F44) else 2


def bit(p):
    return int(rng.random() < p)


# --- stubs ------------------------------------------------------------------
def create_task(mm):
    ok = rng.random() >= st['fail_create']
    t = 0
    if ok:
        t = alloc(0x60)
        m.put(t + 0x34, 'I', alloc(0x40))
    ev('create_task', mm.gpr(3), mm.gpr(4), mm.gpr(5) - S1(0), mm.gpr(6) - S5(0), int(ok))
    mm.gpr(3, t)


def calloc(mm):
    ok = rng.random() >= st['fail_calloc']
    n, size = mm.gpr(3), mm.gpr(4)
    ev('calloc', n, size, int(ok))
    mm.gpr(3, alloc(n * size) if ok else 0)


def answer(name, p, *args):
    def stub(mm):
        r = bit(p)
        ev(name, *[a(mm) for a in args], r)
        mm.gpr(3, r)
    return stub


def record(name, *args):
    return lambda mm: ev(name, *[a(mm) for a in args])


r3w = lambda mm: who(mm.gpr(3))
g = lambda i: (lambda mm: s32(mm.gpr(i)))
fl = lambda i: (lambda mm: fb(f32(mm.fpr(i))))
v3 = lambda i: (lambda mm: vec(mm.gpr(i)))


def hit(name):
    def stub(mm):
        r = rng.choice([0, 0, HIT])
        ev(name, r)
        mm.gpr(3, r)
    return stub


def which_player(mm):
    r = rng.choice([-1, 0, 1, 1, 0])
    ev('which_player', mm.gpr(3), r)
    mm.gpr(3, r & 0xffffffff)


def engine_stubs():
    m.stub(0x800655F4, answer('range_out', 0.1))
    m.stub(0x8006544C, answer('broken', 0.3, r3w))
    m.stub(0x80065404, answer('flag20', 0.5, r3w))
    m.stub(0x80065414, record('set_flag20', r3w))
    m.stub(0x80065484, record('set_broken', r3w))
    m.stub(0x800653B0, record('dead_out', r3w))
    m.stub(0x80071AE0, record('free_task', r3w))
    m.stub(0x80071AF0, record('destroy_task'))
    m.stub(0x800066F4, hit('hit_player'))
    m.stub(0x80006574, hit('hit_6574'))
    m.stub(0x80037EDC, which_player)
    m.stub(0x8000F7D8, lambda mm: ev('init_share', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6)))
    m.stub(0x8000F8B8, lambda mm: ev('ccl_init', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6)))
    m.stub(0x8000F4EC, record('entry'))
    m.stub(0x80006908, record('calc_range'))
    m.stub(0x800067F8, record('f67f8'))
    m.stub(0x80018D28, record('f18d28'))
    m.stub(0x80018B88, record('f18b88'))
    m.stub(0x8006378C, record('score', g(3)))
    m.stub(0x800637EC, record('rings', g(3), g(4)))
    m.stub(0x80035CF0, record('mech_hp', g(3), fl(1)))
    m.stub(0x800641B8, record('f641b8', g(3), g(4)))
    for a, n in [(0x800373B8, 'f373b8'), (0x800374E0, 'f374e0'), (0x800375FC, 'f375fc'),
                 (0x80037718, 'f37718'), (0x800168C8, 'f168c8')]:
        m.stub(a, record(n, g(3)))
    m.stub(S1(0xCCFC), record('break', v3(3), v3(4), fl(1)))
    m.stub(0x8006B978, record('se', g(3), g(4), g(5), g(6)))
    m.stub(0x8002FB2C, record('f2fb2c', g(3), g(4), g(5), g(6)))
    m.stub(0x80071E0C, create_task)
    m.stub(0x801197FC, calloc)
    m.stub(0x80119828, record('free'))


def draw_stubs():
    m.stub(0x80116E18, lambda mm: ev('tex', mm.gpr(3) - S5(0)))
    m.stub(0x80116098, record('push'))
    # njPushMatrixEx is interpreted (paired singles) before its stub, so
    # njPopMatrixEx runs too, only watched, to keep the stack balanced.
    m.u.hook_add(UC_HOOK_CODE, lambda uc, pc, size, _: ev('pop'), begin=0x801160E4, end=0x801160E4)
    m.stub(0x8011616C, record('translate', v3(3)))
    m.stub(0x80115828, record('rot_y', g(4)))
    m.stub(0x801158A8, record('rot_z', g(4)))
    m.stub(0x80115178, lambda mm: ev('scale', [fb(f32(mm.fpr(k))) for k in (1, 2, 3)]))
    m.stub(0x8011C2BC, record('fog_dis'))
    m.stub(0x8011C2B0, record('fog_en'))
    m.stub(0x8011C310, record('set_fog'))
    m.stub(0x80121BDC, lambda mm: ev('gj', mm.gpr(3) - S5(0)))
    m.stub(0x8011DF20, lambda mm: ev('cnk', mm.gpr(3) - S5(0)))
    m.stub(S1(0x9C08), record('icon', g(3), v3(4), g(5), fl(1)))
    m.stub(0x800156FC, lambda mm: ev('f156fc', [fb(f32(mm.fpr(k))) for k in (1, 2, 3, 4)]))
    for a, n in [(0x8002B304, 'b304'), (0x8002B35C, 'b35c'), (0x8002B348, 'b348'),
                 (0x8002B2F8, 'b2f8'), (0x800148FC, 'f148fc'), (0x80014CD8, 'f14cd8'),
                 (0x8001475C, 'f1475c')]:
        m.stub(a, record(n))
    m.stub(0x8002B2D8, record('off3d', g(3)))
    m.stub(0x8002B2E8, record('on3d', g(3)))
    m.stub(0x801218C8, record('f218c8', g(3), g(4)))
    m.stub(S1(0x7684), record('exp', fl(1), fl(2)))
    m.stub(S1(0x9D08), record('hud', g(3), v3(4), fl(1)))


# --- state ------------------------------------------------------------------
def queues():
    res = []
    for k in range(2):
        t = m.get(SLOTS + 4 * k, 'I')
        if not t:
            res.append(None)
            continue
        tw, w = m.get(t + 0x34, 'I'), m.get(t + 0x38, 'I')
        n = s32(m.get(w + 4, 'I'))
        res.append({'mode': m.get(tw, 'b'), 'player': m.get(tw + 3, 'B'), 'count': n,
                    'timer': m.get(w + 8, 'I'),
                    'entries': [[s32(m.get(w + 0xC + 12 * i, 'I')), m.get(w + 0x10 + 12 * i, 'I'),
                                 m.get(w + 0x14 + 12 * i, 'I')] for i in range(max(0, min(n, 20)))]})
    return res


def explosions():
    t = m.get(EXPMAN, 'I')
    if not t:
        return None
    res, n = [], m.get(t + 0x3C, 'I')
    while n:
        res.append(vec(n) + [m.get(n + 0xC, 'I'), m.get(n + 0x10, 'I')])
        n = m.get(n + 0x14, 'I')
    return res


def world_reset():
    st['arena'] = ARENA
    m.put(SLOTS, 'I', 0)
    m.put(SLOTS + 4, 'I', 0)
    m.put(EXPMAN, 'I', 0)
    h = rng.choice([0, 0, 0, OTHER_HOOK])
    m.put(HOOK, 'I', h)
    return hook_class()


def globals_frame():
    gl = {'queue_on': rng.choice([0, 1, 1, 1, -3]), 'split': rng.choice([0, 0, 1]),
          'flag20': rng.choice([0, 0, 1]), 'freeze': rng.choice([0, 0, 0, 0, 1]),
          'turn': rng.randrange(-0x8000, 0x8000), 'fog': rng.choice([0, 1]),
          'screen': rng.choice([0, 1, 0, 2]), 'screens': rng.choice([1, 1, 2, 0, -1, 3]),
          'pflags': [rng.choice([0, 0, 0x4000, 0x7fff, -1, 0x1234]) for _ in range(2)]}
    m.put(GB + 0x0B, 'b', gl['queue_on'])
    m.put(GB + 0x1E, 'b', gl['split'])
    m.put(GB + 0x20, 'b', gl['flag20'])
    m.put(GB + 0x37, 'b', gl['freeze'])
    m.put(GB + 0x7C, 'i', gl['turn'])
    m.put(FOG, 'i', gl['fog'])
    m.put(SCREEN, 'i', gl['screen'])
    m.put(SCREENS, 'i', gl['screens'])
    for k in range(2):
        m.put(PLAYERPWP + 4 * k, 'I', PWP + 0x40 * k)
        m.put(PWP + 0x40 * k + 0x10, 'h', gl['pflags'][k])
    return gl


def rnd_scale_x():
    return rng.choice([float(rng.randrange(-2, 13)), f32(rng.uniform(-3.0, 14.0)), float('nan'),
                       float('inf'), -float('inf'), 1e10, -1e10, -0.5, 10.99, 11.0])


# --- the boxes ------------------------------------------------------------------
def box_task():
    for a, n in [(T, 0x60), (PARENT, 0x60), (TWP, 0x40), (MWP, 0x60), (CWP, 0x40), (INFO, 0x30)]:
        clear(a, n)
    m.put(T + 0x8, 'I', PARENT)
    m.put(T + 0x34, 'I', TWP)
    m.put(T + 0x38, 'I', MWP)
    m.put(TWP + 0x2C, 'I', CWP)
    m.put(CWP + 0xC, 'I', INFO)
    t = {'mode': rng.choice([0, 0, 0, 0, 1, 2, 2, 2, 3, 4, 5, 6, 7, -1, rng.randrange(-128, 128)]),
         'child': rng.choice([0, 0, 0, 1, -2]), 'air_item': rng.randrange(0, 11),
         'flags': rng.choice([0, 0, 4, 0x7fff, -4, 0x10]),
         'ang': [rng.randrange(-0x80000000, 0x80000000) for _ in range(3)],
         'pos': [rnd_f() for _ in range(3)], 'scale_x': rnd_scale_x(),
         'spin': rnd_f(), 'size': rng.choice([0.0, 0.5, 0.95, 1.0, 4.9, 4.85, rnd_f(0, 6)]),
         'item': rng.randrange(0, 11), 'has_set': bit(0.6), 'parent_has_set': bit(0.6),
         'work_player': rng.choice([-1, 0, 1])}
    m.put(TWP, 'b', t['mode'])
    m.put(TWP + 1, 'b', t['child'])
    m.put(TWP + 3, 'B', t['air_item'])
    m.put(TWP + 4, 'h', t['flags'])
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', t['ang'][k])
        m.put(TWP + 0x14 + 4 * k, 'f', t['pos'][k])
    m.put(TWP + 0x20, 'f', t['scale_x'])
    m.put(TWP + 0x24, 'f', t['spin'])
    m.put(TWP + 0x28, 'f', t['size'])
    m.put(T + 0x3C, 'i', t['item'])
    m.put(T + 0x30, 'I', 0x80F00000 if t['has_set'] else 0)
    m.put(PARENT + 0x30, 'I', 0x80F00100 if t['parent_has_set'] else 0)
    m.put(MWP + 0x44, 'b', t['work_player'])
    for k in ('pos',):
        t[k] = [fb(v) for v in t[k]]
    for k in ('scale_x', 'spin', 'size'):
        t[k] = fb(t[k])
    return t


def box_state():
    cb = lambda off: (lambda v: v - S1(0) if v else 0)(m.get(T + off, 'I'))
    return {'mode': m.get(TWP, 'b'), 'air_item': m.get(TWP + 3, 'B'), 'item': m.get(T + 0x3C, 'i'),
            'spin': m.get(TWP + 0x24, 'I'), 'size': m.get(TWP + 0x28, 'I'),
            'display': cb(0x14), 'dest': cb(0x18), 'display_icon': cb(0x20),
            'center': vec(INFO + 8), 'hook': hook_class(), 'queues': queues(),
            'explosions': explosions()}


def boxes(exec_):
    engine_stubs()
    for _ in range(400):
        hook0 = world_reset()
        t = box_task()
        frames = []
        for _ in range(8):
            gl = globals_frame()
            events.clear()
            m.call(S1(exec_), r3=T)
            frames.append({'globals': gl, 'events': list(events), 'state': box_state()})
        out['cases'].append({'task': t, 'hook': hook0, 'frames': frames})


# --- the HUD queue ------------------------------------------------------------
def queue():
    engine_stubs()
    draw_stubs()
    for _ in range(150):
        hook0 = world_reset()
        frames = []
        busy, quiet = rng.choice([0.03, 0.1, 0.5]), rng.choice([20, 40, 1000])
        for f in range(rng.choice([10, 30, 160, 220])):
            gl = globals_frame()
            events.clear()
            for _ in range(rng.choice([1, 1, 2]) if f < quiet and rng.random() < busy else 0):
                p, kind = rng.choice([-1, 0, 1, 1, 0, 2]), rng.randrange(0, 11)
                slot = m.get(SLOTS + 4 * p, 'I') if p in (0, 1) else 0
                if slot and s32(m.get(m.get(slot + 0x38, 'I') + 4, 'I')) >= 20:
                    continue
                ev('callback', p, kind)
                m.call(S1(0x9FB8), r3=p & 0xffffffff, r4=kind)
            for k in range(2):
                t = m.get(SLOTS + 4 * k, 'I')
                if not t:
                    continue
                ev('exec', k)
                n = len(events)
                m.call(S1(0xA0CC), r3=t)
                if ['destroy_task'] in events[n:]:
                    ev('dest', k)
                    m.call(S1(0x9F48), r3=t)
                    continue
                ev('display', k)
                m.call(S1(0xA40C), r3=t)
            frames.append({'globals': gl, 'events': list(events), 'state': {'queues': queues()}})
        out['cases'].append({'hook': hook0, 'frames': frames})


# --- ExpMan -------------------------------------------------------------------
def explosion():
    engine_stubs()
    draw_stubs()
    st['fail_create'], st['fail_calloc'] = 0.1, 0.1
    pt = SCRATCH + 0xA00
    for _ in range(150):
        hook0 = world_reset()
        frames = []
        for _ in range(rng.choice([8, 30, 60])):
            gl = globals_frame()
            events.clear()
            for _ in range(rng.choice([0, 0, 0, 1, 2])):
                p = [rnd_f() for _ in range(3)]
                for k in range(3):
                    m.put(pt + 4 * k, 'f', p[k])
                ev('spawn', vec(pt))
                m.call(S1(0x12C18), r3=pt)
            g_ = m.get(EXPMAN, 'I')
            if g_:
                ev('exec')
                n = len(events)
                m.call(S1(0x12D50), r3=g_)
                if ['destroy_task'] in events[n:] or rng.random() < 0.02:
                    ev('dest')
                    m.call(S1(0x12EB0), r3=g_)
                else:
                    ev('display')
                    m.call(S1(0x12E20), r3=g_)
            if hook_class() == 1 or rng.random() < 0.3:
                for _ in range(3):
                    sp = explosions()
                    if sp and rng.random() < 0.6:
                        c = rng.choice(sp)
                        p = [f32(bf(c[k]) + rng.uniform(-1, 1) * bf(c[3]) * 0.8) for k in range(3)]
                    else:
                        p = [rnd_f() for _ in range(3)]
                    for k in range(3):
                        m.put(pt + 4 * k, 'f', p[k])
                    m.call(S1(0x12F44), r3=pt)
                    ev('hit_test', vec(pt), m.gpr(3))
            frames.append({'globals': gl, 'events': list(events),
                           'state': {'explosions': explosions(), 'hook': hook_class()}})
        out['cases'].append({'hook': hook0, 'frames': frames})


# --- the box displays ---------------------------------------------------------
def display():
    draw_stubs()
    for _ in range(600):
        t = box_task()
        if rng.random() < 0.3:
            m.put(TWP, 'b', 4)
            t['mode'] = 4
        if rng.random() < 0.4:
            s = rng.choice([1.0, 1.2, 4.9, 5.0, 3.0, f32(rng.uniform(0.9, 5.1)), float('nan'), 1e30])
            m.put(TWP + 0x28, 'f', s)
            t['size'] = fb(f32(s))
        gl = globals_frame()
        calls = []
        for off in (0x12690, 0x12750, 0x1289C, 0x13640, 0x13710):
            events.clear()
            m.call(S1(off), r3=T)
            calls.append(list(events))
        out['cases'].append({'task': t, 'globals': gl, 'calls': calls})


{'ground': lambda: boxes(0x12028), 'air': lambda: boxes(0x12FD0), 'queue': queue,
 'explosion': explosion, 'display': display}[which]()
print(json.dumps(out))
