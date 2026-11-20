"""Local PowerPC oracle: run the installed RING_LINEAR / RING_CIRCLE code under Unicorn.

Executes the module's own init, update and destructor callbacks (and the RING
link/unlink helpers they call) with stubbed engine imports, and prints JSON:
randomized cases with their per-run inputs and the native results.
tests/ring_group.rs replays the same inputs in Rust and compares.
Usage: python ring_group_ppc.py [STAGE_REL] (default: City Escape).
"""
import json, os, random, struct, sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

rel = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(os.environ["SA2_GAME_PATH"]) / "resource/gd_PC/stg13D.rel"
d = rel.read_bytes()
w = lambda a: struct.unpack_from('>I', d, a)[0]
sec = lambda i: w(w(16) + 8 * i) & ~1
size = lambda i: w(w(16) + 8 * i + 4)
own = w(0)
refs = {}
for mod, at in struct.iter_unpack('>II', d[w(40):w(40) + w(44)]):
    off = 0
    while True:
        inc, kind, s, v = struct.unpack_from('>HBBI', d, at); at += 8
        if kind == 203: break
        if kind == 202: section = s; off = 0; continue
        off += inc; refs[section, off] = (mod, s, v, kind)
def init_of(name):
    row, = [k for (t, k), (m, s, v, kind) in refs.items()
            if t == 5 and kind == 1 and m == own and d[sec(s) + v:].split(b'\0', 1)[0].rstrip() == name]
    m, s, v, kind = refs[5, row - 4]; assert (m, s, kind) == (own, 1, 1); return v
LINEAR, CIRCLE = init_of(b'RING_LINEAR'), init_of(b'RING_CIRCLE')
RANGES = [(0x27ca8, 0x285f8), (0x3dfdc, 0x3e038)]
B, D = 0x81000000, 0x82000000
T, W, WORK, CHILD, STACK = 0x100000, 0x100100, 0x101000, 0x102000, 0x110000
base = lambda s: B if s == 1 else D + s * 0x400000
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
mapped = set()
def page(a, n=4):
    for p in range(a & ~4095, a + n, 4096):
        if p not in mapped: u.mem_map(p, 4096); mapped.add(p)
def put(a, f, v): u.mem_write(a, struct.pack('>' + f, v))
def get(a, f): return struct.unpack('>' + f, u.mem_read(a, struct.calcsize('>' + f)))[0]
for lo, hi in RANGES:
    code = bytearray(d[sec(1) + lo:sec(1) + hi])
    for (t, off), (mod, s, v, k) in refs.items():
        if t != 1 or not lo <= off < hi: continue
        val = base(s) + v if mod == own else v
        page(val)
        o = off - lo
        if k == 4: struct.pack_into('>H', code, o, val & 0xffff)
        elif k == 6: struct.pack_into('>H', code, o, ((val + 0x8000) >> 16) & 0xffff)
        elif k == 10:
            ins = struct.unpack_from('>I', code, o)[0]
            struct.pack_into('>I', code, o, (ins & 0xfc000003) | ((val - (B + off)) & 0x3fffffc))
        else: raise AssertionError(k)
    page(B + lo, hi - lo + 4); u.mem_write(B + lo, bytes(code))
for s in (4, 5):
    page(base(s), size(s)); u.mem_write(base(s), d[sec(s):sec(s) + size(s)])
END = B + 0x285f8; put(END, 'I', 0x4e800020)
for a in (T, W, WORK, STACK - 0x1000, STACK): page(a, 0x400)
page(CHILD, 0x4000)
# MWCC register save/restore helpers (_savegpr_14.. / _restgpr_14..).
SAVE, REST = 0x8010ac50, 0x8010ac9c
page(SAVE, 0x100)
for n in range(14, 32):
    put(SAVE + 4 * (n - 14), 'I', (36 << 26) | (n << 21) | (11 << 16) | ((-(32 - n) * 4) & 0xffff))
    put(REST + 4 * (n - 14), 'I', (32 << 26) | (n << 21) | (11 << 16) | ((-(32 - n) * 4) & 0xffff))
put(SAVE + 72, 'I', 0x4e800020); put(REST + 72, 'I', 0x4e800020)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
reg = lambda i: u.reg_read(getattr(R, f'UC_PPC_REG_{i}'))
def setreg(i, v): u.reg_write(getattr(R, f'UC_PPC_REG_{i}'), v)
f32 = lambda x: struct.unpack('>f', struct.pack('>f', x))[0]
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
def fpr1(v): setreg('FPR1', struct.unpack('>Q', struct.pack('>d', v))[0])
sine = lambda a: (a % 4096) / 4096.0 - 0.5
cosine = lambda a: ((a + 1000) % 2048) / 2048.0
calc = lambda l: [f32(l[0] + 1.5), f32(l[1] * 2.0), f32(l[2] - 0.25)]
calls = []; env = {}
def s32(v): return v - (1 << 32) if v & 0x80000000 else v
def hook(uc, pc, sz, name):
    if name == 'gate': setreg(3, int(env['clipped'].pop(0)))
    elif name == 'alloc':
        assert (reg(3), reg(4)) == (1, 0xa0); calls.append({'call': 'allocate', 'size': reg(4)})
        ok = env['alloc']; u.mem_write(WORK, bytes(0xa0)); setreg(3, WORK if ok else 0)
    elif name == 'setread': calls.append({'call': 'readSetState'}); setreg(3, env['state'] & 0xff)
    elif name == 'push': calls.append({'call': 'pushMatrix'})
    elif name == 'unit': assert reg(3) == 0; calls.append({'call': 'unitMatrix'})
    elif name == 'translate': calls.append({'call': 'translate', 'position': [get(reg(3) + 4 * i, 'I') for i in range(3)]})
    elif name in ('rz', 'ry', 'rx'):
        assert reg(3) == 0; calls.append({'call': 'rotate', 'import': {'rz': 0x80115560, 'ry': 0x801154e0, 'rx': 0x80115918}[name], 'angle': s32(reg(4))})
    elif name == 'sin': calls.append({'call': 'sine', 'angle': s32(reg(3))}); fpr1(sine(s32(reg(3))))
    elif name == 'cos': calls.append({'call': 'cosine', 'angle': s32(reg(3))}); fpr1(cosine(s32(reg(3))))
    elif name == 'calc':
        assert reg(3) == 0
        l = [get(reg(4) + 4 * i, 'f') for i in range(3)]
        calls.append({'call': 'calcPoint', 'local': [fb(x) for x in l]})
        for i, x in enumerate(calc(l)): put(reg(5) + 4 * i, 'f', x)
    elif name == 'pop': calls.append({'call': 'popMatrix'})
    elif name == 'spawn':
        assert (reg(3), reg(4), reg(5)) == (2, 1, B + 0x3e174)
        ok = env['spawn'].pop(0)
        if ok:
            c = CHILD + 0x100 * env['children']; env['children'] += 1
            u.mem_write(c, bytes(0x100)); put(c + 0x34, 'I', c + 0x80); setreg(3, c)
        else: setreg(3, 0)
        calls.append({'call': 'spawnRing'})
    elif name == 'mark': calls.append({'call': 'markSetEntryDone'})
    elif name == 'free': calls.append({'call': 'freeTask', 'self': reg(3) == T})
    elif name == 'save': calls.append({'call': 'saveSetState', 'mask': s32(reg(4))})
    elif name == 'freework': assert reg(3) == WORK; calls.append({'call': 'freeWork'})
for addr, name in [(0x8006538c, 'gate'), (0x801194b4, 'alloc'), (0x80065120, 'setread'), (0x80115d50, 'push'),
                   (0x8011509c, 'unit'), (0x80115e24, 'translate'), (0x80115560, 'rz'), (0x801154e0, 'ry'),
                   (0x80115918, 'rx'), (0x80112050, 'sin'), (0x80112068, 'cos'), (0x80114bc4, 'calc'),
                   (0x80115d9c, 'pop'), (0x80071ba4, 'spawn'), (0x80065148, 'mark'), (0x80071878, 'free'),
                   (0x80065134, 'save'), (0x801194e0, 'freework')]:
    page(addr); put(addr, 'I', 0x4e800020); u.hook_add(UC_HOOK_CODE, hook, begin=addr, end=addr, user_data=name)
def run(pc):
    setreg(1, STACK); setreg(3, T); u.reg_write(R.UC_PPC_REG_LR, END)
    u.emu_start(pc, END, count=20000); assert u.reg_read(R.UC_PPC_REG_PC) == END
def state():
    work = get(T + 0x38, 'I')
    ents = None if work == 0 else [[get(WORK + 8 * i, 'I'), get(WORK + 8 * i + 4, 'I') != 0] for i in range(8)]
    pos = None if work == 0 else [[get(WORK + 0x40 + 12 * i + 4 * k, 'I') for k in range(3)] for i in range(8)]
    return {'count': get(W + 3, 'B'), 'entries': ents, 'positions': pos}
rng = random.Random(7)
cases = []
for case in range(96):
    kind = rng.choice(['linear', 'circle'])
    scale = [rng.choice([-10.0, 0.0, 5.0, 20.0, 30.0]), rng.choice([-1.0, -0.5, 0.0, 0.25, 6.0, 60.0]), rng.choice([-3.0, 0.0, 0.9, 1.0, 3.0, 5.0, 7.5, 8.0, 12.0])]
    angles = [rng.choice([0, 0x4000, 0x8000, 0xC000, 0x12345678, rng.randrange(1 << 32)]) for _ in range(3)]
    pos = [f32(rng.uniform(-9000, 9000)) for _ in range(3)]
    has_set = rng.random() < 0.6
    env = {'clipped': [], 'spawn': [], 'children': 0, 'alloc': rng.random() < 0.9, 'state': rng.randrange(256)}
    u.mem_write(T, bytes(0x100)); u.mem_write(W, bytes(0x40)); u.mem_write(WORK, bytes(0xa0))
    put(T + 0x34, 'I', W); put(T + 0x30, 'I', 0x100f00 if has_set else 0)
    for i, a in enumerate(angles): put(W + 8 + 4 * i, 'I', a)
    for i, x in enumerate(pos): put(W + 0x14 + 4 * i, 'f', x)
    for i, x in enumerate(scale): put(W + 0x20 + 4 * i, 'f', x)
    runs, native = [], []
    for tick in range(14):
        r = {'clipped': rng.random() < 0.2, 'spawn': [rng.random() < 0.85 for _ in range(8)], 'collect': None}
        if tick > 1 and get(T + 0x38, 'I') and rng.random() < 0.6:
            with_child = [i for i in range(8) if get(WORK + 8 * i + 4, 'I')]
            if with_child:
                i = rng.choice(with_child); r['collect'] = i
                put(WORK + 8 * i, 'I', get(WORK + 8 * i, 'I') | 2); put(WORK + 8 * i + 4, 'I', 0)
        env['clipped'] = [r['clipped']]; env['spawn'] = list(r['spawn']); calls = []
        main = get(T + 0x10, 'I')
        run(main if main else B + (LINEAR if kind == 'linear' else CIRCLE))
        r['spawnUsed'] = 8 - len(env['spawn']); r['clippedUsed'] = not env['clipped']
        runs.append(r); native.append({'calls': calls, **state()})
        if any(c['call'] in ('markSetEntryDone', 'freeTask') for c in calls): break
    calls = []
    dest = get(T + 0x18, 'I')
    if dest: run(dest)
    native.append({'calls': calls, **state()})
    cases.append({'kind': kind, 'scale': [fb(x) for x in scale], 'angles': angles, 'position': [fb(x) for x in pos],
                  'hasSet': has_set, 'alloc': env['alloc'], 'state': env['state'], 'runs': runs, 'native': native})
print(json.dumps({'linear': LINEAR, 'circle': CIRCLE, 'cases': cases}))
