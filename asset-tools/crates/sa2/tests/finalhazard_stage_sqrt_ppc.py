"""Compare the Rust stage square root with the installed routine on Gekko rules.

Usage: python finalhazard_stage_sqrt_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0x40cc natively for many single-precision
inputs, with its `frsqrte` replaced by the Gekko hardware estimate
(gekko_float.py, from Dolphin). Every result must match the Rust port bit
for bit. It also counts the inputs where an exact reciprocal-root estimate
(Unicorn's own) would give a different final result.
"""
import json
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import ppc_const as R

# Gekko frsqrte from the shared local tables (tests/gekko_tables.py). Without the table file,
# Unicorn and the Rust port both use the exact estimate.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gekko_tables  # noqa: E402

install_frsqrte = gekko_tables.install_frsqrte if gekko_tables.load() else None

exe, module = sys.argv[1:]
if install_frsqrte is None:
    gekko_tables.require()  # skips (exit 0) without the table file
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: (word(word(16) + 8 * n) & ~1, word(word(16) + 8 * n + 4))
code = section(1)[0]
START, END = 0x40CC, 0x4158
CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000

imp_off, imp_size = struct.unpack_from(">II", data, 0x28)
relocs = {}
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


def machine(gekko):
    cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
    cpu.mem_map(0, 0x200000)
    cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
    cpu.mem_write(CODE + START, data[code + START:code + END])
    windows = {}
    for at, (t, mod, s, add) in relocs.items():
        key = (mod, s, add)
        if key not in windows:
            windows[key] = 0x40000 + 0x1000 * len(windows)
            start, size = section(s)
            cpu.mem_write(windows[key], data[start + add:start + min(size, add + 0x1000)])
        dest = windows[key]
        half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
        cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))
    if gekko:
        if install_frsqrte:
            install_frsqrte(cpu, CODE + 0x40E4)
    return cpu


def run(cpu, x):
    cpu.reg_write(R.UC_PPC_REG_FPR1, struct.unpack(">Q", struct.pack(">d", x))[0])
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + START, RETURN, count=200)
    out = struct.unpack(">d", struct.pack(">Q", cpu.reg_read(R.UC_PPC_REG_FPR1)))[0]
    return struct.unpack(">I", struct.pack(">f", out))[0]


rng = random.Random(421)
f32 = lambda bits: struct.unpack(">f", struct.pack(">I", bits))[0]
values = [0.0, 1.0, 2.0, 4.0, 2250000.0, 1e-30, f32(1), f32(0x7F7FFFFF), -1.0]
values += [f32(rng.randrange(0x00800000, 0x7F800000)) for _ in range(20000)]
values += [float(rng.uniform(0, 4_000_000)) for _ in range(20000)]
values = [struct.unpack(">f", struct.pack(">f", v))[0] for v in values]
gekko, exact = machine(True), machine(False)
native = [run(gekko, v) for v in values]
differ = sum(1 for v, n in zip(values, native) if run(exact, v) != n)
Path(".local").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=".local") as directory:
    q = Path(directory) / "request.json"
    out = Path(directory) / "out.json"
    q.write_text(json.dumps(dict(path="stage_sqrt", values=values)))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    got = json.loads(out.read_text())["bits"]
mismatch = [(v, g, n) for v, g, n in zip(values, got, native) if g != n]
assert not mismatch, mismatch[:5]
print(f"{len(values)} stage square roots matched the Gekko-estimate routine bit for bit; "
      f"an exact estimate would change {differ} of them")
