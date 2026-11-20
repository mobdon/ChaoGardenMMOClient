"""Run stg10D.rel's (Metal Harbor) stage program and stage task under the
main.dol harness and print JSON cases. tests/metal_harbor.rs replays each
case with `metal_harbor` and compares the engine calls and every location the
routine writes, bit for bit.

Needs SA2_MAIN_DOL (GSNE8P main.dol) and SA2_GC_REL_DIR (the GameCube disc
stage RELs). Without them it prints nothing and exits 0 (the test skips).
Every main.dol routine the REL calls is stubbed and recorded with its
arguments; REL-local helpers outside the routine under test are stubbed and
recorded the same way. The game is never started.
"""
import hashlib, json, os, random, struct, sys
from pathlib import Path

if not os.environ.get("SA2_MAIN_DOL") or not os.environ.get("SA2_GC_REL_DIR"):
    print("SA2_MAIN_DOL / SA2_GC_REL_DIR not set", file=sys.stderr)
    sys.exit(0)
sys.path.insert(0, str(Path(__file__).parent))
from unicorn import UC_HOOK_CODE  # noqa: E402
from unicorn import ppc_const as R  # noqa: E402
from dol_ppc import Dol  # noqa: E402
from stage_rel_ppc import load_rel  # noqa: E402

REL_PATH = Path(os.environ["SA2_GC_REL_DIR"]) / "stg10D.rel"
GC_SHA256 = "5de85d7f52d6f01416bcd7023c2b3608769f4478daaccdb4e4506be1402b19ac"
assert hashlib.sha256(REL_PATH.read_bytes()).hexdigest() == GC_SHA256
m = Dol(os.environ["SA2_MAIN_DOL"]).machine(paired={}, boot=False)
rel = load_rel(m, REL_PATH)
S = lambda sec, off: rel.addr(sec, off)
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))
rng = random.Random(10)

SCR = 0x81400000
TASK, WORK, PARENT = SCR, SCR + 0x100, SCR + 0x200
FRAME_TASK, FRAME_WORK, CAM = SCR + 0x300, SCR + 0x400, SCR + 0x500
PWP, TWP, MWP = [SCR + 0x1000, SCR + 0x1400], [SCR + 0x1800, SCR + 0x1900], [SCR + 0x1A00, SCR + 0x1B00]
SLOTS, TRAIL, G = 0x801A2A18, 0x801E77D8, 0x801CC168
TEXOV, CAM_INDEX, CAM_TABLE = 0x801E5D28, 0x803ADAD0, 0x80175378
PLAYERPWP, PLAYERMWP, PLAYERTWP = 0x801E7728, 0x801E7748, 0x801E7768
NAMES = {TASK: 'task', WORK: 'work', PARENT: 'parent'}
for k in range(2):
    NAMES.update({PWP[k]: f'pwp{k}', PWP[k] + 0x174: f'pwp{k}+0x174', TWP[k]: f'twp{k}', MWP[k]: f'mwp{k}'})
SECTIONS = {}
n, info = struct.unpack_from('>II', rel.data, 0xC)
for s in range(n):
    off, size = struct.unpack_from('>II', rel.data, info + 8 * s)
    if size and s in rel.bases:
        SECTIONS[s] = (rel.bases[s], size)
if 6 not in SECTIONS:  # the .bss size comes from the header
    SECTIONS[6] = (rel.bases[6], struct.unpack_from('>I', rel.data, 0x20)[0])


def name(v):
    if v in NAMES:
        return NAMES[v]
    for s, (b, z) in SECTIONS.items():
        if b <= v < b + z:
            return f's{s}+0x{v - b:x}'
    return v


ENGINE = {
    0x800324E4: 'r3 r4', 0x80072270: 'r3 r4', 0x80072218: 'r3 r4', 0x80065048: 'r3 r4',
    0x8006638C: 'r3 r4', 0x8001A8A8: '', 0x80033710: '', 0x800C3CD8: 'f1 f2 f3', 0x8011E154: 'f1 f2',
    0x8011E150: 'f1 f2 f3', 0x8002506C: 'r3 r4', 0x80025028: 'r3 r4', 0x80025048: 'r3',
    0x8002503C: 'r3', 0x80011A04: 'r3', 0x80011050: 'r3', 0x8005D0EC: 'r3', 0x80071E0C: 'r3 r4 r5 r6',
    0x8003BCC4: 'r3', 0x8011C2BC: '', 0x8011C310: '', 0x8006D500: 'r3 r4', 0x800336DC: '',
    0x80072118: 'r3 r4', 0x80066364: '', 0x8001A2B0: 'r3', 0x800245B8: '', 0x80012218: 'r3',
    0x80116098: '', 0x8011538C: 'r3 v4', 0x80116E18: 'r3', 0x8011DF44: 'r3', 0x80115DC8: 'r3',
    0x8011C2B0: '', 0x8003D7D4: 'r3', 0x8005523C: 'r3 r4 r5', 0x800709F8: 'r3 r4 r5',
}
MODULE = {0xB500: 'r3', 0xA3B0: '', 0x106A8: 'r3 r4 r5 r6', 0x10C98: '', 0x15298: 'r3 r4', 0x326A0: 'r3',
          0x3261C: '', 0x1582C: '', 0x10D34: '', 0xA490: '', 0x158E0: '', 0xAB7C: 'r3', 0x13DC: 'f1 f2',
          0x158B4: 'r3', 0x1594: ''}
calls, returns, stubbed = [], {}, set()


def args(spec):
    out = []
    for a in spec.split():
        if a[0] == 'r':
            out.append(name(m.gpr(int(a[1:]))))
        elif a[0] == 'f':
            out.append(fb(m.fpr(int(a[1:]))))
        else:  # v4: the three floats r4 points at
            out.append([m.get(m.gpr(4) + 4 * k, 'I') for k in range(3)])
    return out


def hook(addr, record):
    def h(uc, pc, size, _):
        if record():
            uc.reg_write(R.UC_PPC_REG_PC, uc.reg_read(R.UC_PPC_REG_LR))
    m.u.hook_add(UC_HOOK_CODE, h, begin=addr, end=addr)


def engine(addr, spec):
    def record():
        calls.append({'engine': f'0x{addr:08x}', 'args': args(spec)})
        if addr in returns:
            m.gpr(3, returns[addr])
        return True
    hook(addr, record)


def module(off, spec):
    def record():
        if off not in stubbed:
            return False
        calls.append({'module': f's1+0x{off:x}', 'args': args(spec)})
        return True
    hook(S(1, off), record)


# Every main.dol routine the REL branches to must be one of ENGINE (audits the
# GSNE8P addresses the port reports).
imports = set()
imp, impsize = struct.unpack_from('>II', rel.data, 0x28)
for k in range(impsize // 8):
    mod, at = struct.unpack_from('>II', rel.data, imp + 8 * k)
    if mod:
        continue
    sec = 0
    while True:
        o, t, s, a = struct.unpack_from('>HBBI', rel.data, at)
        at += 8
        if t == 203:
            break
        if t == 202:
            sec = s
        elif t == 10 and sec == 1:
            imports.add(a)
for a, spec in ENGINE.items():
    assert a in imports, hex(a)
    engine(a, spec)
for off, spec in MODULE.items():
    module(off, spec)


def run(off, only_stub, **regs):
    calls.clear()
    stubbed.clear()
    stubbed.update(set(MODULE) - {off} if only_stub is None else only_stub)
    m.call(S(1, off), **regs)
    return list(calls)


def clear(a, n):
    m.u.mem_write(a, bytes(n))


def rnd_word():
    return rng.choice([0, 1, 0x80001234, rng.randrange(1 << 32)])


def rnd_f(lo=-5000.0, hi=5000.0):
    if rng.random() < 0.1:
        return fb(rng.choice([0.0, -0.0, 1.0, 1300.0, -1300.0, 1e30, -1e30]))
    return fb(f32(rng.uniform(lo, hi)))


cases = []
which = sys.argv[1] if len(sys.argv) > 1 else 'all'

# --- setup (+0xA66C) ---------------------------------------------------------
if which in ('all', 'setup'):
    ours = [S(1, 0x37EF8), S(1, 0x14880), S(1, 0x10D5C)]
    for i in range(48):
        handle = rng.choice([0, 0x80123450, rnd_word()])
        task = i % 4 != 1
        slots = [rng.choice([0, 0, rnd_word()]) for _ in range(3)]
        trail = [rnd_word() for _ in range(14)]
        clear(TASK, 0x40)
        m.put(TASK + 0x34, 'I', WORK)
        for k, o in enumerate((0, 8, 0x24)):
            m.put(SLOTS + o, 'I', slots[k])
        for k in range(14):
            m.put(TRAIL + 4 * k, 'I', trail[k])
        m.put(S(6, 0xD8), 'I', rng.randrange(1 << 32))
        returns.clear()
        returns.update({0x80065048: handle, 0x80071E0C: TASK if task else 0})
        c = run(0xA66C, None)
        cases.append({'fn': 'setup', 'input': {'handle': handle, 'task': task, 'slots': slots, 'trail': trail},
                      'calls': c, 'state': {
                          'handle': m.get(S(6, 0xD8), 'I'),
                          'slots': [name(m.get(SLOTS + o, 'I')) for o in (0, 8, 0x24)],
                          'trail': [name(m.get(TRAIL + 4 * k, 'I')) for k in range(14)],
                          'task': [name(m.get(TASK + o, 'I')) for o in (0x14, 0x18, 0x20)] if task else None}})

# --- teardown (+0xAA5C) ------------------------------------------------------
if which in ('all', 'teardown'):
    ours = [S(1, 0x37EF8), S(1, 0x14880), S(1, 0x10D5C)]
    for i in range(48):
        slots = [rng.choice(['ours', 0, rnd_word()]) for _ in range(3)]
        handle = rng.choice([0, rnd_word()])
        for k, o in enumerate((0, 8, 0x24)):
            m.put(SLOTS + o, 'I', ours[k] if slots[k] == 'ours' else slots[k])
        m.put(S(6, 0xD8), 'I', handle)
        returns.clear()
        c = run(0xAA5C, None)
        cases.append({'fn': 'teardown', 'input': {'slots': slots, 'handle': handle}, 'calls': c, 'state': {
            'handle': m.get(S(6, 0xD8), 'I'), 'slots': [name(m.get(SLOTS + o, 'I')) for o in (0, 8, 0x24)]}})

# --- stage program exec (+0xACD4) and its speed probe (+0xAB7C) --------------
if which in ('all', 'exec'):
    for mode in (-1, 0, 0, 1, 2, 5, 127, -128):
        clear(WORK, 0x10)
        m.put(TASK + 0x34, 'I', WORK)
        m.put(WORK, 'b', mode)
        c = run(0xACD4, None, r3=TASK)
        cases.append({'fn': 'exec', 'input': {'mode': mode}, 'calls': c, 'state': {}})
    # +0xAB7C reads player 0's speed and computes a value it never stores
    # outside its own frame: it has no effect, which this checks.
    watch = [(S(s, 0), z) for s, (b, z) in SECTIONS.items()] + [(PWP[0], 0x400), (TASK, 0x40), (WORK, 0x10)]
    for i in range(40):
        null = i % 8 == 3
        m.put(PLAYERPWP, 'I', 0 if null else PWP[0])
        m.put(PWP[0] + 0x64, 'I', rnd_f(-2.0, 9.0))
        before = [bytes(m.u.mem_read(a, z)) for a, z in watch]
        c = run(0xAB7C, set(), r3=TASK)
        assert c == [] and before == [bytes(m.u.mem_read(a, z)) for a, z in watch]
    cases.append({'fn': 'speed_probe', 'input': {'runs': 40}, 'calls': [], 'state': {}})

# --- stage task exec (+0xAD28) -----------------------------------------------
if which in ('all', 'task_exec'):
    for i in range(160):
        frame = None if i % 7 == 0 else {
            'mode': rng.choice([0, 1, 2, 3, 3, 4, 5, 5, -3, rng.randrange(-128, 128)]),
            'timer': rng.choice([0, 239, 240, 241, 0xFFFF, rng.randrange(1 << 16)])}
        flags = rng.choice([0, 1, 2, 4, 6, 7, -1, rng.randrange(-128, 128)])
        clock = rng.choice([0, 9, 10, 11, 0xFFFFFFFF, rng.randrange(1 << 32)])
        fog = rnd_f()
        clear(WORK, 0x10)
        m.put(TASK + 0x34, 'I', WORK)
        m.put(WORK + 1, 'b', flags)
        if frame is None:
            m.put(S(6, 0xB8), 'I', 0)
        else:
            m.put(S(6, 0xB8), 'I', FRAME_TASK)
            m.put(FRAME_TASK + 0x34, 'I', FRAME_WORK)
            m.put(FRAME_WORK, 'b', frame['mode'])
            m.put(FRAME_WORK + 6, 'H', frame['timer'])
        m.put(G + 0x7C, 'I', clock)
        m.put(S(5, 0x117C), 'I', fog)
        c = run(0xAD28, None, r3=TASK)
        cases.append({'fn': 'task_exec', 'input': {'frame': frame, 'flags': flags, 'clock': clock, 'fog': fog},
                      'calls': c, 'state': {'flags': m.get(WORK + 1, 'b'), 'fog': m.get(S(5, 0x117C), 'I')}})

# --- stage task displays (+0xAEA8, +0xB02C) ----------------------------------
if which in ('all', 'display'):
    m.put(CAM_INDEX, 'I', 0)
    m.put(CAM_TABLE, 'I', CAM)
    for fn, off in (('display', 0xAEA8), ('display2', 0xB02C)):
        for i in range(120):
            cam = [rnd_f(), rng.choice([rnd_f(), fb(1299.0), fb(1300.0), fb(1301.0)]), rnd_f()]
            flags = rng.choice([0, 1, 2, 3, 4, 5, 6, 7, -1, rng.randrange(-128, 128)])
            saved = [rnd_f() for _ in range(3)]
            for k in range(3):
                m.put(CAM + 4 * k, 'I', cam[k])
                m.put(S(6, 0xDC) + 4 * k, 'I', saved[k])
            clear(WORK, 0x10)
            m.put(TASK + 0x34, 'I', WORK)
            m.put(WORK + 1, 'b', flags)
            c = run(off, None, r3=TASK)
            cases.append({'fn': fn, 'input': {'cam': cam, 'flags': flags, 'saved': saved}, 'calls': c, 'state': {
                'flags': m.get(WORK + 1, 'b'), 'saved': [m.get(S(6, 0xDC) + 4 * k, 'I') for k in range(3)]}})

# --- drifting entries (+0x13DC) ----------------------------------------------
if which in ('all', 'drift'):
    for i in range(200):
        count = rng.choice([-1, 0, 1, 2, 3, 4, 5, 5, 5])
        rate = rng.choice([fb(0.07), fb(0.005), rnd_f(0.0, 0.2), fb(0.0)])
        entries = []
        for _ in range(5):
            start = rng.randrange(-500, 500)
            entries.append([rnd_f(), rnd_f(), rnd_word(), rnd_word(),
                            (start + rng.choice([0, 1, 300, 454, 455, 456, rng.randrange(0, 2000)])) & 0xFFFFFFFF,
                            start & 0xFFFFFFFF])
        d = [rnd_f(-10.0, 10.0), rnd_f(-10.0, 10.0)] if i % 3 else [fb(0.0), fb(0.0)]
        for k, e in enumerate(entries):
            for j, v in enumerate(e):
                m.put(S(6, 0) + 24 * k + 4 * j, 'I', v)
        m.put(S(6, 0x78), 'i', count)
        m.put(S(5, 0x1170), 'I', rate)
        c = run(0x13DC, None, f1=bf(d[0]), f2=bf(d[1]))
        cases.append({'fn': 'drift', 'input': {'count': count, 'rate': rate, 'entries': entries, 'delta': d},
                      'calls': c, 'state': {'count': m.get(S(6, 0x78), 'i'), 'entries': [
                          [m.get(S(6, 0) + 24 * k + 4 * j, 'I') for j in range(6)] for k in range(5)]}})

# --- texture overrides: install (+0xA3B0) and restore (+0xA490) --------------
if which in ('all', 'overrides'):
    table, at = [], S(5, 0x154404)
    while m.get(at, 'h') != -1:
        table.append((m.get(at, 'h'), m.get(at + 4, 'I')))
        at += 8
    ids = sorted({i for i, _ in table if 0 <= i < 300})

    def put_e(e):
        for i in ids:
            v = e[str(i)]
            m.put(TEXOV + 8 * i + 2, 'h', v[0])
            m.put(TEXOV + 8 * i + 4, 'I', v[1])

    def get_e():
        return {str(i): [m.get(TEXOV + 8 * i + 2, 'h'), name(m.get(TEXOV + 8 * i + 4, 'I'))] for i in ids}

    ptrs = [p for _, p in table]
    for i in range(60):
        e = {str(k): [rng.randrange(-0x8000, 0x8000), rng.choice([0, 0, rnd_word()])] for k in ids}
        put_e(e)
        c = run(0xA3B0, set())
        cases.append({'fn': 'install', 'input': {'e': e}, 'calls': c, 'state': {'e': get_e()}})
    for i in range(80):
        e = {}
        for k in ids:
            own = [p for j, p in table if j == k]
            e[str(k)] = [rng.randrange(-0x8000, 0x8000), rng.choice([0, rnd_word()] + own * 2)]
        put_e(e)
        players = []
        for p in range(2):
            if rng.random() < 0.2:
                players.append(None)
                m.put(PLAYERPWP + 4 * p, 'I', 0)
                continue
            players.append({'index': p if rng.random() < 0.8 else 1 - p, 'flags': rng.randrange(-0x8000, 0x8000)})
            m.put(PLAYERPWP + 4 * p, 'I', PWP[p])
            m.put(PLAYERTWP + 4 * p, 'I', TWP[p])
            m.put(PLAYERMWP + 4 * p, 'I', MWP[p])
        for p in range(2):  # a player's index must name a present player
            if players[p] and players[players[p]['index']] is None:
                players[p]['index'] = p
        for p in range(2):
            if players[p]:
                m.put(PWP[p], 'b', players[p]['index'])
                m.put(PWP[p] + 0x180, 'h', players[p]['flags'])
        c = run(0xA490, set())
        e_in = {k: [v[0], name(v[1])] for k, v in e.items()}
        cases.append({'fn': 'restore', 'input': {'e': e_in, 'players': players}, 'calls': c, 'state': {
            'e': get_e(), 'flags': [m.get(PWP[p] + 0x180, 'h') if players[p] else None for p in range(2)]}})

# --- follower task spawn (+0x106A8) ------------------------------------------
if which in ('all', 'spawn'):
    for i in range(120):
        player = rng.choice([0, 1, 0, 1, 2, 3, 0x7FFFFFFF])
        a, b = rnd_word(), rnd_word()
        parent = rng.random() < 0.5
        twp = rng.random() < 0.85
        created = rng.random() < 0.85
        pwp = [rnd_f() for _ in range(6)]
        for p in range(2):
            m.put(PLAYERTWP + 4 * p, 'I', TWP[p] if twp else 0)
            m.put(PLAYERPWP + 4 * p, 'I', PWP[p])
            for k in range(6):
                m.put(PWP[p] + 0x200 + 4 * k, 'I', pwp[k])
        clear(TASK, 0x40)
        clear(WORK, 0x40)
        m.put(TASK + 0x34, 'I', WORK)
        returns.clear()
        returns.update({0x80071E0C: TASK if created else 0, 0x800709F8: TASK if created else 0})
        c = run(0x106A8, set(), r3=player, r4=a, r5=b, r6=PARENT if parent else 0)
        cases.append({'fn': 'spawn', 'input': {'player': player, 'a': a, 'b': b, 'parent': parent, 'twp': twp,
                                               'created': created, 'pwp': pwp}, 'calls': c, 'state': {
            'ret': name(m.gpr(3)),
            'task': [name(m.get(TASK + o, 'I')) for o in (0x10, 0x14, 0x18)],
            'work': [m.get(WORK + 4 * k, 'I') for k in range(11)]}})

print(json.dumps({'cases': cases}))
