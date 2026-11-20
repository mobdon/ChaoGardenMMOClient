"""Run stg42D.rel's HINTBOX and SCHBOX code (set-up, exec, destructor, and
SCHBOX's SearchBoxMan and PauseHint tasks) under the main.dol harness and
print JSON (every float as its bit pattern). tests/so_hintbox.rs replays
each case with `stage_objects::{hintbox, schbox}` and compares events and
state frame by frame. Usage: python so_hintbox_ppc.py hintbox|schbox

Engine routines are stubbed and recorded (task and collision set-up,
messages, sound, player control, DestroyTask, GetShadowPos, ...). Run
natively, as the Rust port reuses their native ports: njSin, atan2f,
EitherPlayerWithinSphere (0x800398D8), njDistanceP2P, njScalor and the
Ninja matrix routines.
"""
import json, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

T, TWP, CWP, WORK = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x300
PLAYERS = SCRATCH + 0x1000  # per slot: twp +0, its cwp +0x40, info +0x80
PWP = SCRATCH + 0x1400
HTAB, HUNT, PADS = SCRATCH + 0x2000, SCRATCH + 0x2400, SCRATCH + 0x2500
MAN, PAUSE = SCRATCH + 0x3000, SCRATCH + 0x3200
G = 0x801CC168
PLAYERTWP, PLAYERPWP = 0x801E7768, 0x801E7728
HINT_TEXT, TEXT_BASE = 0x81300000, 0x81310000
which = sys.argv[1]
out = {'object': which, 'cases': []}
frame = {}
sext8 = lambda v: v - 256 if v & 0x80 else v
sext16 = lambda v: v - 0x10000 if v & 0x8000 else v
s32 = lambda v: v - (1 << 32) if v & 0x80000000 else v


def text(p):
    if p == S5(0x10EAC):
        return 'default'
    if p == S5(0x1E3B8):
        return 'notext'
    if HINT_TEXT <= p < HINT_TEXT + 0x10000:
        return ['hint', (p - HINT_TEXT) // 0x100]
    return ['ptr', p]


def table_word(v):
    return None if v == 0 else text(v)


def look(mm):
    assert mm.gpr(4) == T and mm.gpr(6) == 1
    assert all(mm.get(mm.gpr(5) + 4 * k, 'I') == 0 for k in range(3))
    ev('look', mm.gpr(3))


def se(mm):
    assert mm.gpr(4) == 0 and mm.gpr(5) == 0 and mm.gpr(6) == 0
    ev('se', mm.gpr(3))


def msg(mm):
    ev('msg', mm.gpr(3), text(mm.gpr(4)), mm.gpr(5), sext8(mm.gpr(6) & 0xff))
    mm.put(mm.gpr(7), 'I', frame['msgtask'])


def calloc(mm):
    assert mm.gpr(3) == 1
    ev('calloc', mm.gpr(4))
    if frame['alloc']:
        clear(WORK, 0x40)
        mm.gpr(3, WORK)
    else:
        mm.gpr(3, 0)


def init_share(mm):
    assert mm.gpr(3) == T
    ev('init_share', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6))
    mm.put(TWP + 0x2C, 'I', CWP if frame['cwp'] else 0)


def create(mm):
    name = bytes(mm.u.mem_read(mm.gpr(6), 24)).split(b'\0')[0].decode()
    task = {'SearchBoxManExec': MAN, 'PauseHintExec': PAUSE}[name] if frame['create'] else 0
    ev('create', mm.gpr(3), mm.gpr(4), mm.gpr(5) - S1(0), name, task)
    if task:
        clear(task, 0x40)
        clear(task + 0x100, 0x40)
        mm.put(task + 0x34, 'I', task + 0x100)
    mm.gpr(3, task)


m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), frame['out'])[1]))
m.stub(0x801197FC, calloc)
m.stub(0x80119828, lambda mm: ev('free'))
m.stub(0x8000F8B8, lambda mm: ev('init', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6)))
m.stub(0x8000F7D8, init_share)
m.stub(0x8000F4EC, lambda mm: ev('entry'))
m.stub(0x80067AF0, lambda mm: (ev('shadow', fb(f32(mm.fpr(1))), fb(f32(mm.fpr(2))), fb(f32(mm.fpr(3)))),
                               mm.fpr(1, bf(frame['shadow']))))
m.stub(0x80022E0C, ret(lambda mm: frame['ctrl']))
m.stub(0x80022E24, lambda mm: ev('ctrl_off', mm.gpr(3)))
m.stub(0x80022E64, lambda mm: ev('ctrl_on', mm.gpr(3)))
m.stub(0x800E018C, ret(lambda mm: frame['busy']))
m.stub(0x800E0118, msg)
m.stub(0x80071AF0, lambda mm: ev('destroy', mm.gpr(3)))
m.stub(0x8006B978, se)
m.stub(0x8003A54C, look)
m.stub(0x800399BC, lambda mm: ev('setpos', mm.gpr(3), fb(f32(mm.fpr(1))), fb(f32(mm.fpr(2))), fb(f32(mm.fpr(3)))))
m.stub(0x8008C6C0, ret(lambda mm: frame['text']))
m.stub(0x80071E0C, create)
m.stub(0x800653B0, lambda mm: ev('deadout'))
m.stub(0x80071AE0, lambda mm: ev('freetask'))
m.stub(0x8001CD40, ret(lambda mm: frame['gstate']))


def rnd_angle():
    return rng.choice([0, 0x8000, -0x4000, rng.randrange(-0x10000, 0x10000), rng.randrange(-(1 << 31), 1 << 31)])


def players(case_players, box):
    """Write the player slots and return their JSON form."""
    out_ = []
    for i in range(2):
        twp, cwp, info, pwp = PLAYERS + 0x100 * i, PLAYERS + 0x100 * i + 0x40, PLAYERS + 0x100 * i + 0x80, PWP + 0x40 * i
        if not case_players[i]:
            m.put(PLAYERTWP + 4 * i, 'I', 0)
            m.put(PLAYERPWP + 4 * i, 'I', 0)
            out_.append(None)
            continue
        near = rng.random() < 0.8
        pos = [f32(box[k] + rng.uniform(-60, 60)) if near else rnd_f() for k in range(3)]
        if rng.random() < 0.1:
            pos = list(box)
        rpos = pos if rng.random() < 0.7 else [f32(box[k] + rng.uniform(-80, 80)) for k in range(3)]
        ang = [rnd_angle() for _ in range(3)]
        m.put(PLAYERTWP + 4 * i, 'I', twp)
        m.put(PLAYERPWP + 4 * i, 'I', pwp)
        for k in range(3):
            m.put(twp + 8 + 4 * k, 'i', ang[k])
            m.put(twp + 0x14 + 4 * k, 'f', pos[k])
            m.put(info + 8 + 4 * k, 'f', rpos[k])
        m.put(twp + 0x2C, 'I', cwp)
        m.put(cwp + 0xC, 'I', info)
        if rng.random() < 0.5:
            m.put(pwp + 0xE, 'b', rng.choice([0x32, 0x32, 0, 5]))
        out_.append({'ang': ang, 'pos': [fb(v) for v in pos], 'range': [fb(v) for v in rpos],
                     'action': m.get(pwp + 0xE, 'b')})
    return out_


def pwp_state(case_players):
    return [[m.get(PWP + 0x40 * i + 0xD, 'b'), m.get(PWP + 0x40 * i + 0xE, 'b')] if case_players[i] else None
            for i in range(2)]


def twp_state():
    return {'mode': m.get(TWP, 'b'), 'smode': m.get(TWP + 1, 'b'), 'player': m.get(TWP + 3, 'B'),
            'wtimer': m.get(TWP + 6, 'H'), 'ang': [m.get(TWP + 8 + 4 * k, 'i') for k in range(3)],
            'pos': [m.get(TWP + 0x14 + 4 * k, 'I') for k in range(3)],
            'scale': [m.get(TWP + 0x20 + 4 * k, 'I') for k in range(3)]}


def work_state(size):
    w = m.get(T + 0x38, 'I')
    if w == 0:
        return None
    assert w == WORK
    s = {'alpha': m.get(WORK, 'I'), 'fade': m.get(WORK + 4, 'I'), 'ang_y': m.get(WORK + 8, 'i'),
         'pos': [m.get(WORK + 0xC + 4 * k, 'I') for k in range(3)], 'spin': m.get(WORK + 0x18, 'i'),
         'phase': m.get(WORK + 0x1C, 'i'), 'bob': m.get(WORK + 0x20, 'I'), 'message': m.get(WORK + 0x24, 'I')}
    if size > 0x28:
        s['hint'] = m.get(WORK + 0x28, 'i')
    return s


def put_work(size):
    clear(WORK, 0x40)
    m.put(WORK, 'f', rng.choice([1.0, 0.4, 0.5, rnd_f(0, 1.2), float('nan')]))
    m.put(WORK + 4, 'f', rng.choice([0.0, 1.0, 0.95, 0.02, -0.01, rnd_f(-0.2, 1.2), float('nan')]))
    m.put(WORK + 8, 'i', rnd_angle())
    ty = m.get(TWP + 0x18, 'f')
    for k in range(3):
        m.put(WORK + 0xC + 4 * k, 'f', rnd_f())
    if rng.random() < 0.5:
        m.put(WORK + 0x10, 'f', ty)
    m.put(WORK + 0x18, 'i', rng.randrange(-(1 << 31), 1 << 31))
    m.put(WORK + 0x1C, 'i', rnd_angle())
    m.put(WORK + 0x20, 'f', rnd_f())
    m.put(WORK + 0x24, 'I', rng.choice([0, 0, 0x81500000]))
    if size > 0x28:
        m.put(WORK + 0x28, 'i', -1)
    m.put(T + 0x38, 'I', WORK)


def put_twp(modes, wtimers, slot1):
    clear(TWP, 0x40)
    m.put(TWP, 'b', rng.choice(modes))
    m.put(TWP + 1, 'b', rng.choice([0, 0, 6, 6, 1, -1]))
    m.put(TWP + 3, 'B', rng.choice([0, 1]) if slot1 else 0)
    m.put(TWP + 6, 'H', rng.choice(wtimers))
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', rnd_angle())
        m.put(TWP + 0x14 + 4 * k, 'f', rnd_f())
        m.put(TWP + 0x20 + 4 * k, 'f', rng.choice([rnd_f(), 0.0, 1.0, 2.5, -3.0, 9.9, float('nan')]))
    m.put(TWP + 0x2C, 'I', CWP)


def common_frame():
    frame['out'] = rng.random() < 0.1
    frame['frame'] = rng.choice([0, 4, 8, rng.randrange(1 << 32)])
    frame['lang'] = rng.choice([0, 1, 5, 0xff])
    frame['shadow'] = rng.choice([fb(-1000000.0), fb(rnd_f()), fb(0.0)])
    frame['ctrl'] = rng.choice([0, 1, 1])
    frame['busy'] = rng.choice([0, 0, 0, 0x81600000])
    frame['msgtask'] = rng.choice([0x81700040, 0x81700080, 0])
    frame['alloc'] = rng.random() < 0.9
    frame['cflag'] = rng.choice([0, 1, 2, 3])
    m.put(G + 0x7C, 'I', frame['frame'])
    m.put(G + 0x11, 'B', frame['lang'])
    m.put(CWP + 4, 'H', frame['cflag'])


# --- HINTBOX ----------------------------------------------------------------
def hintbox():
    for _ in range(800):
        slot1 = rng.random() < 0.5
        case_players = [True, slot1]
        count = rng.choice([-1, -1, 0, 1, 3, 10])
        m.put(S5(0x10EA8), 'i', count)
        m.put(S6(0x680), 'I', HTAB)
        for k in range(64):
            m.put(HTAB + 4 * k, 'I', HINT_TEXT + 0x100 * k)
        clear(T, 0x40)
        clear(PWP, 0x80)
        m.put(T + 0x34, 'I', TWP)
        put_twp([0, 1, 1, 2, 2, 3, 3, -1, 4, 127], [0, 0, 1, 30, 0xFFFF], slot1)
        setup = rng.random() < 0.3
        if not setup:
            put_work(0x2C)
        case = {'count': count, 'twp': twp_state(), 'work': None if setup else work_state(0x2C), 'frames': []}
        kinds = (['setup'] if setup else []) + ['exec'] * 14 + (['destroy'] if rng.random() < 0.4 else [])
        for kind in kinds:
            common_frame()
            pl = players(case_players, [m.get(TWP + 0x14 + 4 * k, 'f') for k in range(3)])
            events.clear()
            if kind == 'setup':
                m.call(S1(0x11694), r3=T)
            elif m.get(T + 0x38, 'I') == 0:
                continue  # the game runs neither without a work block
            elif kind == 'exec':
                m.call(S1(0x1180C), r3=T)
            else:
                m.call(S1(0x117C0), r3=T)
            f = {k: frame[k] for k in ('out', 'frame', 'lang', 'shadow', 'ctrl', 'busy', 'msgtask', 'alloc', 'cflag')}
            f.update({'kind': kind, 'players': pl, 'events': list(events), 'twp': twp_state(),
                      'work': work_state(0x2C), 'pwp': pwp_state(case_players),
                      'cb': [m.get(T + o, 'I') - S1(0) if m.get(T + o, 'I') else 0 for o in (0x10, 0x14, 0x18)]})
            case['frames'].append(f)
        out['cases'].append(case)


# --- SCHBOX -----------------------------------------------------------------
def shared_state():
    man, pause = m.get(S6(0x6BC), 'I'), m.get(S6(0x6C4), 'I')
    return {'texts': [table_word(m.get(S5(0x1E388) + 4 * k, 'I')) for k in range(3)],
            'man': None if man == 0 else {'task': man, 'wtimer': m.get(m.get(man + 0x34, 'I') + 6, 'H'),
                                          'cb': [m.get(man + 0x10, 'I') - S1(0), m.get(man + 0x18, 'I') - S1(0)]},
            'pause': None if pause == 0 else {
                'task': pause, 'wtimer': m.get(m.get(pause + 0x34, 'I') + 6, 'H'), 'message': m.get(pause + 0x38, 'I'),
                'cb': [m.get(pause + o, 'I') - S1(0) for o in (0x10, 0x14, 0x18)]},
            'high': m.get(S6(0x6C0), 'i'), 'alt': m.get(S6(0x6B8), 'i')}


def put_shared():
    for k in range(3):
        m.put(S5(0x1E388) + 4 * k, 'I', rng.choice([0, 0, TEXT_BASE + 0x10 * k, S5(0x1E3B8)]))
    # Tasks the code started earlier, with the callbacks it installs.
    for g, task, cbs in ((0x6BC, MAN, {0x10: 0x1B8F0, 0x18: 0x1B8D0}),
                         (0x6C4, PAUSE, {0x10: 0x1BEAC, 0x14: 0x1BAC0, 0x18: 0x1BED8})):
        if rng.random() < 0.5:
            clear(task, 0x40)
            clear(task + 0x100, 0x40)
            for o, f in cbs.items():
                m.put(task + o, 'I', S1(f))
            m.put(task + 0x34, 'I', task + 0x100)
            m.put(task + 0x100 + 6, 'H', rng.choice([0, 1, 0x12C, 0x12D, 0xFFFF, rng.randrange(0x10000)]))
            m.put(task + 0x38, 'I', rng.choice([0, 0x81700100]))
            m.put(S6(g), 'I', task)
        else:
            m.put(S6(g), 'I', 0)
    m.put(S6(0x6C0), 'i', rng.choice([0, 1]))
    m.put(S6(0x6B8), 'i', rng.choice([0, 0, 1]))


def schbox_frame():
    common_frame()
    frame['create'] = rng.random() < 0.85
    frame['cwp'] = rng.random() < 0.8
    m.put(CWP + 8, 'I', 0)
    frame['text'] = rng.choice([0, TEXT_BASE + 0x400, TEXT_BASE + 0x800])
    frame['gstate'] = rng.choice([0x11, 0x11, 0x11, 0, 0xFFFFFFFF])
    frame['hunt'] = rng.choice([None, 0, 1, 2, 2, 3, -1, 5])
    frame['lock'] = rng.choice([0, 0, 0, 1])
    frame['play_mode'] = rng.choice([0, 0, 0, 0, 1, 2, 3])
    frame['pause_pad'] = rng.choice([1, 1, 2, 0, 3])
    frame['mask'] = rng.choice([0, 1, 2, 3, 3, -1])
    frame['from_set'] = rng.random() < 0.5
    buttons = [0, 0, 0x10000, 0x20000, 0x30000, 0x600, 0x200, 0x10600]
    frame['pads'] = [[rng.choice(buttons), rng.choice(buttons)] for _ in range(2)]
    m.put(G + 0x23, 'b', frame['play_mode'])
    m.put(G + 0x17, 'b', frame['pause_pad'])
    m.put(G + 0x38, 'b', frame['mask'])
    m.put(T + 0x30, 'I', 0x81000000 if frame['from_set'] else 0)
    if frame['hunt'] is None:
        m.put(0x803ADA88, 'I', 0)
    else:
        m.put(0x803ADA88, 'I', HUNT)
        m.put(HUNT + 3, 'b', frame['hunt'])
    m.put(0x803ADAD0, 'i', frame['lock'])
    for p in range(2):
        m.put(0x801E55D8 + 4 * p, 'I', PADS + 0x40 * p)
        m.put(PADS + 0x40 * p + 8, 'I', frame['pads'][p][0])
        m.put(PADS + 0x40 * p + 0x10, 'I', frame['pads'][p][1])


SCH_KEYS = ('out', 'frame', 'lang', 'shadow', 'ctrl', 'busy', 'msgtask', 'alloc', 'cflag', 'create', 'cwp', 'text',
            'gstate', 'hunt', 'lock', 'play_mode', 'pause_pad', 'mask', 'from_set', 'pads')


def schbox():
    for _ in range(1000):
        slot1 = rng.random() < 0.5
        case_players = [True, slot1]
        clear(T, 0x40)
        clear(PWP, 0x80)
        m.put(T + 0x34, 'I', TWP)
        put_twp([0, 0, 1, 1, 1, 2, 2, 3, 3, -1, 4, 100], [0, 0, 0, 1, 0x3B, 0x3C, 0xFFFF], slot1)
        put_shared()
        setup = rng.random() < 0.3
        if not setup:
            put_work(0x28)
        case = {'twp': twp_state(), 'work': None if setup else work_state(0x28), 'shared': shared_state(),
                'frames': []}
        kinds = (['setup'] if setup else []) + rng.choices(
            ['exec', 'man_exec', 'man_dest', 'pause_exec', 'pause_disp', 'pause_dest'],
            weights=[12, 2, 0.5, 1, 4, 0.5], k=16) + (['destroy'] if rng.random() < 0.3 else [])
        for kind in kinds:
            schbox_frame()
            pl = players(case_players, [m.get(TWP + 0x14 + 4 * k, 'f') for k in range(3)])
            events.clear()
            man, pause = m.get(S6(0x6BC), 'I'), m.get(S6(0x6C4), 'I')
            if kind == 'setup':
                m.call(S1(0x1BFB4), r3=T)
            elif kind in ('exec', 'destroy') and m.get(T + 0x38, 'I') == 0:
                continue  # the game runs neither without a work block
            elif kind == 'exec':
                m.call(S1(0x1C1B0), r3=T)
            elif kind == 'destroy':
                m.call(S1(0x1C164), r3=T)
            elif kind.startswith('man'):
                if not man:
                    continue
                m.call(S1({'man_exec': 0x1B8F0, 'man_dest': 0x1B8D0}[kind]), r3=man)
            else:
                if not pause:
                    continue
                m.call(S1({'pause_exec': 0x1BEAC, 'pause_disp': 0x1BAC0, 'pause_dest': 0x1BED8}[kind]), r3=pause)
            f = {k: frame[k] for k in SCH_KEYS}
            f.update({'kind': kind, 'players': pl, 'events': list(events), 'twp': twp_state(),
                      'work': work_state(0x28), 'pwp': pwp_state(case_players), 'shared': shared_state(),
                      'cb': [m.get(T + o, 'I') - S1(0) if m.get(T + o, 'I') else 0 for o in (0x10, 0x14, 0x18, 0x1C)],
                      'colli_range': m.get(CWP + 8, 'I')})
            if kind == 'pause_dest':
                f['pause_message'] = m.get(pause + 0x38, 'I')
            case['frames'].append(f)
        out['cases'].append(case)


{'hintbox': hintbox, 'schbox': schbox}[which]()
print(json.dumps(out))
