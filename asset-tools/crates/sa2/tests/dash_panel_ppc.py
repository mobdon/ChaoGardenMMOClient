"""Local PowerPC oracle: run the installed KASOKU update callback under Unicorn.

Prints JSON: randomized cases, each a list of per-update engine observations
and the native result of every update (engine calls with arguments, then the
task state). tests/dash_panel.rs replays the same inputs in Rust and compares.
Usage: python dash_panel_ppc.py [STAGE_REL] (default: City Escape).
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
# The object-list row whose name is KASOKU; its init pointer is the word before.
row, = [k for (t, k), (m, s, v, kind) in refs.items()
        if t == 5 and kind == 1 and m == own and d[sec(s) + v:].split(b'\0', 1)[0].rstrip() == b'KASOKU']
m, s, U, kind = refs[5, row - 4]; assert (m, s, kind) == (own, 1, 1)
LEN = 0x264
B, D = 0x81000000, 0x82000000
T, W, P0, P1, H0, STACK = 0x100000, 0x100100, 0x100200, 0x100300, 0x100400, 0x110000
END = B + U + LEN
base = lambda s: B if s == 1 else D + s * 0x400000
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
mapped = set()
def page(a, n=4):
    for p in range(a & ~4095, a + n, 4096):
        if p not in mapped: u.mem_map(p, 4096); mapped.add(p)
def put(a, f, v): u.mem_write(a, struct.pack('>' + f, v))
def get(a, f): return struct.unpack('>' + f, u.mem_read(a, struct.calcsize('>' + f)))[0]
code = bytearray(d[sec(1) + U:sec(1) + U + LEN])
for (t, off), (mod, s, v, k) in refs.items():
    if t != 1 or not U <= off < U + LEN: continue
    val = base(s) + v if mod == own else v
    page(val)
    rel_off = off - U
    if k == 4: struct.pack_into('>H', code, rel_off, val & 0xffff)
    elif k == 6: struct.pack_into('>H', code, rel_off, ((val + 0x8000) >> 16) & 0xffff)
    elif k == 10:
        ins = struct.unpack_from('>I', code, rel_off)[0]
        struct.pack_into('>I', code, rel_off, (ins & 0xfc000003) | ((val - (B + off)) & 0x3fffffc))
    else: raise AssertionError(k)
for s in (4, 5):
    page(base(s), size(s)); u.mem_write(base(s), d[sec(s):sec(s) + size(s)])
page(base(6) + 0x6f8)
page(B + U, LEN + 4); u.mem_write(B + U, bytes(code)); put(END, 'I', 0x4e800020)
for a in (T, STACK - 0x1000, STACK, 0x801e7368, 0x801e7388): page(a, 0x400)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
reg = lambda i: u.reg_read(getattr(R, f'UC_PPC_REG_{i}'))
def setreg(i, v): u.reg_write(getattr(R, f'UC_PPC_REG_{i}'), v)
def fpr1(v): setreg('FPR1', struct.unpack('>Q', struct.pack('>d', v))[0])
calls = []; obs = {}
def hook(uc, pc, sz, name):
    if name == 'gate': setreg(3, int(obs['clipped']))
    elif name == 'init': calls.append({'call': 'initCollision'})
    elif name == 'hit':
        h = obs['hit']; setreg(3, 0 if h is None else (get(0x801e7388 + 4, 'I') if h == 1 else H0 + 0x10 * h))
    elif name == 'conv': fpr1(obs['frames'])
    elif name == 'accel':
        v = [get(reg(4) + 4 * i, 'f') for i in range(3)]
        a = [get(reg(5) + 4 * i, 'i') for i in range(3)]
        calls.append({'call': 'accelerate', 'slot': reg(3), 'velocity': [struct.unpack('>I', struct.pack('>f', x))[0] for x in v], 'angles': a, 'frames': reg(6)})
    elif name == 'sound':
        assert (reg(4), reg(5), reg(6)) == (0, 0, 0); calls.append({'call': 'sound', 'code': reg(3)})
    elif name == 'rumble': calls.append({'call': 'rumble', 'slot': reg(3), 'args': [reg(4), reg(5), reg(6)]})
    elif name == 'entry': calls.append({'call': 'entryCollision'})
for addr, name in [(0x8006538c, 'gate'), (0x8000f7d8, 'init'), (0x800066f4, 'hit'), (0x80110fa0, 'conv'),
                   (0x80039c48, 'accel'), (0x8006b710, 'sound'), (0x8002f8c4, 'rumble'), (0x8000f4ec, 'entry')]:
    page(addr); put(addr, 'I', 0x4e800020); u.hook_add(UC_HOOK_CODE, hook, begin=addr, end=addr, user_data=name)
put(0x801e7368, 'I', P0); put(0x801e7368 + 4, 'I', P1); put(0x801e7388 + 4, 'I', P1)
fbits = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
rng = random.Random(13)
cases = []
for case in range(64):
    scale = [rng.choice([-2.0, 0.0, 0.5, 8.0, 12.0, 14.0, 15.0]), rng.choice([-1.0, 0.0, 20.0, 60.0, 100.0, 119.99997]), rng.choice([0.0, 3.0, -19.0])]
    angles = [rng.choice([0, 1, 256, 16384, -16384, -32768, rng.randrange(-65536, 65536)]) for _ in range(3)]
    display = rng.choice([0, 1])
    u.mem_write(T, bytes(0x100)); u.mem_write(W, bytes(0x40))
    put(T + 0x34, 'I', W); put(base(6) + 0x6f8, 'I', display)
    for i, a in enumerate(angles): put(W + 8 + 4 * i, 'i', a)
    for i, x in enumerate(scale): put(W + 0x20 + 4 * i, 'f', x)
    frames, native = [], []
    for tick in range(48):
        obs = {'clipped': rng.random() < 0.08, 'hit': rng.choice([None, None, 0, 1, 2]),  # 2: some other task
               'ground': [rng.random() < 0.8, rng.random() < 0.8],
               'frames': rng.choice([0.0, 0.5, 3.0, 3.99, 20.0, 60.0, 119.99997, -1.5])}
        put(P0 + 4, 'h', 1 if obs['ground'][0] else 2); put(P1 + 4, 'h', 3 if obs['ground'][1] else 0)
        calls = []
        setreg(1, STACK); setreg(3, T); u.reg_write(R.UC_PPC_REG_LR, END)
        u.emu_start(B + U, END, count=400); assert u.reg_read(R.UC_PPC_REG_PC) == END
        frames.append(obs)
        native.append({'calls': calls, 'mode': get(W, 'b'), 'lastSlot': get(W + 1, 'b'), 'timer': get(T + 0x40, 'I'),
                       'scale': [get(W + 0x20, 'I'), get(W + 0x24, 'I'), get(W + 0x28, 'I')],
                       'displaySet': get(T + 0x14, 'I') != 0})
    cases.append({'angles': angles, 'scale': [fbits(x) for x in scale], 'frames': frames, 'native': native})
print(json.dumps({'update': U, 'module': own, 'cases': cases}))
