"""Run main.dol's own atanf, atan2f, asinf, _inv_sqrtf and 0x8005071C.

Needs SA2_GSNE8P_DOL pointing at the GSNE8P main.dol and a Python with
Unicorn PPC support. Prints one line per case: kind, input bits, output bits.
"""
import hashlib
import os
import random
import struct
from pathlib import Path

from unicorn import UC_ARCH_PPC, UC_HOOK_CODE, UC_MODE_BIG_ENDIAN, UC_MODE_PPC32, Uc
from unicorn import ppc_const as R

import sys
sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402
from gekko_tables import install_frsqrte  # noqa: E402

gekko_tables.require()

dol = Path(os.environ["SA2_GSNE8P_DOL"]).read_bytes()
assert hashlib.sha1(dol).hexdigest() == "e4c50f32242a3cd36cdc8ae776661601a52130a2"

u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
u.mem_map(0x80000000, 0x400000)
offs = struct.unpack(">18I", dol[0:72])
addrs = struct.unpack(">18I", dol[0x48:0x90])
sizes = struct.unpack(">18I", dol[0x90:0xD8])
for o, a, s in zip(offs, addrs, sizes):
    if s:
        u.mem_write(a, dol[o:o + s])
STACK, SENTINEL, OUT = 0x803F0000, 0x80000100, 0x803F8000
u.mem_write(SENTINEL, struct.pack(">I", 0x60000000))  # nop; stopped by hook
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
install_frsqrte(u, 0x801110C0)  # _inv_sqrtf: Gekko's estimate, not QEMU's exact one

ATANF, ATAN2F, ASINF, INV_SQRTF, ANGLES = 0x80110E64, 0x80111168, 0x80111058, 0x801110B4, 0x8005071C


def stop(uc, pc, size, _):
    if pc == SENTINEL:
        uc.emu_stop()


u.hook_add(UC_HOOK_CODE, stop, begin=SENTINEL, end=SENTINEL + 4)


def fpr(i, v=None):
    reg = getattr(R, f"UC_PPC_REG_FPR{i}")
    if v is None:
        dbl = struct.unpack(">d", struct.pack(">Q", u.reg_read(reg)))[0]
        return struct.unpack(">f", struct.pack(">f", dbl))[0]
    u.reg_write(reg, struct.unpack(">Q", struct.pack(">d", v))[0])


def call(addr, *fs, r3=0, r4=0):
    u.reg_write(R.UC_PPC_REG_1, STACK)
    u.reg_write(R.UC_PPC_REG_2, 0x803B6520)
    u.reg_write(R.UC_PPC_REG_13, 0x803B52C0)
    u.reg_write(R.UC_PPC_REG_3, r3)
    u.reg_write(R.UC_PPC_REG_4, r4)
    u.reg_write(R.UC_PPC_REG_LR, SENTINEL)
    for i, v in enumerate(fs, 1):
        fpr(i, v)
    u.emu_start(addr, SENTINEL + 4, count=200000)


def bits(f):
    return struct.unpack(">I", struct.pack(">f", f))[0]


def f32(x):
    return struct.unpack(">f", struct.pack(">f", x))[0]


rng = random.Random(0x5A2)
values = [0.0, -0.0, 1.0, -1.0, 0.4142135, 0.41421357, 2.4142136, 1e-6, 3.0, 0.5, 0.75, 1.5, 2.0, 100.0]
values += [f32(rng.uniform(-4, 4)) for _ in range(400)]
values += [f32(rng.uniform(-1, 1)) for _ in range(200)]
values = [f32(v) for v in values]
for x in values:
    call(ATANF, x)
    print("atanf", bits(x), bits(fpr(1)))
    if -1 < x < 1:
        call(ASINF, x)
        print("asinf", bits(x), bits(fpr(1)))
    if x >= 0:
        call(INV_SQRTF, x)
        print("inv_sqrtf", bits(x), bits(fpr(1)))
for _ in range(400):
    y, x = f32(rng.uniform(-3, 3)), f32(rng.choice([rng.uniform(-3, 3), 0.0]))
    call(ATAN2F, y, x)
    print("atan2f", bits(y), bits(x), bits(fpr(1)))
normals = [(0.0, 0.0, 1.0), (0.0, 0.0, -1.0), (0.0, 1.0, 0.0), (1.0, 0.0, 0.0), (0.001, 0.0005, 0.5)]
for _ in range(300):
    v = [rng.uniform(-1, 1) for _ in range(3)]
    n = sum(c * c for c in v) ** 0.5
    normals.append(tuple(f32(c / n) for c in v))
for nx, ny, nz in normals:
    call(ANGLES, f32(nx), f32(ny), f32(nz), r3=OUT, r4=OUT + 4)
    ax, az = struct.unpack(">ii", bytes(u.mem_read(OUT, 8)))
    print("angles", bits(f32(nx)), bits(f32(ny)), bits(f32(nz)), ax, az)
