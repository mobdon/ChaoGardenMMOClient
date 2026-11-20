"""Run stg42D.rel's BIG THE CAT, LIGHT SW and MODMOD under the main.dol
harness and print JSON (every float as its bit pattern). tests/so_bigcat.rs
replays each case with `stage_objects::{big_the_cat, light_modmod}` and
compares. Usage: python so_bigcat_ppc.py big|light_inside|light_sw|modmod

CheckRangeOut, syCalloc and (for MODMOD's display) the rendering calls are
stubbed and recorded as events; everything else runs as compiled: the REL's
code, its own sqrtf, and main.dol's njScalor, njDistanceP2P, the Ninja
matrix calls of the player test and fn_80025028 (which writes the light
bytes at 0x803AD924, read back after each frame).
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

T, TWP, WORK = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200
PT = [SCRATCH + 0x400, SCRATCH + 0x500]
CWP = [SCRATCH + 0x600, SCRATCH + 0x700]
INFO = [SCRATCH + 0x800, SCRATCH + 0x900]
PLAYERTWP, LIGHTS = 0x801E7768, 0x803AD924
frame = {}
which = sys.argv[1]
out = {'object': which, 'cases': []}


def s1off(a):
    return a - S1(0) if a else 0


def range_stub():
    m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), frame['out'])[1]))


def rnd_ang():
    return rng.choice([0, rng.randrange(0, 0x10000), rng.randrange(-0x80000000, 0x80000000),
                       0x4000, -0x4000, 0x8000, 0x2000])


def put_twp(ang, pos, scl):
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', ang[k])
        m.put(TWP + 0x14 + 4 * k, 'f', pos[k])
        m.put(TWP + 0x20 + 4 * k, 'f', scl[k])


def twp_state():
    return {'mode': m.get(TWP + 1, 'b'), 'light': m.get(TWP + 3, 'B'), 'shape': m.get(TWP + 6, 'H'),
            'ang': [m.get(TWP + 8 + 4 * k, 'i') for k in range(3)]}


def rnd_size():
    r = rng.random()
    if r < 0.05:
        return rng.choice([0.0, -0.0, -5.0, 1e30])
    return f32(rng.uniform(1.0, 60.0))


def rnd_player(pos, scl):
    """A player collision near the switch: (null level or None, centre, a, b)."""
    if rng.random() < 0.12:
        return {'null': rng.randrange(3)}
    reach = max(abs(s) for s in scl if s == s and abs(s) < 1e6) if any(
        s == s and abs(s) < 1e6 for s in scl) else 10.0
    span = rng.choice([0.6, 1.0, 1.6]) * reach + 5.0
    c = [f32(pos[k] + rng.uniform(-span, span)) for k in range(3)]
    if rng.random() < 0.02:
        c[rng.randrange(3)] = float('nan')
    a = f32(rng.uniform(0.0, 12.0)) if rng.random() > 0.05 else 0.0
    b = f32(rng.uniform(0.0, 12.0)) if rng.random() > 0.05 else 0.0
    return {'center': [fb(v) for v in c], 'a': fb(a), 'b': fb(b)}


def put_players(players):
    clear(PLAYERTWP, 0x20)
    for p, pl in enumerate(players):
        clear(PT[p], 0x40); clear(CWP[p], 0x20); clear(INFO[p], 0x30)
        null = pl.get('null', -1)
        if null == 0:
            continue
        m.put(PLAYERTWP + 4 * p, 'I', PT[p])
        if null == 1:
            continue
        m.put(PT[p] + 0x2C, 'I', CWP[p])
        if null == 2:
            continue
        m.put(CWP[p] + 0xC, 'I', INFO[p])
        for k in range(3):
            m.put(INFO[p] + 8 + 4 * k, 'I', pl['center'][k])
        m.put(INFO[p] + 0x14, 'I', pl['a'])
        m.put(INFO[p] + 0x18, 'I', pl['b'])


# --- BIG THE CAT ---------------------------------------------------------------
def big():
    range_stub()
    for _ in range(20):
        frames = []
        for _ in range(4):
            frame['out'] = rng.random() < 0.3
            events.clear()
            clear(T, 0x40)
            m.call(S1(0x1B7FC), r3=T)
            frames.append({'out': frame['out'], 'events': list(events),
                           'task': [m.get(T + o, 'I') for o in (0x10, 0x14, 0x18)]})
        out['cases'].append({'frames': frames})


# --- LIGHT SW player test (0x13D7C) ------------------------------------------
def light_inside():
    for _ in range(4000):
        clear(TWP, 0x40)
        ang = [rnd_ang() for _ in range(3)]
        pos = [rnd_f() for _ in range(3)]
        scl = [rnd_size() for _ in range(3)]
        shape = rng.choice([0, 0, 0, 1, 1, 2, 2, 2, 3, 0xFFFF])
        put_twp(ang, pos, scl)
        m.put(TWP + 6, 'H', shape)
        players = [rnd_player(pos, scl), rnd_player(pos, scl)]
        put_players(players)
        p = rng.randrange(2)
        m.call(S1(0x13D7C), r3=TWP, r4=p)
        out['cases'].append({'ang': ang, 'pos': [fb(v) for v in pos], 'scl': [fb(v) for v in scl],
                             'shape': shape, 'players': players, 'p': p, 'inside': m.gpr(3)})


# --- LIGHT SW set-up and exec ------------------------------------------------
def light_sw():
    range_stub()

    def calloc(mm):
        assert (mm.gpr(3), mm.gpr(4)) == (1, 4)
        ok = not frame['calloc_fail']
        ev('calloc')
        if ok:
            clear(WORK, 4)
        mm.gpr(3, WORK if ok else 0)
    m.stub(0x801197FC, calloc)
    for _ in range(400):
        clear(T, 0x80); clear(TWP, 0x40)
        clear(WORK, 4)
        for k in range(4):
            m.put(WORK + k, 'B', rng.randrange(256))
        ang = [rnd_ang() for _ in range(3)]
        ang[1] = (ang[1] & ~0xFF) | rng.choice([0, 1, 2, 3, 0x7F, 0x80, 0xFF, rng.randrange(256)])
        ang[0] = (ang[0] & ~0xFF) | rng.randrange(256)
        pos = [rnd_f() for _ in range(3)]
        scl = [rnd_size() for _ in range(3)]
        put_twp(ang, pos, scl)
        m.put(T + 0x34, 'I', TWP)
        lights = [rng.randrange(256) for _ in range(4)]
        for k in range(4):
            m.put(LIGHTS + k, 'B', lights[k])
        force = rng.choice([None] * 6 + [3, -1, 127, -128, 1, 2])
        players = [rnd_player(pos, scl), rnd_player(pos, scl)]
        frames = []
        for _ in range(10):
            frame['out'] = rng.random() < 0.1
            frame['calloc_fail'] = rng.random() < 0.25
            for p in range(2):
                if rng.random() < 0.5:
                    players[p] = rnd_player(pos, scl)
            put_players(players)
            events.clear()
            setup = m.get(T + 0x10, 'I') == 0
            m.call(S1(0x13C78) if setup else S1(0x14030), r3=T)
            forced = False
            if setup and force is not None and m.get(T + 0x10, 'I') != 0:
                m.put(TWP + 1, 'b', force)
                forced = True
            st = twp_state()
            st.update({'out': frame['out'], 'calloc_fail': frame['calloc_fail'],
                       'players': [dict(pl) for pl in players], 'events': list(events),
                       'forced': forced,
                       'callbacks': [s1off(m.get(T + o, 'I')) for o in (0x10, 0x14, 0x18)],
                       'work_ptr': m.get(T + 0x40, 'I') == WORK,
                       'work': [m.get(WORK + k, 'B') for k in range(4)],
                       'lights': [m.get(LIGHTS + k, 'B') for k in range(4)]})
            frames.append(st)
        out['cases'].append({'ang': ang, 'pos': [fb(v) for v in pos], 'scl': [fb(v) for v in scl],
                             'lights': lights, 'force': force, 'frames': frames})


# --- MODMOD -----------------------------------------------------------------
def modmod():
    range_stub()
    m.stub(0x80115D60, lambda mm: ev('push', mm.gpr(3)))
    m.stub(0x80115BD8, lambda mm: ev('translate', mm.gpr(3), [fb(f32(mm.fpr(k))) for k in (1, 2, 3)]))
    m.stub(0x80115C60, lambda mm: ev('rot_x', mm.gpr(3), mm.gpr(4)))
    m.stub(0x80115828, lambda mm: ev('rot_y', mm.gpr(3), mm.gpr(4)))
    m.stub(0x80115178, lambda mm: ev('scale', mm.gpr(3), [fb(f32(mm.fpr(k))) for k in (1, 2, 3)]))
    m.stub(0x8011E23C, ret(lambda mm: (ev('model', mm.gpr(3) - S5(0)), 0)[1]))
    m.stub(0x80115DC8, lambda mm: ev('pop', mm.gpr(3)))
    out['models'] = [m.get(S5(0x21194 + 8 * k + 4), 'I') - S5(0) for k in range(2)]
    for _ in range(200):
        clear(T, 0x80); clear(TWP, 0x40)
        ang = [rnd_ang() for _ in range(3)]
        pos = [rnd_f() for _ in range(3)]
        scl = [rnd_f() for _ in range(3)]
        put_twp(ang, pos, scl)
        row = rng.randrange(0x10000)
        m.put(TWP + 6, 'H', row)
        m.put(T + 0x34, 'I', TWP)
        frames = []
        for _ in range(4):
            frame['out'] = rng.random() < 0.3
            events.clear()
            ex = m.get(T + 0x10, 'I')
            m.call(ex if ex else S1(0x20310), r3=T)
            disp = m.get(T + 0x14, 'I')
            if disp:
                m.call(disp, r3=T)
            frames.append({'out': frame['out'], 'events': list(events), 'row': m.get(TWP + 6, 'H'),
                           'callbacks': [s1off(m.get(T + o, 'I')) for o in (0x10, 0x14, 0x18)]})
        out['cases'].append({'ang': ang, 'pos': [fb(v) for v in pos], 'scl': [fb(v) for v in scl],
                             'row': row, 'frames': frames})


{'big': big, 'light_inside': light_inside, 'light_sw': light_sw, 'modmod': modmod}[which]()
print(json.dumps(out))
