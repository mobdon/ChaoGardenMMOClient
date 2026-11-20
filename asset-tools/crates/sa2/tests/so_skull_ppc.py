"""Run stg42D.rel's SKULL, PSKULL, UDREEL and STOPLSD code under the main.dol
harness and print JSON (floats as bit patterns). tests/so_skull.rs replays
each case with `stage_objects::skull` / `stage_objects::udreel_stoplsd`.
Usage: python so_skull_ppc.py <skull|pskull|skull_display|stoplsd|udreel_load|udreel|udreel_display>
Engine routines (collision, SET state, player calls, sound, allocation,
drawing) and the stage module's movable-object helpers 0x4B44 / 0x4BA8 /
0x4BE0 are stubbed and recorded as events; rand, the player range test,
fabsf and the Ninja matrix routines run as compiled.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

T = SCRATCH
TWP, CWP, INFO, MWP, LAND = T + 0x100, T + 0x200, T + 0x400, T + 0x500, T + 0x600
HIT, HTASK = T + 0x700, T + 0x800
PT = [T + 0x900, T + 0xA00]     # player twp
PC = [T + 0xB00, T + 0xC00]     # player cwp
PI = [T + 0xD00, T + 0xE00]     # player info
PP = [T + 0xF00, T + 0x1100]    # player pwp
ALT = [T + 0x1300, T + 0x1400]  # stand-in models for display 0x20114
PLAYERTWP, PLAYERPWP = 0x801E7768, 0x801E7728
GLOBAL = 0x801CC168
NEXT = 0x803AD6A0              # MSL rand state
frame = {}
which = sys.argv[1]
out = {'object': which, 's1': S1(0), 's5': S5(0), 'cases': []}
sint = lambda v: v - (1 << 32) if v & 0x80000000 else v


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def fv(*regs):
    return [fb(f32(m.fpr(r))) for r in regs]


def put_v(a, v):
    for k, x in enumerate(v):
        m.put(a + 4 * k, 'f', x)


def task_reset():
    clear(T, 0x2000)
    m.put(T + 0x34, 'I', TWP)
    m.put(T + 0x38, 'I', MWP)
    m.put(TWP + 0x2C, 'I', CWP)
    m.put(CWP + 0xC, 'I', INFO)
    m.put(CWP + 0xA4, 'I', HIT)
    m.put(HIT + 0x9C, 'I', HTASK)


def rnd_twp():
    t = {'mode': rng.randrange(-128, 128), 'id': rng.choice([0, 1, 2, 3, 4, -1, 5, rng.randrange(-128, 128)]),
         'b2': rng.randrange(256), 'b3': rng.choice([0, 0x3C, 0x3D, rng.randrange(256)]),
         'flag': rng.randrange(-0x8000, 0x8000), 'w6': rng.choice([0, 1, 2, 0xB3F, 0xB40, rng.randrange(0x10000)]),
         'ang': [rng.choice([0, rng.randrange(-1 << 31, 1 << 31), rng.randrange(-0x10000, 0x10000)]) for _ in range(3)],
         'pos': [fb(rnd_f()) for _ in range(3)], 'scale': [fb(rnd_f(-3, 3)) for _ in range(3)]}
    return t


def put_twp(t):
    m.put(TWP, 'b', t['mode']); m.put(TWP + 1, 'b', t['id']); m.put(TWP + 2, 'B', t['b2'])
    m.put(TWP + 3, 'B', t['b3']); m.put(TWP + 4, 'h', t['flag']); m.put(TWP + 6, 'H', t['w6'])
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', t['ang'][k])
        m.put(TWP + 0x14 + 4 * k, 'I', t['pos'][k])
        m.put(TWP + 0x20 + 4 * k, 'I', t['scale'][k])


def get_twp():
    return {'mode': m.get(TWP, 'b'), 'id': m.get(TWP + 1, 'b'), 'b2': m.get(TWP + 2, 'B'),
            'b3': m.get(TWP + 3, 'B'), 'flag': m.get(TWP + 4, 'h'), 'w6': m.get(TWP + 6, 'H'),
            'ang': [m.get(TWP + 8 + 4 * k, 'i') for k in range(3)], 'pos': vec(TWP + 0x14),
            'scale': vec(TWP + 0x20)}


def callbacks():
    def off(v):
        return v - S1(0) if v else 0
    return [off(m.get(T + o, 'I')) for o in (0x10, 0x14, 0x18, 0x1C)]


def common_stubs():
    m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), frame['out'])[1]))
    m.stub(0x8000F8B8, lambda mm: (ev('init', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6)), mm.gpr(3, 1)))
    m.stub(0x8000F4EC, lambda mm: (ev('entry'), assert_(mm.gpr(3) == T)))


def assert_(c):
    assert c


# --- SKULL / PSKULL ---------------------------------------------------------
def skull_players(present):
    ps = []
    for i in range(2):
        if not present[i]:
            m.put(PLAYERTWP + 4 * i, 'I', 0); m.put(PLAYERPWP + 4 * i, 'I', 0)
            ps.append(None)
            continue
        clear(PT[i], 0x100); clear(PP[i], 0x200); clear(PC[i], 0x100); clear(PI[i], 0x40)
        m.put(PLAYERTWP + 4 * i, 'I', PT[i]); m.put(PLAYERPWP + 4 * i, 'I', PP[i])
        rides = rng.random() < 0.4
        m.put(PP[i] + 0x94, 'I', T if rides else HTASK)
        ang_y = rng.randrange(-1 << 31, 1 << 31)
        m.put(PT[i] + 0xC, 'i', ang_y)
        center = None
        if rng.random() < 0.85:
            m.put(PT[i] + 0x2C, 'I', PC[i]); m.put(PC[i] + 0xC, 'I', PI[i])
            center = [fb(rnd_f(-200, 200)) for _ in range(3)]
            for k in range(3):
                m.put(PI[i] + 8 + 4 * k, 'I', center[k])
        ps.append({'rides': rides, 'ang_y': ang_y, 'center': center})
    return ps


def skull_stubs():
    common_stubs()
    m.stub(S1(0x4B44), lambda mm: (ev('movable_create'), mm.put(T + 0x3C, 'I', frame['land'])))
    m.stub(S1(0x4BA8), lambda mm: (ev('movable_release'), mm.put(T + 0x3C, 'I', 0)))
    m.stub(S1(0x4BE0), lambda mm: ev('movable_update'))
    m.stub(0x80065388, ret(lambda mm: (ev('cond_get'), frame['cond'])[1]))
    m.stub(0x8006539C, lambda mm: ev('cond_set', mm.gpr(4)))
    m.stub(0x800260FC, lambda mm: ev('drop', mm.gpr(3), fv(1, 2, 3)))
    m.stub(0x800653B0, lambda mm: ev('dead_out'))
    m.stub(0x80071AE0, lambda mm: ev('free_task'))


def skull_state():
    land = m.get(T + 0x3C, 'I')
    return {'twp': get_twp(), 'cb': callbacks(), 'alpha': m.get(T + 0x40, 'I'),
            'attr': [m.get(INFO + 0x30 * k + 4, 'I') for k in range(3)], 'speed': vec(MWP),
            'mwp30': m.get(MWP + 0x30, 'I'),
            'land': [m.get(LAND + 0x60, 'I'), m.get(LAND + 0x84, 'I')] if land else None}


def skull_case(kind):
    task_reset()
    t = rnd_twp()
    if kind == 'pskull_exec':
        t['id'] = rng.choice([0, 1, 1, 2, 2, 3, -1, rng.randrange(-128, 128)])
        t['flag'] = rng.choice([0, -0x8000, rng.randrange(-0x8000, 0x8000)])
        t['w6'] = rng.choice([0, 1, 2, 5, 0xB3F, 0xB40, 0xB40, 0xFFFF])
        t['scale'] = [fb(rnd_f(-3, 3)) for _ in range(3)]
    put_twp(t)
    has_set = rng.random() < 0.6
    m.put(T + 0x30, 'I', HTASK if has_set else 0)
    alpha = rng.choice([1.0, 0.5, 0.01, 0.0101, 0.02, 0.0, f32(rng.uniform(-0.1, 1.5))])
    m.put(T + 0x40, 'f', alpha)
    attr = [rng.randrange(1 << 32) for _ in range(3)]
    for k in range(3):
        m.put(INFO + 0x30 * k + 4, 'I', attr[k])
    speed = [fb(rnd_f(-5, 5)) for _ in range(3)]
    for k in range(3):
        m.put(MWP + 4 * k, 'I', speed[k])
    land0 = [rng.randrange(1 << 32), fb(rnd_f())]
    m.put(LAND + 0x60, 'I', land0[0]); m.put(LAND + 0x84, 'I', land0[1])
    has_land = rng.random() < 0.7
    frame['land'] = LAND if has_land else 0
    if kind == 'pskull_exec':
        m.put(T + 0x3C, 'I', frame['land'])
    seed = rng.randrange(1 << 32)
    m.put(NEXT, 'I', seed)
    present = [rng.random() < 0.8, rng.random() < 0.8]
    players = skull_players(present)
    case = {'kind': kind, 'twp': t, 'has_set': has_set, 'alpha': fb(f32(alpha)), 'attr': attr, 'speed': speed,
            'land0': land0, 'has_land': has_land, 'seed': seed, 'players': players, 'frames': []}
    if kind == 'pskull_exec':
        case['mwp30'] = 0
    nframes = 6 if kind != 'skull_load' and kind != 'pskull_load' else 1
    for _ in range(nframes):
        frame['out'] = rng.random() < 0.1
        frame['cond'] = rng.choice([0, 0, 1, -1])
        cwp_flag = rng.choice([0, 1, rng.randrange(0x10000)])
        m.put(CWP + 4, 'H', cwp_flag)
        events.clear()
        fn = {'skull_load': 0x1DB78, 'skull_exec': 0x1DB14, 'pskull_load': 0x1D500, 'pskull_exec': 0x1D62C,
              'destructor': 0x1D598}[kind]
        m.call(S1(fn), r3=T)
        case['frames'].append({'out': frame['out'], 'cond': frame['cond'], 'cwp_flag': cwp_flag,
                               'events': list(events), 'state': skull_state()})
    out['cases'].append(case)


def skull():
    skull_stubs()
    for _ in range(150):
        skull_case('skull_load')
    for _ in range(150):
        skull_case('skull_exec')
    for _ in range(60):
        skull_case('destructor')


def pskull():
    skull_stubs()
    for _ in range(200):
        skull_case('pskull_load')
    for _ in range(1200):
        skull_case('pskull_exec')
    out['rows'] = {'skull': 0x1EDA0, 'pskull': 0x1ED10}


# --- displays ---------------------------------------------------------------
def draw_stubs():
    m.stub(0x80116E18, lambda mm: ev('tex', mm.gpr(3)))
    m.stub(0x80116098, lambda mm: ev('push'))
    m.stub(0x801160E4, lambda mm: ev('pop'))
    m.stub(0x8011616C, lambda mm: ev('translate_ex', vec(mm.gpr(3))))
    m.stub(0x80115BD8, lambda mm: (assert_(mm.gpr(3) == 0), ev('translate', fv(1, 2, 3))))
    m.stub(0x80115828, lambda mm: (assert_(mm.gpr(3) == 0), ev('rot_y', sint(mm.gpr(4)))))
    m.stub(0x80115178, lambda mm: (assert_(mm.gpr(3) == 0), ev('scale', fv(1, 2, 3))))
    for r in (0x8011E158, 0x80121BDC, 0x8011F514):
        m.stub(r, (lambda r: lambda mm: ev('model', r, mm.gpr(3), m.get(mm.gpr(3) + 0x20, 'I')))(r))
    m.stub(0x8002B2E8, lambda mm: ev('on3d', mm.gpr(3)))
    m.stub(0x8002B2D8, lambda mm: ev('off3d', mm.gpr(3)))
    for r in (0x8002B304, 0x8002B35C, 0x8002B348, 0x8002B2F8, 0x8011C2BC, 0x8011C2B0, 0x8011C310):
        m.stub(r, (lambda r: lambda mm: ev('call', r))(r))
    m.stub(0x801218C8, lambda mm: ev('blend', mm.gpr(3), mm.gpr(4)))
    m.stub(0x800156FC, lambda mm: ev('color', fv(1, 2, 3, 4)))


def skull_display():
    draw_stubs()
    for _ in range(200):
        task_reset()
        t = rnd_twp()
        put_twp(t)
        alpha = rng.choice([1.0, 1.5, 0.99999994, 0.5, 0.0, f32(rng.uniform(-1, 2))])
        m.put(T + 0x40, 'f', alpha)
        events.clear()
        m.call(S1(0x1DA40), r3=T)
        out['cases'].append({'twp': t, 'alpha': fb(f32(alpha)), 'events': list(events)})
    out['texlist'], out['model'] = S5(0x1E3DC), S5(0x1ECF0)
    out['f20'] = m.get(S5(0x1ECF0) + 0x20, 'I')


# --- STOPLSD ----------------------------------------------------------------
def stoplsd():
    common_stubs()
    m.stub(0x800068E4, lambda mm: (assert_(mm.gpr(3) == CWP),
                                   ev('calc', [m.get(INFO + 0x14 + 4 * k, 'I') for k in range(3)],
                                      m.get(INFO + 0x2C, 'I'))))
    for _ in range(300):
        task_reset()
        t = rnd_twp()
        put_twp(t)
        w2c = rng.randrange(1 << 32)
        m.put(INFO + 0x2C, 'I', w2c)
        frames = []
        for fn in (0xCB8C, 0xCCBC, 0xCCBC, 0xCCB4, 0xCCB8):
            frame['out'] = rng.random() < 0.2
            events.clear()
            m.call(S1(fn), r3=T)
            frames.append({'fn': fn, 'out': frame['out'], 'events': list(events), 'twp': get_twp(),
                           'cb': callbacks()})
        out['cases'].append({'twp': t, 'w2c': w2c, 'frames': frames})
    out['rows'] = 0xDEA8


# --- UDREEL -----------------------------------------------------------------
MODEL = None


def reel_players():
    ps = []
    for i in range(2):
        clear(PT[i], 0x100); clear(PP[i], 0x200); clear(PC[i], 0x100); clear(PI[i], 0x40)
        m.put(PLAYERTWP + 4 * i, 'I', PT[i]); m.put(PLAYERPWP + 4 * i, 'I', PP[i])
        p = {'mode': rng.choice([0x33, 0, 5, rng.randrange(-128, 128)]),
             'ang': [rng.randrange(-1 << 31, 1 << 31) if rng.random() < 0.5 else rng.randrange(-0x10000, 0x10000)
                     for _ in range(3)],
             'pos': [fb(rnd_f(-100, 100)) for _ in range(3)],
             'character': rng.choice(list(range(-1, 18)) + [rng.randrange(-128, 128)]),
             'center': None}
        m.put(PT[i], 'b', p['mode'])
        for k in range(3):
            m.put(PT[i] + 8 + 4 * k, 'i', p['ang'][k]); m.put(PT[i] + 0x14 + 4 * k, 'I', p['pos'][k])
        m.put(PP[i] + 3, 'b', p['character'])
        if rng.random() < 0.85:
            m.put(PT[i] + 0x2C, 'I', PC[i]); m.put(PC[i] + 0xC, 'I', PI[i])
            p['center'] = [fb(rnd_f(-60, 60)) for _ in range(3)]
            for k in range(3):
                m.put(PI[i] + 8 + 4 * k, 'I', p['center'][k])
        ps.append(p)
    return ps


def get_players():
    return [{'mode': m.get(PT[i], 'b'), 'ang': [m.get(PT[i] + 8 + 4 * k, 'i') for k in range(3)],
             'pos': vec(PT[i] + 0x14)} for i in range(2)]


def reel_state():
    w = m.get(T + 0x38, 'I')
    work = None
    if w:
        assert w == MWP
        work = {'obj': [m.get(MWP + 4 * k, 'I') for k in range(14)],
                'f': [m.get(MWP + 0x38 + 4 * k, 'I') for k in range(5)]}
    return {'twp': get_twp(), 'cb': callbacks(), 'work': work, 'counter': m.get(T + 0x4C, 'i'),
            'attr': m.get(INFO + 4, 'I'), 'info_c': m.get(INFO + 0xC, 'I'), 'players': get_players()}


def reel_stubs():
    common_stubs()
    m.stub(0x801197FC, ret(lambda mm: (ev('calloc', mm.gpr(3), mm.gpr(4)), clear(MWP, 0x4C),
                                       MWP if frame['calloc'] else 0)[2]))
    m.stub(0x800232A4, lambda mm: (assert_(mm.gpr(4) == T and mm.gpr(5) == MWP), ev('dyn_land', mm.gpr(3))))
    m.stub(0x800231CC, lambda mm: (assert_(mm.gpr(3) == T), ev('dyn_remove')))
    m.stub(0x80119828, lambda mm: (assert_(mm.gpr(3) == MWP), ev('free')))
    m.stub(0x80067AF0, ret_f(lambda mm: (ev('shadow', fv(1, 2, 3)), frame['shadow'])[1]))
    m.stub(0x8006AFFC, lambda mm: (assert_(mm.gpr(4) == TWP),
                                   ev('sound', mm.gpr(3), mm.gpr(5), mm.gpr(6), mm.gpr(7), vec(mm.gpr(8)))))
    m.stub(0x8003A3DC, lambda mm: ev('velocity', sint(mm.gpr(3)), fv(1, 2, 3)))
    m.stub(0x8002FB2C, lambda mm: ev('action', sint(mm.gpr(3)), mm.gpr(4), mm.gpr(5), mm.gpr(6)))
    m.stub(0x80037EDC, ret(lambda mm: (assert_(mm.gpr(3) == HTASK), ev('is_player'), frame['slot'])[2]))
    m.stub(0x8003A508, lambda mm: ev('input', sint(mm.gpr(3)), mm.gpr(4), mm.gpr(5)))
    m.stub(0x800399BC, lambda mm: ev('set_pos', sint(mm.gpr(3)), fv(1, 2, 3)))
    m.stub(0x800068E4, lambda mm: (assert_(mm.gpr(3) == CWP), ev('calc', m.get(INFO + 0xC, 'I'))))


def reel_data():
    global MODEL
    MODEL = m.get(S5(0x20B30) + 4, 'I')
    out['template'] = [m.get(S5(0x20B30) + 4 * k, 'I') for k in range(14)]
    out['radius'] = m.get(MODEL + 0x24, 'I')
    out['row'] = 0x20BD0


def udreel_load():
    reel_stubs()
    reel_data()
    for _ in range(300):
        task_reset()
        t = rnd_twp()
        t['scale'] = [fb(rnd_f(-3, 3)) for _ in range(3)]
        put_twp(t)
        m.put(T + 0x38, 'I', 0)
        alt = rng.random() < 0.3
        m.put(S6(0x6C8), 'I', rng.randrange(1, 1 << 32) if alt else 0)
        frames = []
        for _ in range(2):
            frame['out'] = rng.random() < 0.15
            frame['calloc'] = rng.random() < 0.8
            events.clear()
            m.call(S1(0x1EDC8), r3=T)
            frames.append({'out': frame['out'], 'calloc': frame['calloc'], 'events': list(events),
                           'state': reel_state()})
            if m.get(T + 0x38, 'I'):
                break
        out['cases'].append({'twp': t, 'alt': alt, 'frames': frames})
    # The destructor, with and without a model.
    for _ in range(40):
        task_reset()
        has_model = rng.random() < 0.6
        m.put(MWP + 4, 'I', MODEL if has_model else 0)
        events.clear()
        m.call(S1(0x1F094), r3=T)
        out.setdefault('dest', []).append({'model': has_model, 'events': list(events),
                                           'work': m.get(T + 0x38, 'I')})


def udreel():
    reel_stubs()
    reel_data()
    for _ in range(1500):
        task_reset()
        t = rnd_twp()
        t['mode'] = rng.choice([0, 1, 2, 3, 4, 5, 8, 9, 0xD, 0xF, rng.randrange(-128, 128)])
        t['id'] = rng.choice([0, 1, 2, 2, 3, 4, 4, -1, 5])
        t['w6'] = rng.choice([0, 1])
        t['scale'] = [fb(rnd_f(-3, 3)) for _ in range(3)]
        put_twp(t)
        obj = list(out['template'])
        obj[1] = MODEL if rng.random() < 0.85 else 0
        for k in range(2, 5):
            obj[k] = fb(rnd_f(-60, 60))
        obj[6] = t['ang'][1] & 0xffffffff
        for k in range(14):
            m.put(MWP + 4 * k, 'I', obj[k])
        f = [fb(rng.choice([rnd_f(-80, 10), -7.0, -5.0, -6.9, 0.0])), fb(rnd_f(-4, 4)), fb(rnd_f()), fb(rnd_f()),
             fb(rng.choice([-1000000.0, -1000000.0, rnd_f()]))]
        for k in range(5):
            m.put(MWP + 0x38 + 4 * k, 'I', f[k])
        counter = rng.choice([0, 15, 16, 30, rng.randrange(-5, 40)])
        m.put(T + 0x4C, 'i', counter)
        attr = rng.randrange(1 << 32)
        info_c = fb(rnd_f())
        m.put(INFO + 4, 'I', attr); m.put(INFO + 0xC, 'I', info_c)
        m.put(T + 0x38, 'I', MWP)
        players = reel_players()
        case = {'twp': t, 'work': {'obj': obj, 'f': f}, 'counter': counter, 'attr': attr, 'info_c': info_c,
                'players': players, 'frames': []}
        for _ in range(8):
            frame['out'] = rng.random() < 0.1
            frame['shadow'] = rng.choice([-1000000.0, f32(rng.uniform(-100, 100))])
            frame['slot'] = rng.choice([-1, 0, 1, 0, 1])
            g42 = rng.choice([0, 0, 0, 1, 2])
            m.put(GLOBAL + 0x42, 'b', g42)
            cwp_flag = rng.choice([0, 1, 1, rng.randrange(0x10000)])
            m.put(CWP + 4, 'H', cwp_flag)
            hit_id = rng.choice([0, 0, 0, 1, rng.randrange(0x10000)])
            m.put(HIT, 'H', hit_id)
            events.clear()
            m.call(S1(0x1F0E4), r3=T)
            case['frames'].append({'out': frame['out'], 'shadow': fb(f32(frame['shadow'])), 'slot': frame['slot'],
                                   'g42': g42, 'cwp_flag': cwp_flag, 'hit_id': hit_id, 'events': list(events),
                                   'state': reel_state()})
        out['cases'].append(case)


def udreel_display():
    draw_stubs()
    reel_data()
    root = S5(0x209B4)
    rope_obj = m.get(root + 0x2C, 'I')
    hook_obj = m.get(rope_obj + 0x2C, 'I')
    models = {'texlist': S5(0x1F008), 'base': m.get(root + 4, 'I'), 'rope': m.get(rope_obj + 4, 'I'),
              'hook': m.get(hook_obj + 4, 'I'), 'shadow_texlist': S5(0x1AB94), 'shadow_model': S5(0x1ACD0),
              'alt': ALT}
    m.put(S5(0x20BA0) + 0xC, 'I', ALT[0]); m.put(S5(0x20BA0) + 0x2C, 'I', ALT[1])
    models['rope_f20'] = m.get(models['rope'] + 0x20, 'I')
    f20 = {}
    for a in (models['base'], models['rope'], models['hook'], models['shadow_model'], ALT[0], ALT[1]):
        f20[str(a)] = m.get(a + 0x20, 'I')
    out['models'], out['f20'] = models, f20
    for fn in (0x1FF48, 0x20088, 0x20114):
        for _ in range(100):
            task_reset()
            t = rnd_twp()
            put_twp(t)
            wf = [fb(rnd_f()) for _ in range(5)]
            for k in range(5):
                m.put(MWP + 0x38 + 4 * k, 'I', wf[k])
            fog = rng.random() < 0.5
            m.put(S6(0x630), 'I', rng.randrange(1, 1 << 32) if fog else 0)
            fr = rng.randrange(1 << 32)
            m.put(GLOBAL + 0x7C, 'I', fr)
            events.clear()
            m.call(S1(fn), r3=T)
            assert m.get(models['rope'] + 0x20, 'I') == models['rope_f20']
            out['cases'].append({'fn': fn, 'twp': t, 'f': wf, 'fog': fog, 'frame': fr, 'events': list(events)})


{'skull': skull, 'pskull': pskull, 'skull_display': skull_display, 'stoplsd': stoplsd,
 'udreel_load': udreel_load, 'udreel': udreel, 'udreel_display': udreel_display}[which]()
print(json.dumps(out))
