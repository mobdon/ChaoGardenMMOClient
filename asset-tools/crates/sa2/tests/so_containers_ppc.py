"""Run stg42D.rel's CONTIRON (0xE888 set-up, 0xE970 delete, 0xE9A8 exec)
and CONTWOOD (0xF4A0, 0xF588, 0xF5C0) under the main.dol harness and print
JSON (every float as its bit pattern). tests/so_containers.rs replays each
case with `stage_objects::containers` and compares.
Usage: python so_containers_ppc.py

Everything the containers call outside their own code is stubbed and
recorded: the main.dol engine routines and the stage REL's shared helpers
(stack registry 0x5238/0x5488, collapse 0x55F8, landing child 0x59E4, dust
0x1E8DC, debris 0x1EC18, flying pieces 0x7804/0x782C). fabs (0x84) runs as
compiled.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

T, TWP, CWP, INFO = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x300
CT, CTWP = SCRATCH + 0x400, SCRATCH + 0x500
HITS = SCRATCH + 0x600            # 8-byte hit records, +4 = hit task
HT = SCRATCH + 0x800              # hit tasks, 0x100 apart (+0x34 twp)
PL = SCRATCH + 0x2000             # player twp / pwp blocks
PLAYERPWP, PLAYERTWP = 0x801E7728, 0x801E7768
SLOT_TAG = 0x81300000             # value written into each DC model slot

KINDS = {
    'iron': dict(setup=0xE888, dest=0xE970, exec=0xE9A8, cnk=0x670,
                 slots=[0xEF34, 0xEF44, 0xEF54]),
    'wood': dict(setup=0xF4A0, dest=0xF588, exec=0xF5C0, cnk=0x678,
                 slots=[0xFF24, 0xFF34, 0xFF44]),
}
frame = {}
out = {'slot_tag': SLOT_TAG, 'setup': [], 'dest': [], 'cases': []}


def s5off(a):
    return a - S5(0) if S5(0) <= a < S5(0x20000) else a


def s1off(a):
    return a - S1(0) if S1(0) <= a < S1(0x40000) else a


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def ints(a):
    return [m.get(a + 4 * k, 'i') for k in range(3)]


def fr(i):
    return fb(f32(m.fpr(i)))


def pop(key):
    return frame[key].pop(0)


def rand_stub(mm):
    v = pop('rands')
    ev('rand', v)
    return v


def create_child(mm):
    ev('child', mm.gpr(4), mm.gpr(5), s5off(mm.gpr(6)), mm.gpr(7), mm.gpr(8))
    if frame['child_ok']:
        m.put(T + 0xC, 'I', CT)


def free_child(mm):
    ev('free_child')
    m.put(T + 0xC, 'I', 0)


def hit_record(mm):
    assert mm.gpr(3) == T
    i = mm.gpr(4)
    ev('hit_task', i)
    return HITS + 8 * i


def hit_check(mm):
    assert mm.gpr(3) == T
    k = (mm.gpr(4) - HT) // 0x100
    ev('hit_check', k)
    return frame['hit_ok'][k]


def stubs():
    m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), frame['out'])[1]))
    m.stub(0x8001DD78, ret(lambda mm: (ev('stage_num'), frame['stage'])[1]))
    m.stub(0x8000F7D8, lambda mm: ev('init_share', s5off(mm.gpr(4)), mm.gpr(5), mm.gpr(6)))
    m.stub(S1(0x5238), lambda mm: ev('stack_add'))
    m.stub(S1(0x5488), lambda mm: ev('stack_remove'))
    m.stub(S1(0x59E4), create_child)
    m.stub(0x8007098C, free_child)
    m.stub(S1(0x1E8DC), lambda mm: ev('dust', vec(mm.gpr(3)), fr(1), fr(2), ints(mm.gpr(4)), fr(3), fr(4), fr(5)))
    m.stub(0x800067C0, ret(lambda mm: (ev('hit_count'), len(frame['hit_ok']))[1]))
    m.stub(0x80005904, ret(hit_record))
    m.stub(0x800368A0, ret(lambda mm: (ev('iron_hit'), hit_check(mm))[1]))
    m.stub(0x8003699C, ret(lambda mm: (ev('wood_hit'), hit_check(mm))[1]))
    m.stub(0x8006AFFC, lambda mm: ev('sound', mm.gpr(3), mm.gpr(4) == TWP, mm.gpr(5), mm.gpr(6), mm.gpr(7), vec(mm.gpr(8))))
    m.stub(0x8010DC88, ret(rand_stub))
    m.stub(S1(0x1EC18), lambda mm: ev('debris', vec(mm.gpr(3)), fr(1), fr(2), fr(3)))
    m.stub(0x8006378C, lambda mm: ev('score', mm.gpr(3)))
    m.stub(0x80037EDC, ret(lambda mm: (ev('is_player', (mm.gpr(3) - HT) // 0x100), frame['pn'])[1]))
    m.stub(0x8003A3DC, lambda mm: ev('set_velocity', mm.gpr(3), fr(1), fr(2), fr(3)))
    m.stub(0x80067AF0, ret_f(lambda mm: (ev('shadow', fr(1), fr(2), fr(3), ints(mm.gpr(3))), frame['shadow'])[1]))
    m.stub(0x8000F4EC, lambda mm: ev('entry'))
    m.stub(S1(0x55F8), lambda mm: ev('collapse', vec(mm.gpr(3))))
    for w in (0x782C, 0x7804):
        m.stub(S1(w), (lambda w: lambda mm: ev(
            'piece', w, vec(mm.gpr(3)), mm.gpr(4), vec(mm.gpr(5)), mm.gpr(6),
            s5off(mm.gpr(7)), s5off(mm.gpr(8)), s5off(mm.gpr(9)) if w == 0x782C else 0,
            fr(1), fr(2)))(w))
    m.stub(0x80065404, ret(lambda mm: (ev('check_flag'), frame['flag20'])[1]))
    m.stub(0x80065414, lambda mm: ev('set_flag'))
    m.stub(0x800260FC, lambda mm: ev('item', fr(1), fr(2), fr(3), mm.gpr(3)))
    m.stub(0x800653B0, lambda mm: ev('dead_out'))
    m.stub(0x80071AE0, lambda mm: ev('free_task'))


def layout():
    clear(T, 0x50); clear(TWP, 0x40); clear(CWP, 0x20); clear(INFO, 0x20); clear(CTWP, 0x40)
    m.put(T + 0x34, 'I', TWP)
    m.put(TWP + 0x2C, 'I', CWP)
    m.put(CWP + 0xC, 'I', INFO)
    m.put(CT + 0x34, 'I', CTWP)


def rnd_ang():
    return rng.choice([0, 1, -1, 0x7fffffff, -0x80000000, rng.randrange(-0x80000000, 0x80000000),
                       rng.randrange(0, 0x10000)])


def rands(n):
    return [rng.choice([0, 32767, 1, 16384]) if rng.random() < 0.15 else rng.randrange(0, 32768) for _ in range(n)]


def state():
    child = m.get(T + 0xC, 'I')
    return {
        'smode': m.get(TWP + 1, 'b'), 'btimer': m.get(TWP + 3, 'B'), 'wtimer': m.get(TWP + 6, 'H'),
        'pos': vec(TWP + 0x14), 'fall': m.get(TWP + 0x24, 'I'), 'speed': m.get(TWP + 0x28, 'I'),
        'ground': m.get(T + 0x40, 'I'), 'attr': m.get(INFO + 4, 'I'),
        'child': vec(CTWP + 0x14) if child else None,
    }


def setup_cases():
    for kind, k in KINDS.items():
        for _ in range(60):
            layout()
            ang = [rnd_ang() for _ in range(3)]
            for i, a in enumerate(ang):
                m.put(TWP + 8 + 4 * i, 'i', a)
            sort = rng.randrange(0, 0x100000)
            m.put(T + 0x20, 'I', sort)
            cnk = rng.choice([0, 1])
            m.put(S6(k['cnk']), 'I', cnk)
            frame.update(out=rng.random() < 0.2, stage=rng.choice([0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0, 42, rng.randrange(0, 100)]))
            events.clear()
            m.call(S1(k['setup']), r3=T)
            out['setup'].append({
                'kind': kind, 'ang': ang, 'cnk': cnk, 'out': frame['out'], 'stage': frame['stage'],
                'events': list(events), 'state': state(),
                'exec': s1off(m.get(T + 0x10, 'I')), 'disp': s1off(m.get(T + 0x14, 'I')),
                'dest': s1off(m.get(T + 0x18, 'I')), 'sort': s1off(m.get(T + 0x20, 'I')), 'sort_before': sort,
            })
        for _ in range(10):
            layout()
            m.put(T + 0x40, 'f', rnd_f())
            events.clear()
            m.call(S1(k['dest']), r3=T)
            out['dest'].append({'kind': kind, 'events': list(events), 'ground': m.get(T + 0x40, 'I')})


def exec_cases():
    for kind, k in KINDS.items():
        for slot in k['slots']:
            m.put(S5(slot), 'I', SLOT_TAG + slot)
        for _ in range(400):
            layout()
            ang = [rnd_ang() for _ in range(3)]
            pos = [rnd_f() for _ in range(3)]
            fall = rng.choice([0.0, 0.0, 20.0, 40.0, -0.0, rnd_f(-5, 30), 0.5, 1e-30])
            speed = rng.choice([0.0, 0.0, -1.0, rnd_f(-5, 1), 0.3, -0.0])
            ground = rng.choice([-1000000.0, -1000000.0, f32(pos[1] - rng.uniform(-6, 6)), pos[1], rnd_f()])
            smode = rng.choice([0, 0, 0, 0, 1, 1, 1, 2, 2, 3, -1, 127])
            wtimer = rng.choice([0, 1, 2, 3, 31, 63, 0xffff, rng.randrange(0, 0x10000)])
            btimer = rng.randrange(0, 256)
            attr = rng.randrange(0, 1 << 32)
            child = rng.random() < 0.4
            ocp = rng.random() < 0.7
            m.put(TWP + 1, 'b', smode)
            m.put(TWP + 3, 'B', btimer)
            m.put(TWP + 6, 'H', wtimer)
            for i in range(3):
                m.put(TWP + 8 + 4 * i, 'i', ang[i])
                m.put(TWP + 0x14 + 4 * i, 'f', pos[i])
            m.put(TWP + 0x24, 'f', fall)
            m.put(TWP + 0x28, 'f', speed)
            m.put(T + 0x40, 'f', ground)
            m.put(INFO + 4, 'I', attr)
            m.put(T + 0xC, 'I', CT if child else 0)
            m.put(T + 0x30, 'I', 0x81234560 if ocp else 0)
            cnk = rng.choice([0, 1])
            m.put(S6(k['cnk']), 'I', cnk)
            case = {'kind': kind, 'cnk': cnk, 'ocp': ocp, 'start': state() | {'ang': ang}, 'frames': []}
            for _ in range(6):
                nh = rng.choice([0, 0, 1, 1, 2, 3])
                hit_ok = [rng.random() < 0.35 for _ in range(nh)]
                hit_id = [rng.choice([1, 1, 0, 2]) for _ in range(nh)]
                for i in range(nh):
                    m.put(HITS + 8 * i + 4, 'I', HT + 0x100 * i)
                    tw = HT + 0x100 * i + 0x40
                    m.put(HT + 0x100 * i + 0x34, 'I', tw)
                    m.put(tw + 2, 'B', hit_id[i])
                pn = rng.choice([-1, 0, 1, 2, 3])
                players = []
                for p in range(4):
                    # Player 0's pwp is always present: the exec reads its
                    # equipment through it without a check.
                    tw_ok = kind == 'wood' or rng.random() < 0.85
                    pw_ok = p == 0 or kind == 'wood' or rng.random() < 0.85
                    mode = rng.choice([0x45, 0x4e, 0, 0x45, 0x4e, rng.randrange(-128, 128)])
                    char = rng.choice([0, 4, 5, 0, 4, 5, 1, 2, 3, 6, rng.randrange(-128, 128)])
                    tw, pw = PL + 0x100 * p, PL + 0x100 * p + 0x80
                    m.put(tw, 'b', mode)
                    m.put(pw + 1, 'b', char)
                    m.put(PLAYERTWP + 4 * p, 'I', tw if tw_ok else 0)
                    m.put(PLAYERPWP + 4 * p, 'I', pw if pw_ok else 0)
                    players.append({'mode': mode if tw_ok else None, 'char': char if pw_ok else None})
                eq = rng.choice([0, 0x10, 0x8, 0x18, 0x40000, 0x40010, 0x1000, 0x8000000, 0x2000000,
                                 0x400, 0x410, rng.randrange(0, 1 << 32), 0])
                m.put(PLAYERPWP + 0x80 * 0 + 0, 'I', PL + 0x80)  # player 0 pwp
                m.put(PL + 0x80 + 0x24, 'I', eq)
                players[0]['char'] = m.get(PL + 0x80 + 1, 'b')
                frame.update(out=rng.random() < 0.1, hit_ok=hit_ok, pn=pn, rands=rands(60),
                             shadow=f32(rnd_f()), child_ok=rng.random() < 0.85, flag20=rng.random() < 0.2)
                fin = {'out': frame['out'], 'hit_ok': hit_ok, 'hit_id': hit_id, 'pn': pn,
                       'players': players, 'eq': eq, 'rands': list(frame['rands']),
                       'shadow': fb(frame['shadow']), 'child_ok': frame['child_ok'], 'flag20': frame['flag20']}
                events.clear()
                m.call(S1(k['exec']), r3=T, count=5_000_000)
                fin['events'] = list(events)
                fin['state'] = state()
                case['frames'].append(fin)
            out['cases'].append(case)


stubs()
setup_cases()
exec_cases()
print(json.dumps(out))
