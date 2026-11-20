"""Compare the Rust Finalhazard core contact routines with installed PPC.

Usage: python finalhazard_core_contact_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x7154 (hit compare), 0x71e0 (knockback)
and 0x72e0 (proximity voice) to their return. Engine imports and local
calls are observed; float arguments f1..f3 and stack vectors are compared."""
import itertools
import json
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: (word(word(16) + 8 * n) & ~1, word(word(16) + 8 * n + 4))
code = section(1)[0]
START, END = 0x7154, 0x743C
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
REGIONS = dict(task=0x80000, entity=0x81000, block=0x82000, collision=0x84000)

relocs = {}
imp_off, imp_size = struct.unpack_from(">II", data, 0x28)
for i in range(imp_off, imp_off + imp_size, 8):
    mod, off = struct.unpack_from(">II", data, i)
    pos, sec = 0, None
    while True:
        o, t, s, add = struct.unpack_from(">HBBI", data, off)
        off += 8
        if t == 203:
            break
        pos += o
        if t == 202:
            sec, pos = s, 0
        elif t != 201 and sec == 1 and START <= pos < END:
            relocs[pos] = (t, mod, s, add)

cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(CODE + START, data[code + START:code + END])
windows, imports = {}, {}
for at, (t, mod, s, add) in sorted(relocs.items()):
    if t == 10:
        imports[at] = add
        continue
    if mod == 88 and s == 1:
        dest = CODE + add
    else:
        key = (mod, s, add)
        if key not in windows:
            windows[key] = 0x40000 + 0x1000 * len(windows)
            if mod == 88 and section(s)[0]:
                cpu.mem_write(windows[key], data[section(s)[0] + add:][:0x1000])
        dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))
pristine = {k: bytes(cpu.mem_read(w, 0x1000)) for k, w in windows.items()}


def symbol(v):
    if STACK - 0x200 <= v < STACK:
        return "stack"
    if CODE <= v < CODE + 0x20000:
        return f"88:1+{v - CODE:#x}"
    for (mod, s, add), w in windows.items():
        if w <= v < w + 0x1000:
            return f"88:{s}+{add + v - w:#x}" if mod == 88 else add + v - w
    return v & 0xFFFFFFFF


def vec(u, addr):
    return list(struct.unpack(">3f", u.mem_read(addr, 12)))


def code_hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    for addr, fmt, value in changes.get(off, ()):
        u.mem_write(addr, struct.pack(fmt, value))
    ins = struct.unpack(">I", u.mem_read(pc, 4))[0]
    if ins >> 26 != 18 or not ins & 1:
        return
    regs = [u.reg_read(getattr(R, f"UC_PPC_REG_{n}")) for n in range(3, 9)]
    fl = [struct.unpack(">d", struct.pack(">Q", u.reg_read(getattr(R, f"UC_PPC_REG_FPR{k}"))))[0]
          for k in (1, 2, 3)]
    f1 = fl
    vectors = {str(i): vec(u, r) for i, r in enumerate(regs[:3]) if 0x1000 <= r < 0x1FFFF4}
    if off in imports:
        target = imports[off]
        events.append(dict(site=off, call=target, args=regs, f1=f1, vectors=vectors))
    else:
        li = ins & 0x3FFFFFC
        if li & 0x2000000:
            li -= 0x4000000
        target = off + li
        events.append(dict(site=off, local=target, args=regs, f1=f1, vectors=vectors))
    for reg, value in effects.get(off, ()):
        if reg == "f1":
            u.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", value))[0])
        elif reg == "r3":
            u.reg_write(R.UC_PPC_REG_3, value & 0xFFFFFFFF)
        else:
            u.mem_write(regs[reg], struct.pack(">3f", *value))
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


def write_hook(u, access, addr, size, value, _):
    if STACK - 0x200 <= addr < STACK + 0x100:
        return
    site = u.reg_read(R.UC_PPC_REG_PC) - CODE
    for name, base in REGIONS.items():
        if base <= addr < base + 0x1000:
            events.append(dict(site=site, store=name, offset=addr - base, size=size,
                               value=symbol(value & ((1 << 8 * size) - 1))))
            return
    events.append(dict(site=site, store="module", offset=symbol(addr), size=size, value=value))


def compare(start, inp, n, regs=()):
    for k, w in windows.items():
        cpu.mem_write(w, pristine[k])
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    for r, v in regs:
        cpu.reg_write(r, v)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + start, RETURN + 4, count=20000)
    q, out = p / "request.json", p / f"{inp['path']}-{inp['routine']}-{n}.json"
    q.write_text(json.dumps(inp))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    report = json.loads(out.read_text())
    got = report["events"]
    if "returned" in report:
        assert report["returned"] == cpu.reg_read(R.UC_PPC_REG_3), (n, inp, report)
    assert [e["site"] for e in got] == [e["site"] for e in events], (n, inp, got, events)
    for r, e in zip(got, events):
        if "store" in r:
            assert (r["store"], r["offset"], r["size"], r["value"]) == (
                e["store"], e["offset"], e["size"], e["value"]), (n, r, e)
            continue
        assert r.get("call") == e.get("call") and r.get("local") == e.get("local"), (n, r, e)
        assert r["args"] == [symbol(v) for v in e["args"][:len(r["args"])]], (n, r, e)
        if "floats" in r:
            assert r["floats"] == e["f1"][:len(r["floats"])], (n, r, e)
        for i, v in r.get("vectors", {}).items():
            assert v == e["vectors"][i], (n, r, e)


rv = lambda: [struct.unpack('>f', struct.pack('>f', random.uniform(-300, 300)))[0] for _ in range(3)]
cpu.hook_add(UC_HOOK_CODE, code_hook)
cpu.hook_add(UC_HOOK_MEM_WRITE, write_hook)
rng = random.Random(17)
f32 = lambda x: struct.unpack(">f", struct.pack(">f", x))[0]
k4 = lambda o: struct.unpack_from(">f", data, section(4)[0] + o)[0]
T, E, B, C = REGIONS["task"], REGIONS["entity"], REGIONS["block"], REGIONS["collision"]
cases = 0
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for n in range(300):
        events, changes = [], {}
        attacker, hit = rng.choice((0, T)), rng.choice((0, C))
        hword, b = rng.randrange(1 << 32), [rng.randrange(-128, 128) for _ in range(2)]
        de, at = [rng.choice((0, 1, -1, 5, 0x7FFFFFFF, -0x80000000)) for _ in range(2)]
        cpu.mem_write(0x80000, bytes(0x6000))
        cpu.mem_write(C, struct.pack(">bbxxI", b[0], b[1], hword))
        effects = {0x7198: [("r3", de)], 0x71AC: [("r3", at)]}
        compare(0x7154, dict(path="core_contact", routine="compare", attacker=attacker, hit=hit,
                             hit_word=hword, hit_bytes=b, defense=de, attack=at), n,
                [(R.UC_PPC_REG_3, attacker), (R.UC_PPC_REG_4, hit)])
        cases += 1
    edge = k4(0x298)
    for n in range(300):
        events, changes = [], {}
        strength = f32(rng.uniform(-40, 40))
        sid = n % 2
        py = f32(rng.choice((edge, edge + 1, edge - 1, rng.uniform(-500, 500))))
        player = 0x6000 + n
        vector = rv() if n % 3 else None
        normal = rv()
        cpu.mem_write(0x80000, bytes(0x6000))
        cpu.mem_write(player, struct.pack(">24xf", py))
        pristine[(88, 6, 0x4A8)] = struct.pack(">I", 0x85000) + bytes(0xFFC)
        cpu.mem_write(0x85000, struct.pack(">b", sid))
        pristine[(0, 7, 0x801E7368)] = struct.pack(">II", player if sid == 0 else 0,
                                                   player if sid == 1 else 0) + bytes(0xFF8)
        effects = {0x72A4: [(0, normal)]}
        if vector:
            cpu.mem_write(E, struct.pack(">3f", *vector))
        cpu.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", strength))[0])
        compare(0x71E0, dict(path="core_contact", routine="knockback", strength=strength,
                             vector=vector, normal=normal, stage_id=sid, player_y=py), n,
                [(R.UC_PPC_REG_3, E if vector else 0)])
        cases += 1
    near = k4(0x2D8)
    for n in range(600):
        events, changes = [], {}
        sid = n % 2
        player = 0x6000 + n
        dist = f32(rng.choice((near, near - 1, near + 1, 0.0, rng.uniform(0, 2 * near))))
        flags = rng.choice((0, 0x100, -1, 0x0FF, rng.randrange(-32768, 32768)))
        rnd = rng.choice((0, 0x7FFF, rng.randrange(0x8000), -1))
        curve = f32(rng.uniform(-2, 2))
        cpu.mem_write(0x80000, bytes(0x6000))
        cpu.mem_write(T + 0x34, struct.pack(">I", E))
        cpu.mem_write(E + 4, struct.pack(">h", flags))
        pristine[(88, 6, 0x4A8)] = struct.pack(">I", 0x85000) + bytes(0xFFC)
        cpu.mem_write(0x85000, struct.pack(">b", sid))
        pristine[(0, 7, 0x801E7368)] = struct.pack(">II", player if sid == 0 else 0,
                                                   player if sid == 1 else 0) + bytes(0xFF8)
        effects = {0x7330: [("f1", dist)], 0x739C: [("r3", rnd)], 0x73F0: [("f1", curve)]}
        compare(0x72E0, dict(path="core_contact", routine="voice", task=T, entity=E,
                             player=player, distance=dist, flags=flags, random=rnd, curve=curve),
                n, [(R.UC_PPC_REG_3, T)])
        cases += 1
print(f"{cases} installed PPC core contact calls matched")