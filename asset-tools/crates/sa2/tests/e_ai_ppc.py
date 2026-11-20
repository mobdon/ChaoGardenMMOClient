"""Run stg09D.rel's E AI (row callback 0x51034), or with SA2_ENEMY=gold its
E GOLD (0x4A024), frame by frame under the
dol_ppc harness, with the game around it scripted, and print JSON: each
frame's inputs, the task and enemy work bytes after the frame, and the
calls it made to the routines enemy_lib leaves to its host.
tests/e_ai.rs replays the same inputs through enemy_lib::e_ai.

Real code: the REL's E AI and E EMI-family routines, and main.dol's enemy
helpers, motion sequencer, Ninja math, rand, CCL_Init and fn_800164FC.
Stubbed and recorded: culling, the CCL hit queries, ground collision
(fn_80017C00), sounds, score, debris, the REL's child-task creators and
the player reactions.

Needs SA2_MAIN_DOL and SA2_STAGE_REL (GameCube stg09D.rel), never committed.
"""
import json, os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402
from stage_rel_ppc import load_rel  # noqa: E402

dol = Dol(os.environ["SA2_MAIN_DOL"])
m = dol.machine()
rel = load_rel(m, os.environ["SA2_STAGE_REL"])
R1 = lambda off: rel.addr(1, off)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
f32 = lambda x: struct.unpack('>f', struct.pack('>f', x))[0]

HEAP = [0x81100000]
TASK, SETENTRY, PARTNER, A4WORD = 0x81000000, 0x81000080, 0x810000C0, 0x810000E0
TWP = 0x81000100
PLAYERS = [0x81000600, 0x81000700]
PCWP = [0x81000800, 0x81000900]
PINFO = [0x81000A00, 0x81000B00]
PLAYERTWP, GAME, NEXT, COUNTS = 0x801E7768, 0x801CC168, 0x803AD6A0, 0x801E4AB8
SINGLETON = rel.addr(6, 0x2DE0)

events = []
frame_in = {}


def ev(*e):
    events.append(list(e))


def ret(v):
    def f(mm):
        mm.gpr(3, v() if callable(v) else v)
    return f


def calloc(mm):
    n, size = mm.gpr(3), mm.gpr(4)
    a = HEAP[0]
    HEAP[0] = (a + n * size + 0x1F) & ~0x1F
    mm.u.mem_write(a, bytes(n * size))
    mm.gpr(3, a)


FAKE = []


def fake_task(mm):
    a = HEAP[0]
    FAKE.append(a)
    HEAP[0] += 0x100
    mm.u.mem_write(a, bytes(0x100))
    mm.put(a + 0x34, 'I', a + 0x80)
    mm.put(a + 0x38, 'I', a + 0xC0)
    mm.gpr(3, a)


def vec(mm, reg):
    a = mm.gpr(reg)
    return [mm.get(a + 4 * k, 'I') for k in range(3)]


def fbits(mm, i):
    return fb(f32(mm.fpr(i)))


m.stub(0x801197FC, calloc)                       # syCalloc
m.stub(0x80071E0C, fake_task)                    # CreateElementalTask
m.stub(0x800709F8, fake_task)                    # CreateChildTask
m.stub(0x80072820, lambda mm: None)
m.stub(0x8008CEB0, lambda mm: None)              # in fn_80018C88
m.stub(0x8008C8F8, lambda mm: None)              # in fn_80017B94
m.stub(0x800655F4, ret(lambda: frame_in['range_out']))   # CheckRangeOut
m.stub(0x80071AE0, lambda mm: ev('free'))        # FreeTask
m.stub(0x800653B0, lambda mm: ev('free'))        # DeadOut
m.stub(0x80065414, lambda mm: ev('set_dead'))    # _rename_SetFlag0x20
m.stub(0x80006D7C, lambda mm: None)              # CCL_ClearSearch


def is_hit_player(mm):
    a = mm.gpr(3)
    mm.gpr(3, (frame_in['hit_player'] if a == TASK else frame_in['hit_by_player']) and 1)


m.stub(0x800066F4, is_hit_player)
PARTNER_TWP = 0x81000480
m.stub(0x80006574, ret(lambda: PARTNER if frame_in['held_hit'] else 0))
m.stub(0x8006544C, ret(lambda: frame_in['broken']))   # CheckBroken
m.stub(0x80065484, lambda mm: ev('set_broken'))     # SetBroken
m.stub(0x800168C8, lambda mm: None)
m.stub(0x8003679C, ret(0))
m.stub(0x80037EDC, ret(lambda: frame_in['hit_index'] & 0xffffffff))   # IsThisTaskPlayer
m.stub(0x80035C54, ret(lambda: frame_in['score_doubled']))


def ground(mm):
    w = mm.get(TASK + 0x38, 'I')
    mm.put(w + 0xF8, 'I', frame_in['probe_y'])
    mm.put(w + 0x150, 'I', frame_in['shadow_y'])
    if frame_in['landed']:
        mm.put(w + 0x4C, 'H', mm.get(w + 0x4C, 'H') | 0x80)
    ev('ground')


m.stub(0x80017C00, ground)
m.stub(0x8006B7EC, lambda mm: ev('sound', mm.gpr(3), mm.gpr(6), vec(mm, 7)))
m.stub(0x8006A5F4, lambda mm: ev('task_sound', mm.gpr(3), mm.gpr(6)))
m.stub(0x80062E0C, lambda mm: ev('score', mm.gpr(3)))
m.stub(0x80016D10, lambda mm: ev('debris'))
m.stub(0x80015730, lambda mm: ev('carried', mm.gpr(3), mm.gpr(4) - rel.bases[5]))
m.stub(0x80039EB0, lambda mm: ev('knock', mm.gpr(3), vec(mm, 4), [mm.get(mm.gpr(5) + 4 * k, 'i') for k in range(3)]))
m.stub(0x8002FB2C, lambda mm: ev('hurt', mm.gpr(3), mm.gpr(4), mm.gpr(5)))
# E KUMI's main.dol calls.
m.stub(0x80067AF0, lambda mm: (ev('shadow', fbits(mm, 1), fbits(mm, 2), fbits(mm, 3)), mm.fpr(1, f32(struct.unpack('>f', struct.pack('>I', frame_in['shadow_pos']))[0]))))
m.stub(0x800068BC, lambda mm: ev('range', fbits(mm, 1)))
m.stub(0x80005F88, ret(lambda: PARTNER if frame_in['spring'] else 0))
m.stub(0x8003A2B8, lambda mm: ev('spring', mm.gpr(3), fbits(mm, 2)))
m.stub(0x8006B978, lambda mm: None)              # SE_Call
m.stub(0x800BD92C, lambda mm: ev('burrow', vec(mm, 4)))
m.stub(0x8011F50C, ret(0x10))
# The shared display tasks' texture and fog set-up.
for a in (0x80116E18, 0x80024CB8, 0x8011C2B0, 0x8011C310, 0x8011F520, 0x8011F504):
    m.stub(a, lambda mm: None)
m.stub(R1(0x427EC), lambda mm: ev('child', 0x427EC))
m.stub(R1(0x47468), lambda mm: ev('child', 0x47468))
m.stub(R1(0x53F0C), lambda mm: ev('child', 0x53F0C, {0: 3, 1: 6, 2: 9, 3: 3}[mm.get(mm.get(mm.gpr(3) + 0x34, 'I') + 1, 'b')]))
m.stub(R1(0x4286C), lambda mm: ev('child', 0x4286C, vec(mm, 6), mm.gpr(3), mm.gpr(4), mm.gpr(5)))
m.stub(0x8000F748, lambda mm: ev('clear'))       # CCL_ClearInfo


def ccl_init(mm):
    # The part of CCL_Init the enemy reads back: a cwp whose +0xC points
    # at a copy of the CollisionData rows.
    task, rows, n = mm.gpr(3), mm.gpr(4), mm.gpr(5)
    cwp, info = HEAP[0], HEAP[0] + 0x100
    HEAP[0] += 0x100 + 0x30 * n + 0x20
    mm.u.mem_write(cwp, bytes(0xA8))
    mm.u.mem_write(info, bytes(mm.u.mem_read(rows, 0x30 * n)))
    mm.put(cwp + 0xC, 'I', info)
    mm.put(mm.get(task + 0x34, 'I') + 0x2C, 'I', cwp)
    ev('ccl_init', rows - rel.bases[5])


m.stub(0x8000F8B8, ccl_init)
# fn_800164FC's callees other than rand and CreateElementalTask.
for a in (0x8000EDD0, 0x8008C988, 0x800ABB1C, 0x800E29BC, 0x80013944, 0x80025EFC):
    m.stub(a, lambda mm: None)

# The REL's child-task creators.
m.stub(R1(0x449CC), lambda mm: ev('child', 0x449CC))
m.stub(R1(0x43510), lambda mm: ev('child', 0x43510))
m.stub(R1(0x455BC), lambda mm: ev('child', 0x455BC))
m.stub(R1(0x45EC8), lambda mm: ev('child', 0x45EC8))
m.stub(R1(0x43E5C), lambda mm: ev('child', 0x43E5C))
m.stub(R1(0x43ED0), lambda mm: ev('child', 0x43ED0, vec(mm, 5), vec(mm, 6), mm.gpr(3), mm.gpr(4)))
m.stub(R1(0x44BA0), lambda mm: ev('child', 0x44BA0, vec(mm, 5), vec(mm, 6), mm.gpr(4)))
m.stub(R1(0x45638), lambda mm: ev('child', 0x45638, vec(mm, 4), vec(mm, 5), fbits(mm, 1)))
m.stub(R1(0x4358C), lambda mm: ev('child', 0x4358C, vec(mm, 4), vec(mm, 5)))
m.stub(R1(0x3D71C), lambda mm: ev('child', 0x3D71C, vec(mm, 3), mm.gpr(4), fbits(mm, 1), fbits(mm, 2)))
ENEMY = os.environ.get("SA2_ENEMY", "ai")
CALLBACK = {'ai': 0x51034, 'gold': 0x4A024, 'kumi': 0x4C408, 'namie': 0x52A1C}[ENEMY]
if ENEMY in ('gold', 'kumi', 'namie'):
    m.stub(R1(0x45F3C), lambda mm: ev('child', 0x45F3C, vec(mm, 5), mm.gpr(4)))
else:
    m.stub(R1(0x45F3C), lambda mm: ev('child', 0x45F3C, mm.gpr(4)))
m.stub(0x80069D60, lambda mm: ev('work_sound', mm.gpr(3), mm.gpr(5), mm.gpr(6), mm.gpr(7), vec(mm, 8)))

m.stub(0x8000F4EC, lambda mm: ev('ccl_entry'))   # CCL_Entry
m.u.mem_write(GAME, bytes(0x50))
m.put(0x803AD7E8, 'I', 0)  # the damage hook
m.put(SINGLETON, 'I', 0)

rng = random.Random(int(os.environ.get("SA2_E_AI_SEED", "11")))


def rf(lo, hi):
    return f32(rng.uniform(lo, hi))


runs = []
for run in range(int(os.environ.get("SA2_E_AI_RUNS", "80"))):
    kind = run % 10 if run < 30 else rng.randrange(-2, 12)
    g7c = rng.choice([0, 1, rng.randrange(0, 100000)])
    m.put(GAME + 0x7C, 'I', g7c)
    HEAP[0] = 0x81100000
    m.u.mem_write(TASK, bytes(0x600))
    m.u.mem_write(COUNTS, bytes(0x100))
    m.put(TASK + 0x34, 'I', TWP)
    m.put(TASK + 0x30, 'I', SETENTRY)
    pos = [rf(-500, 500), rf(-100, 100), rf(-500, 500)]
    group = rng.choice([0, 3, 7, 0xff, rng.randrange(256)])
    ang = [rng.randrange(0, 0x10000) & 0xff0f, (rng.randrange(256) << 8) | group,
           rng.choice([0, 30, 60, 90, 150, rng.randrange(0, 400)])]
    scl = [kind + rf(0, 0.99), rf(0, 6), rng.choice([0.0, rf(0, 80), rf(-20, 20)])]
    for i, x in enumerate(ang):
        m.put(TWP + 8 + 4 * i, 'i', x)
    m.u.mem_write(TWP + 0x14, struct.pack('>6f', *pos, *scl))
    for off in range(0x2DB8, 0x2DE8, 4):
        m.put(rel.addr(6, off), 'I', 0)
    g3c = rng.choice([0, 1])
    m.put(GAME + 0x3C, 'B', g3c)
    seed = rng.randrange(1 << 32)
    m.put(NEXT, 'I', seed)
    if ENEMY == 'gold':
        for i, x in enumerate([(rng.randrange(256) << 8) | rng.randrange(0, 50), ang[1], rng.randrange(0, 0x10000)]):
            m.put(TWP + 8 + 4 * i, 'i', x)
            ang[i] = x
    m.u.mem_write(PARTNER, bytes(0x40))
    m.put(PARTNER + 0x34, 'I', PARTNER_TWP)
    m.u.mem_write(PARTNER_TWP, bytes(0x10))
    m.put(PARTNER_TWP + 2, 'B', 6)
    if ENEMY == 'namie':
        kind = run % 4 if run < 40 else rng.randrange(-1, 6)
        scl[0] = f32(kind + rng.choice([0.0, rf(0, 0.99)]))
        m.u.mem_write(TWP + 0x20, struct.pack('>3f', *scl))
        a0 = rng.choice([0, rng.randrange(0, 0x10000)])
        a2 = rng.choice([0, rng.randrange(0, 0x10000), rng.randrange(0, 300)])
        m.put(TWP + 8, 'i', a0)
        m.put(TWP + 0x10, 'i', a2)
        ang[0], ang[2] = a0, a2
    if ENEMY == 'kumi':
        kind = run % 8 if run < 48 else rng.randrange(-1, 10)
        scl[0] = f32(kind + rf(0, 0.99))
        scl[1] = rng.choice([rf(0, 6), f32(rng.randrange(0, 300))])
        m.u.mem_write(TWP + 0x20, struct.pack('>3f', *scl))
    init = {'g3c': g3c, 'g7c': g7c, 'pos': [fb(x) for x in pos], 'ang': ang, 'scl': [fb(x) for x in scl], 'rand': seed}
    # Player paths.
    p0 = [pos[0] + rf(-300, 300), pos[1] + rf(-20, 20), pos[2] + rf(-300, 300)]
    v0 = [rf(-2, 2), 0.0, rf(-2, 2)]
    two = rng.random() < 0.3
    kill_at = rng.choice([None, rng.randrange(30, 500), rng.randrange(400, 650)])
    near = rng.random() < 0.7
    link40_at = rng.choice([None, None, rng.randrange(1, 600)])
    carry_at = rng.choice([None] * 5 + [rng.randrange(1, 600)])
    frames = []
    n = 680
    for f in range(n):
        p0 = [f32(p0[0] + v0[0]), p0[1], f32(p0[2] + v0[2])]
        if near and rng.random() < 0.5:
            # Stay around the enemy so it notices, turns and attacks.
            # Around home plus the home offset, where state 2 looks.
            w0 = m.get(TASK + 0x38, 'I')
            c = [m.get(w0 + 0x58 + 4 * k, 'f') + m.get(w0 + 0x70 + 4 * k, 'f') for k in range(3)] if w0 else pos
            p0 = [f32(c[0] + rf(-40, 40)), f32(pos[1] + rf(-30, 30)), f32(c[2] + rf(-40, 40))]
        if rng.random() < 0.02:
            v0 = [rf(-2, 2), 0.0, rf(-2, 2)] if not near else [rf(-0.3, 0.3), 0.0, rf(-0.3, 0.3)]
        if rng.random() < 0.01:
            p0 = [f32(pos[0] + rf(-120, 120)), p0[1], f32(pos[2] + rf(-120, 120))]
        p1 = [f32(pos[0] + rf(-150, 150)), pos[1], f32(pos[2] + rf(-150, 150))] if two else None
        pls = [p0, p1]
        for i in range(2):
            if pls[i] is None:
                m.put(PLAYERTWP + 4 * i, 'I', 0)
                continue
            m.put(PLAYERTWP + 4 * i, 'I', PLAYERS[i])
            m.put(PLAYERS[i], 'b', rng.choice([0, 0x26]) if f % 50 == 0 else m.get(PLAYERS[i], 'b'))
            m.u.mem_write(PLAYERS[i] + 0x14, struct.pack('>3f', *pls[i]))
            m.put(PLAYERS[i] + 0x2C, 'I', PCWP[i])
            m.put(PCWP[i] + 0xC, 'I', PINFO[i])
            m.u.mem_write(PINFO[i] + 8, struct.pack('>3f', pls[i][0], f32(pls[i][1] + 10), pls[i][2]))
        for i in range(2, 8):
            m.put(PLAYERTWP + 4 * i, 'I', 0)
        hit = kill_at is not None and f in (kill_at, kill_at + 9, kill_at + 20)
        cwp_bit = rng.random() < (0.5 if hit else 0.05)
        a4 = rng.choice([0, 1])
        frame_in = {
            'game37': int(rng.random() < 0.005), 'g39': int(rng.random() < 0.003), 'g42': 0,
            'range_out': int(rng.random() < 0.005), 'set_dead': 0,
            'hit_player': int(rng.random() < 0.03), 'hit_by_player': int(rng.random() < 0.5),
            'hit_index': rng.choice([0, 1, -1]), 'score_doubled': int(rng.random() < 0.5),
            'probe_y': fb(rf(-50, 50)), 'shadow_y': fb(rf(-50, 50)), 'landed': int(rng.random() < 0.3),
            'flag4': int(hit), 'spring': int(rng.random() < 0.02),
            'held_hit': int(rng.random() < 0.01), 'broken': int(rng.random() < 0.05),
            'shadow_pos': fb(rng.choice([-1e6, rf(-300, 300)])), 'cwp_bit': int(cwp_bit), 'a4': a4,
            'players': [[fb(x) for x in p] if p else None for p in pls],
            'modes': [m.get(PLAYERS[i], 'b') for i in range(2)],
        }
        m.put(GAME + 0x37, 'B', frame_in['game37'])
        m.put(GAME + 0x39, 'B', frame_in['g39'])
        m.put(GAME + 0x42, 'B', frame_in['g42'])
        m.put(SETENTRY + 2, 'H', 0x20 if frame_in['set_dead'] else 0)
        if hit:
            m.put(TWP + 4, 'H', m.get(TWP + 4, 'H') | 4)
        # Fields the game sets in the enemy work: +0x40 (kind 5 notices and
        # is held while it is set) and flag 0x400 (hits turn it into an
        # object to carry).
        work = m.get(TASK + 0x38, 'I')
        frame_in['link40'] = int(work != 0 and link40_at is not None and f >= link40_at)
        frame_in['carry'] = int(work != 0 and carry_at is not None and f == carry_at)
        if work:
            m.put(work + 0x40, 'I', 0x81000040 if frame_in['link40'] else 0)
            if frame_in['carry']:
                m.put(work + 0x4C, 'H', m.get(work + 0x4C, 'H') | 0x400)
        cwp = m.get(TWP + 0x2C, 'I')
        if cwp:
            m.put(cwp + 4, 'H', (m.get(cwp + 4, 'H') & ~1) | int(cwp_bit))
            m.put(cwp + 0x9C, 'I', PARTNER)
            m.put(cwp + 0xA4, 'I', A4WORD)
            m.put(A4WORD, 'H', a4)
        frame_in['radius_in'] = fb(m.get(m.get(cwp + 0xC, 'I') + 0xC, 'f')) if cwp else None
        events = []
        FAKE.clear()
        shown = m.get(TASK + 0x14, 'I')
        m.call(R1(CALLBACK), r3=TASK, count=20_000_000)
        if shown and not m.get(TASK + 0x14, 'I'):
            events.append(['hidden'])
        if any(e[0] == 'sound' and e[1] == 0x8015 for e in events):
            t = m.get(FAKE[-1] + 0x34, 'I')
            events.append(['animal', m.get(t + 6, 'h'), m.get(t + 1, 'b')])
        work = m.get(TASK + 0x38, 'I')
        cwp = m.get(TWP + 0x2C, 'I')
        frames.append({
            'in': frame_in,
            'twp': m.u.mem_read(TWP, 0x2C).hex(),
            'work': m.u.mem_read(work, 0x210).hex() if work else None,
            'radius': fb(m.get(m.get(cwp + 0xC, 'I') + 0xC, 'f')) if cwp else None,
            'attr0': m.get(m.get(cwp + 0xC, 'I') + 4, 'I') if cwp else None,
            'attr1': m.get(m.get(cwp + 0xC, 'I') + 0x34, 'I') if cwp else None,
            'events': events,
            'rand': m.get(NEXT, 'I'),
            'counts': m.u.mem_read(COUNTS, 0x100).hex(),
            'area': m.u.mem_read(m.get(TASK + 0x40, 'I'), 0x1C).hex() if ENEMY == 'namie' and m.get(TASK + 0x40, 'I') else None,
        })
        if any(e[0] == 'free' for e in events):
            break
    runs.append({'init': init, 'frames': frames})
print(json.dumps({'enemy': ENEMY, 'runs': runs, 'sec5': rel.bases[5]}))
