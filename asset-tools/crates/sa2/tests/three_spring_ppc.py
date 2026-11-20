"""Local PowerPC oracle: run the installed 3SPRING code under Unicorn.

Executes the module's own init, update and destructor callbacks and the
launch helper with stubbed engine imports, and prints JSON: randomized cases
with their per-update inputs and the native results.
tests/three_spring.rs replays the same inputs in Rust and compares.
Usage: python three_spring_ppc.py [STAGE_REL] (default: City Escape).
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
row, = [k for (t, k), (m, s, v, kind) in refs.items()
        if t == 5 and kind == 1 and m == own and d[sec(s) + v:].split(b'\0', 1)[0].rstrip() == b'3SPRING']
m, s, INIT, kind = refs[5, row - 4]; assert (m, s, kind) == (own, 1, 1)
LO, HI = 0x225cc, 0x22a68
B, D = 0x81000000, 0x82000000
T, W, WORK, COLLI, STACK = 0x100000, 0x100100, 0x101000, 0x101800, 0x110000
base = lambda s: B if s == 1 else D + s * 0x400000
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
mapped = set()
def page(a, n=4):
    for p in range(a & ~4095, a + n, 4096):
        if p not in mapped: u.mem_map(p, 4096); mapped.add(p)
def put(a, f, v): u.mem_write(a, struct.pack('>' + f, v))
def get(a, f): return struct.unpack('>' + f, u.mem_read(a, struct.calcsize('>' + f)))[0]
code = bytearray(d[sec(1) + LO:sec(1) + HI])
for (t, off), (mod, s, v, k) in refs.items():
    if t != 1 or not LO <= off < HI: continue
    val = base(s) + v if mod == own else v
    page(val)
    o = off - LO
    if k == 4: struct.pack_into('>H', code, o, val & 0xffff)
    elif k == 6: struct.pack_into('>H', code, o, ((val + 0x8000) >> 16) & 0xffff)
    elif k == 10:
        ins = struct.unpack_from('>I', code, o)[0]
        struct.pack_into('>I', code, o, (ins & 0xfc000003) | ((val - (B + off)) & 0x3fffffc))
    else: raise AssertionError(k)
page(B + LO, HI - LO + 4); u.mem_write(B + LO, bytes(code))
for s in (4, 5):
    page(base(s), size(s)); u.mem_write(base(s), d[sec(s):sec(s) + size(s)])
END = B + HI; put(END, 'I', 0x4e800020)
for a in (T, W, WORK, COLLI, STACK - 0x1000, STACK): page(a, 0x400)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
reg = lambda i: u.reg_read(getattr(R, f'UC_PPC_REG_{i}'))
def setreg(i, v): u.reg_write(getattr(R, f'UC_PPC_REG_{i}'), v)
def freg(i): return struct.unpack('>d', struct.pack('>Q', reg(f'FPR{i}')))[0]
f32 = lambda x: struct.unpack('>f', struct.pack('>f', x))[0]
fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
calc = lambda l: [f32(l[0] + 3.0), f32(l[1] * 2.0), f32(l[2] - 1.0)]
def s32(v): return v - (1 << 32) if v & 0x80000000 else v
def s8(v): v &= 0xff; return v - 256 if v & 0x80 else v
calls = []; env = {}
def hook(uc, pc, sz, name):
    if name == 'gate': setreg(3, int(env['clipped']))
    elif name == 'alloc':
        assert (reg(3), reg(4)) == (1, 0xc); calls.append({'call': 'allocate', 'size': reg(4)})
        u.mem_write(WORK, bytes(0xc)); setreg(3, WORK if env['alloc'] else 0)
    elif name == 'share': assert (reg(3), reg(5), reg(6)) == (T, 1, 4); calls.append({'call': 'initCollision'})
    elif name == 'free': assert reg(3) == WORK; calls.append({'call': 'freeWork'})
    elif name == 'entry': calls.append({'call': 'entryCollision'})
    elif name == 'hit': setreg(3, 0x100f00 if env['hit'] else 0)
    elif name == 'slot': setreg(3, env['slot'] & 0xffffffff)
    elif name == 'getpos':
        assert (reg(4), reg(6)) == (0, 0); calls.append({'call': 'playerPosition', 'slot': s8(reg(3))})
        for i, x in enumerate(env['player']): put(reg(5) + 4 * i, 'f', x)
    elif name == 'push': calls.append({'call': 'pushMatrix'})
    elif name == 'unit': assert reg(3) == 0; calls.append({'call': 'unitMatrix'})
    elif name == 'translate': calls.append({'call': 'translate', 'position': [get(reg(3) + 4 * i, 'I') for i in range(3)]})
    elif name in ('rz', 'rx', 'ry'):
        assert reg(3) == 0; calls.append({'call': 'rotate', 'import': {'rz': 0x80115560, 'rx': 0x80115918, 'ry': 0x801154e0}[name], 'angle': s32(reg(4))})
    elif name == 'invert': assert reg(3) == 0; calls.append({'call': 'invertMatrix'})
    elif name == 'calc':
        assert reg(3) == 0 and reg(4) == reg(5)
        l = [get(reg(4) + 4 * i, 'f') for i in range(3)]
        calls.append({'call': 'calcPoint', 'local': [fb(x) for x in l]})
        for i, x in enumerate(calc(l)): put(reg(5) + 4 * i, 'f', x)
    elif name == 'pop': calls.append({'call': 'popMatrix'})
    elif name == 'setpos': calls.append({'call': 'setPosition', 'slot': s8(reg(3)), 'position': [fb(freg(i)) for i in (1, 2, 3)]})
    elif name == 'setvel':
        calls.append({'call': 'setVelocity', 'slot': s8(reg(3)), 'velocity': [get(reg(4) + 4 * i, 'I') for i in range(3)],
                      'angles': [get(reg(5) + 4 * i, 'i') for i in range(3)], 'frames': s32(reg(6))})
    elif name == 'sound': assert (reg(4), reg(5), reg(6)) == (0, 0, 0); calls.append({'call': 'sound', 'code': reg(3)})
    elif name == 'rumble': calls.append({'call': 'rumble', 'slot': s8(reg(3)), 'args': [reg(4), reg(5), reg(6)]})
for addr, name in [(0x8006538c, 'gate'), (0x801194b4, 'alloc'), (0x8000f8b8, 'share'), (0x801194e0, 'free'),
                   (0x8000f4ec, 'entry'), (0x800066f4, 'hit'), (0x80037c74, 'slot'), (0x80037da0, 'getpos'),
                   (0x80115d50, 'push'), (0x8011509c, 'unit'), (0x80115e24, 'translate'), (0x80115560, 'rz'),
                   (0x80115918, 'rx'), (0x801154e0, 'ry'), (0x80114cb0, 'invert'), (0x80114bc4, 'calc'),
                   (0x80115d9c, 'pop'), (0x80039754, 'setpos'), (0x80039ab8, 'setvel'), (0x8006b710, 'sound'),
                   (0x8002f8c4, 'rumble')]:
    page(addr); put(addr, 'I', 0x4e800020); u.hook_add(UC_HOOK_CODE, hook, begin=addr, end=addr, user_data=name)
def run(pc):
    setreg(1, STACK); setreg(3, T); u.reg_write(R.UC_PPC_REG_LR, END)
    u.emu_start(pc, END, count=5000); assert u.reg_read(R.UC_PPC_REG_PC) == END
def state():
    work = get(T + 0x38, 'I')
    return {'work': None if not work else {'bounce': [get(WORK, 'I'), get(WORK + 4, 'I')], 'cooldown': [get(WORK + 8, 'B'), get(WORK + 9, 'B')]},
            'colliFlags': get(COLLI + 4, 'H')}
rng = random.Random(3)
cases = []
for case in range(80):
    scale = [rng.choice([-2.0, 0.0, 1.0, 3.5, 6.25]), 0.0, 0.0]
    angles = [rng.choice([0, 0x4000, -0x4000, 0x8000, 0x7fff8000 - 1, rng.randrange(-(1 << 31), 1 << 31)]) for _ in range(3)]
    pos = [f32(rng.uniform(-9000, 9000)) for _ in range(3)]
    u.mem_write(T, bytes(0x100)); u.mem_write(W, bytes(0x40)); u.mem_write(COLLI, bytes(0x10))
    put(T + 0x34, 'I', W); put(W + 0x2c, 'I', COLLI)
    for i, a in enumerate(angles): put(W + 8 + 4 * i, 'i', a)
    for i, x in enumerate(pos): put(W + 0x14 + 4 * i, 'f', x)
    for i, x in enumerate(scale): put(W + 0x20 + 4 * i, 'f', x)
    runs, native = [], []
    for tick in range(40):
        env = {'clipped': rng.random() < 0.1, 'alloc': rng.random() < 0.8, 'hit': rng.random() < 0.4,
               'slot': rng.choice([-1, 0, 0, 1, 1]), 'player': [f32(rng.uniform(-9000, 9000)) for _ in range(3)]}
        calls = []
        main = get(T + 0x10, 'I')
        run(main if main else B + INIT)
        runs.append({**env, 'player': [fb(x) for x in env['player']]})
        native.append({'calls': calls, **state()})
    calls = []
    if get(T + 0x18, 'I'): run(get(T + 0x18, 'I'))
    native.append({'calls': calls, **state()})
    cases.append({'scale': [fb(x) for x in scale], 'angles': angles, 'position': [fb(x) for x in pos], 'runs': runs, 'native': native})
print(json.dumps({'init': INIT, 'cases': cases}))
