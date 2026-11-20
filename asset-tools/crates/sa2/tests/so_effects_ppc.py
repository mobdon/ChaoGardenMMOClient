"""Run stg42D.rel's shared particle creators and exec callbacks under the
main.dol harness and print JSON (every float as its bit pattern).
tests/so_effects.rs replays each case with `stage_objects::effects`.
Usage: python so_effects_ppc.py creators|execs

The particle allocator fn_80032B78 is stubbed (it hands out a scratch block
filled with random words, or null) and recorded; MSL rand (0x8010DC88),
njSin (0x80112398) and __cvt_fp2unsigned (0x8010AF3C) run natively, rand
from a seed each case sets (0x803AD6A0).
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

SEED = 0x803AD6A0
PART, DESC, POS, VEL = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x300
frame = {}
which = sys.argv[1]
out = {'object': which, 'cases': []}

CREATORS = [('A67C', 0xA67C), ('C238', 0xC238), ('MakeParticle', 0xC56C), ('C8A0', 0xC8A0), ('CCFC', 0xCCFC),
            ('D0C4', 0xD0C4), ('D540', 0xD540), ('D714', 0xD714), ('E178', 0xE178), ('E334', 0xE334)]
EXECS = [('A7F0', 0xA7F0), ('1E6BC', 0x1E6BC), ('C3D4', 0xC3D4), ('C708', 0xC708), ('CA94', 0xCA94), ('CF04', 0xCF04),
         ('D2B8', 0xD2B8), ('D448', 0xD448), ('D8A0', 0xD8A0), ('E4F8', 0xE4F8)]


def words(a):
    return [m.get(a + 4 * k, 'I') for k in range(16)]


def rnd_word():
    return rng.choice([0, 0xffffffff, 0x80000000, 0x7fffffff, rng.randrange(1 << 32),
                       rng.randrange(1 << 32), rng.randrange(1 << 32)])


def special_f():
    return rng.choice([0.0, -0.0, 1.0, -1.0, 0.5, 7.9, 8.0, 15.5, 16.0, 31.99, 32.0, 1e9,
                       2147483648.0, 3e9, 4294967296.0, 1e30, -1e30, -5.0,
                       float('inf'), float('-inf'), float('nan')])


def creators():
    def alloc(mm):
        ev('alloc', mm.gpr(3) - S5(0))
        mm.gpr(3, 0 if frame['fail'] else PART)
    m.stub(0x80032B78, alloc)
    for name, off in CREATORS:
        for _ in range(120):
            seed = rng.choice([0, 1, 0xffffffff, rng.randrange(1 << 32), rng.randrange(1 << 32)])
            pre = [rnd_word() for _ in range(16)]
            pos = [rnd_f() for _ in range(3)]
            vel = [rnd_f(-5, 5) for _ in range(3)]
            # f1/f2 always hold singles (the callers load them with lfs).
            size, work = f32(rnd_f(0, 10)), f32(rnd_f(-100, 100))
            frame['fail'] = rng.random() < 0.1
            for k, w in enumerate(pre):
                m.put(PART + 4 * k, 'I', w)
            for k in range(3):
                m.put(POS + 4 * k, 'f', pos[k])
                m.put(VEL + 4 * k, 'f', vel[k])
            m.put(SEED, 'I', seed)
            events.clear()
            m.call(S1(off), r3=POS, r4=VEL, f1=size, f2=work)
            out['cases'].append({
                'kind': name, 'seed': seed, 'pre': pre, 'pos': [fb(x) for x in pos],
                'vel': [fb(x) for x in vel], 'size': fb(size), 'work': fb(work),
                'fail': frame['fail'], 'events': list(events), 'ret': m.gpr(3),
                'post': words(PART), 'seed_after': m.get(SEED, 'I')})


def execs():
    for name, off in EXECS:
        for _ in range(120):
            seed = rng.choice([0, 1, rng.randrange(1 << 32), rng.randrange(1 << 32)])
            # Finite descriptor values and velocities (stage data): an
            # inf - inf would only test the host FPU's default NaN sign.
            u = lambda lo, hi: f32(rng.uniform(lo, hi))
            desc = [rng.choice([0, 1, 2, 8, 0xffffffff, 0x8000, rng.randrange(1, 40)]),
                    fb(rng.choice([0.09, 0.5, 1.0, -0.5, u(-2, 2)])),
                    fb(rng.choice([0.98, 0.93, 1.0, u(-1, 2)])),
                    fb(rng.choice([-0.01, 0.007, u(-1, 1)])),
                    fb(rng.choice([0.001, -0.04, u(-1, 1)]))]
            p = [rnd_word() for _ in range(16)]
            p[1] = fb(rng.choice([rnd_f(-1, 10), special_f(), 3.0]))
            for k in (2, 3, 4, 8, 9, 10):
                p[k] = fb(u(-50, 50))
            p[5] = rng.choice([rnd_word(), 0xff000000 | rng.randrange(1 << 24), rng.randrange(12 << 24)])
            p[6] = fb(rng.choice([rnd_f(-2, 40), special_f(), 7.95, 32767.5, 32768.0, -32769.0, 65535.0]))
            if name in ('CF04', 'D2B8'):
                p[14] = fb(rng.choice([rnd_f(-5, 100), special_f()]))
            for k, w in enumerate(desc):
                m.put(DESC + 0xC + 4 * k, 'I', w)
            for k, w in enumerate(p):
                m.put(PART + 4 * k, 'I', w)
            m.put(SEED, 'I', seed)
            frames = []
            for _ in range(5):
                m.call(S1(off), r3=DESC, r4=PART)
                frames.append({'ret': m.gpr(3), 'p': words(PART), 'seed': m.get(SEED, 'I')})
            out['cases'].append({'kind': name, 'seed': seed, 'desc': desc, 'p': p, 'frames': frames})


{'creators': creators, 'execs': execs}[which]()
print(json.dumps(out))
