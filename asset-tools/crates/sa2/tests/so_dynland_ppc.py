"""Run stg42D.rel's movable-object helpers (section 1 0x378C..0x5238) under
the main.dol harness and print JSON (floats as bit patterns).
tests/so_dynland.rs replays each case with `stage_objects::dyn_land`.
Usage: python so_dynland_ppc.py <update|physics|setup|plane>

Stubbed and recorded: syCalloc / syFree, the land probes GetShadowPosXYZ
and fn_80066E1C (they write a scripted block the way main.dol does: every
ray's +0 cleared and coordinate reset, the rest only for hits) and
CCL_Entry. Everything else runs as compiled: the REL's fabsf and sqrtf,
CCL_IsPushed, ObjectMovableInitialize, AdjustAngle, fn_80012EA4, atan2f and
the Ninja matrix and vector routines.

so_skull_ppc.py imports the body helpers and stubs from here.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403
from dol_ppc import SDA2_BASE  # noqa: E402

WORDS = 0x188 // 4
GLOBAL = 0x801CC168
NO_HIT = (m.get(SDA2_BASE - 0x7180, 'I'), m.get(SDA2_BASE - 0x717C, 'I'))  # -1e6, 1e6
# Probe point axis per ray (X below/above, Y below/above, Z below/above).
AXIS = [0, 0, 1, 1, 2, 2]
FLAG_BITS = [1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80, 0x100, 0x2000, 0x4000, 0x8000, 0x10000, 0x80000000]


def vec(a):
    return [m.get(a + 4 * k, 'I') for k in range(3)]


def put_words(a, w):
    for k, x in enumerate(w):
        m.put(a + 4 * k, 'I', x & 0xffffffff)


def get_words(a, n=WORDS):
    return [m.get(a + 4 * k, 'I') for k in range(n)]


def rnd_mtx():
    if rng.random() < 0.4:
        return [fb(1.0), 0, 0, 0, 0, fb(1.0), 0, 0, 0, 0, fb(1.0), 0]
    return [fb(f32(rng.uniform(-1.2, 1.2))) if c != 3 else fb(f32(rng.uniform(-5, 5)))
            for r in range(3) for c in range(4)]


def rnd_unitish():
    v = [rng.gauss(0, 1) for _ in range(3)]
    n = sum(x * x for x in v) ** 0.5 or 1.0
    return [f32(x / n) for x in v]


def rnd_record(hit=None):
    """A stale or fresh 0x20-byte ray record."""
    return [1 if (rng.random() < 0.5 if hit is None else hit) else 0, rng.randrange(1 << 32),
            rng.randrange(-0x8000, 0x8000) & 0xffffffff, rng.randrange(-0x8000, 0x8000) & 0xffffffff,
            fb(rnd_f(-300, 300))] + [fb(x) for x in rnd_unitish()]


def rnd_flags():
    f = 0
    for b in FLAG_BITS:
        if rng.random() < 0.3:
            f |= b
    if rng.random() < 0.1:
        f |= rng.randrange(1 << 32)
    return f


def rnd_body(pos):
    """A body as words: defaults or random values in every field."""
    w = rnd_mtx() + rnd_mtx()
    w.append(rnd_flags())
    w += [fb(rng.choice([0.0, f32(rng.uniform(-6, 6))])), fb(rng.choice([5.0, f32(rng.uniform(-8, 8))])),
          fb(rng.choice([0.0, f32(rng.uniform(-6, 6))]))]
    w.append(fb(rng.choice([15.0, f32(rng.uniform(0, 30))])))    # head
    w.append(fb(rng.choice([10.0, f32(rng.uniform(0, 20))])))    # radius
    w.append(fb(rng.choice([-5.0, f32(rng.uniform(-15, 5))])))   # foot
    w.append(fb(rng.choice([50.0, f32(rng.uniform(0, 60))])))    # step
    w.append(fb(rng.choice([0.8, f32(rng.uniform(0, 1.5))])))    # wall bounce
    w.append(fb(rng.choice([0.7, f32(rng.uniform(0, 1.5))])))    # ground bounce
    w.append(fb(rng.choice([0.7, f32(rng.uniform(0, 1.5))])))    # ground drag
    w.append(fb(0.1))
    w.append(rng.choice([0x400, rng.randrange(-0x10000, 0x10000) & 0xffffffff]))
    w += [fb(f32(p + rng.uniform(-10, 10))) for p in map(bf, pos)]  # last_pos
    w += [fb(x) for x in rnd_unitish()]                              # ground normal
    w += [fb(rnd_f()) for _ in range(3)]                             # shadow point (stale)
    for _ in range(6):
        w += rnd_record()
    w += [fb(rnd_f(-10, 10)) for _ in range(3)]                      # probe_turned
    w.append((rng.randrange(0x10000) << 16))                         # task flag, padding 0
    assert len(w) == WORDS
    return w


def script_ray(r, point, st):
    """What the probe finds along ray r: None or (flags, ax, az, coord, normal)."""
    p = bf(point[AXIS[r]])
    if rng.random() < (0.35 if r in (2, 3) else 0.45):
        return None
    span = rng.choice([3.0, 12.0, 25.0, 300.0])
    coord = f32(p + rng.uniform(-span, span))
    if rng.random() < 0.05:
        coord = rng.choice([p, -1e30, 1e30, 0.0])
    k = rng.random()
    if k < 0.08:
        n = [0.0, 0.0, 0.0]
    elif k < 0.55:
        # Roughly facing back along the ray.
        n = [0.0, 0.0, 0.0]
        n[AXIS[r]] = 1.0 if r % 2 == 0 else -1.0
        n = [f32(x + rng.uniform(-0.6, 0.6)) for x in n]
    else:
        n = rnd_unitish()
    return (rng.randrange(1 << 32), rng.randrange(-0x8000, 0x8000), rng.randrange(-0x8000, 0x8000),
            f32(coord), n)


def make_shadow_stub(state):
    """GetShadowPosXYZ (water False) / fn_80066E1C (True): write a scripted
    block the way main.dol does and record it."""
    def stub(water):
        def run(mm):
            b = mm.gpr(3)
            point = vec(b)
            rays = []
            for r in range(6):
                rec = b + 0xC + 0x20 * r
                s = script_ray(r, point, state)
                m.put(rec, 'I', 0)
                if s is None:
                    m.put(rec + 0x10, 'I', NO_HIT[r % 2])
                    rays.append([0, NO_HIT[r % 2], 0, 0, 0, 0, 0, 0])
                    continue
                flags, ax, az, coord, n = s
                vals = [1, flags, ax & 0xffffffff, az & 0xffffffff, fb(coord)] + [fb(x) for x in n]
                put_words(rec, vals)
                rays.append([1, fb(coord), flags, ax, az] + [fb(x) for x in n])
            ret = rng.random() < 0.5
            ev('shadow', 1 if water else 0, point, rays, 1 if (water and ret) else 0)
            mm.gpr(3, 1 if ret else 0)
        return run
    m.stub(0x80067210, stub(False))
    m.stub(0x80066E1C, stub(True))


def dyn_stubs(task, land, state, entry=True):
    """The engine boundary of the helpers, for a task at `task` whose body
    (when allocated) lives at `land`. `entry` False leaves CCL_Entry to a
    caller that stubs it already (a second stub would record it twice)."""
    make_shadow_stub(state)
    m.stub(0x801197FC, lambda mm: (ev('calloc', mm.gpr(3), mm.gpr(4)), clear(land, 0x188),
                                   mm.gpr(3, land if state['calloc'] else 0)))
    m.stub(0x80119828, lambda mm: (assert_(mm.gpr(3) == land), ev('free')))
    if entry:
        m.stub(0x8000F4EC, lambda mm: (assert_(mm.gpr(3) == task), ev("entry")))


def assert_(c):
    assert c


# --- this file's own cases ---------------------------------------------------
T = SCRATCH
TWP, CWP, MWP, LAND = T + 0x100, T + 0x200, T + 0x300, T + 0x400
state = {'calloc': True}


def task_reset(has_cwp=True, has_mwp=True):
    clear(T, 0x800)
    m.put(T + 0x34, 'I', TWP)
    m.put(T + 0x38, 'I', MWP if has_mwp else 0)
    m.put(TWP + 0x2C, 'I', CWP if has_cwp else 0)


def rnd_task():
    return {'flag': rng.choice([0, -0x8000, rng.randrange(-0x8000, 0x8000), rng.randrange(0, 0x8000)]),
            'b2': rng.randrange(256),
            'ang': [rng.choice([0, rng.randrange(-0x4000, 0x4000), rng.randrange(-1 << 31, 1 << 31)])
                    for _ in range(3)],
            'pos': [fb(rnd_f(-300, 300)) for _ in range(3)],
            'speed': [fb(rng.choice([0.0, 0.0005, -0.0005, f32(rng.uniform(-0.01, 0.01)),
                                     f32(rng.uniform(-4, 4))])) for _ in range(3)]}


def put_task(t):
    m.put(TWP + 2, 'B', t['b2']); m.put(TWP + 4, 'h', t['flag'])
    for k in range(3):
        m.put(TWP + 8 + 4 * k, 'i', t['ang'][k])
        m.put(TWP + 0x14 + 4 * k, 'I', t['pos'][k])
        m.put(MWP + 4 * k, 'I', t['speed'][k])


def get_task():
    return {'flag': m.get(TWP + 4, 'h'), 'b2': m.get(TWP + 2, 'B'),
            'ang': [m.get(TWP + 8 + 4 * k, 'i') for k in range(3)], 'pos': vec(TWP + 0x14),
            'speed': vec(MWP)}


out = {'cases': []}


def update():
    dyn_stubs(T, LAND, state)
    for _ in range(1500):
        has_cwp = rng.random() < 0.8
        task_reset(has_cwp)
        t = rnd_task()
        put_task(t)
        body = rnd_body(t['pos'])
        put_words(LAND, body)
        m.put(T + 0x3C, 'I', LAND)
        case = {'task': t, 'has_cwp': has_cwp, 'body': body, 'frames': []}
        for _ in range(8):
            cwp_flag = rng.choice([0, 0x10, rng.randrange(0x10000)])
            m.put(CWP + 4, 'H', cwp_flag)
            g = rng.choice([0, 0, 0, 1, -1])
            m.put(GLOBAL + 0x37, 'b', g)
            if rng.random() < 0.15:
                m.put(TWP + 4, 'h', rng.choice([-0x8000, 0, rng.randrange(-0x8000, 0x8000)]))
            flag = m.get(TWP + 4, 'h')
            events.clear()
            m.call(S1(0x4BE0), r3=T)
            case['frames'].append({'cwp_flag': cwp_flag, 'g': g, 'flag': flag, 'events': list(events),
                                   'task': get_task(), 'body': get_words(LAND)})
        out['cases'].append(case)


def physics():
    dyn_stubs(T, LAND, state)
    for _ in range(3000):
        task_reset()
        t = rnd_task()
        put_task(t)
        body = rnd_body(t['pos'])
        put_words(LAND, body)
        events.clear()
        m.call(S1(0x491C), r3=TWP + 0x14, r4=MWP, r5=TWP + 8, r6=LAND)
        out['cases'].append({'task': t, 'body': body, 'events': list(events), 'after': get_task(),
                             'after_body': get_words(LAND)})


def setup():
    dyn_stubs(T, LAND, state)
    for _ in range(300):
        has_mwp = rng.random() < 0.8
        task_reset(True, has_mwp)
        t = rnd_task()
        put_task(t)
        state['calloc'] = rng.random() < 0.8
        m.put(T + 0x3C, 'I', 0)
        events.clear()
        m.call(S1(0x4B44), r3=T)
        land = m.get(T + 0x3C, 'I')
        assert land in (0, LAND)
        after = {'task': get_task(), 'body': get_words(LAND) if land else None}
        put_words(LAND, rnd_body(t['pos']))
        ev_setup = list(events)
        events.clear()
        m.call(S1(0x4BA8), r3=T)
        out['cases'].append({'task': t, 'has_mwp': has_mwp, 'calloc': state['calloc'], 'events': ev_setup,
                             'after': after, 'release': list(events), 'land_after': m.get(T + 0x3C, 'I')})
    state['calloc'] = True


def plane():
    buf = T + 0x600
    for _ in range(3000):
        p = [rnd_f(-300, 300) for _ in range(3)]
        q = [rnd_f(-300, 300) for _ in range(3)]
        n = rng.choice([rnd_unitish(), [0.0, 0.0, 0.0], [f32(rng.uniform(-3, 3)) for _ in range(3)]])
        for k in range(3):
            m.put(buf + 4 * k, 'f', p[k]); m.put(buf + 0x10 + 4 * k, 'f', q[k])
            m.put(buf + 0x1C + 4 * k, 'f', n[k])
        clear(buf + 0x40, 12)
        m.call(0x80012EA4, r3=buf, r4=buf + 0x10, r5=buf + 0x40)
        out['cases'].append({'p': [fb(x) for x in p], 'q': [fb(x) for x in q], 'n': [fb(x) for x in n],
                             'd': fb(f32(m.fpr(1))), 'foot': vec(buf + 0x40)})


if __name__ == '__main__':
    {'update': update, 'physics': physics, 'setup': setup, 'plane': plane}[sys.argv[1]]()
    print(json.dumps(out))
