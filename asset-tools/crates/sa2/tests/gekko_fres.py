"""Gekko `fres` for the main.dol harness.

Unicorn (QEMU), and the paired-single interpreter in dol_ppc.py, compute
`fres` as an exact reciprocal. Gekko returns a table estimate. This module
transcribes Dolphin's hardware-verified estimate (Source/Core/Common/
FloatUtils.cpp, ApproximateReciprocal; the same table as `gekko_fres` in
the Rust crate) and installs it on a Machine: over every `fres` in the DOL's
text that Unicorn runs, and in the interpreter.
"""
import struct

from unicorn import UC_HOOK_CODE
from unicorn import ppc_const as R

FRES_EXPECTED = [
    (0x7FF800, 0x3E1), (0x783800, 0x3A7), (0x70EA00, 0x371), (0x6A0800, 0x340),
    (0x638800, 0x313), (0x5D6200, 0x2EA), (0x579000, 0x2C4), (0x520800, 0x2A0),
    (0x4CC800, 0x27F), (0x47CA00, 0x261), (0x430800, 0x245), (0x3E8000, 0x22A),
    (0x3A2C00, 0x212), (0x360800, 0x1FB), (0x321400, 0x1E5), (0x2E4A00, 0x1D1),
    (0x2AA800, 0x1BE), (0x272C00, 0x1AC), (0x23D600, 0x19B), (0x209E00, 0x18B),
    (0x1D8800, 0x17C), (0x1A9000, 0x16E), (0x17AE00, 0x15B), (0x14F800, 0x15B),
    (0x124400, 0x143), (0x0FBE00, 0x143), (0x0D3800, 0x12D), (0x0ADE00, 0x12D),
    (0x088400, 0x11A), (0x065000, 0x11A), (0x041C00, 0x108), (0x020C00, 0x106),
]
F32_MAX = 0x47EFFFFFE0000000


def fres_bits(bits):
    """Gekko fres on a double's bit pattern; returns the result's bits."""
    mantissa = bits & ((1 << 52) - 1)
    sign = bits & (1 << 63)
    exponent = bits & (0x7FF << 52)
    if mantissa == 0 and exponent == 0:
        return sign | (0x7FF << 52)
    if exponent == 0x7FF << 52:
        if mantissa == 0:
            return sign
        return bits | 0x0008000000000000
    if exponent < 895 << 52:
        return sign | F32_MAX
    if exponent >= 1149 << 52:
        return sign
    exponent = (0x7FD << 52) - exponent
    i = mantissa >> 37
    base, dec = FRES_EXPECTED[i // 1024]
    return sign | exponent | ((base - (dec * (i % 1024) + 1) // 2) << 29)


def fres(x):
    return struct.unpack(">d", struct.pack(">Q", fres_bits(struct.unpack(">Q", struct.pack(">d", x))[0])))[0]


def install(m, dol_ppc):
    """Use the Gekko estimate for every fres `m` runs."""
    for a, o, z in m.dol.sections:
        if a >= 0x80173140:  # text sections only
            continue
        for k in range(0, z, 4):
            ins = struct.unpack_from(">I", m.dol.data, o + k)[0]
            if ins >> 26 == 59 and (ins >> 1) & 0x1F == 24:
                d, b = (ins >> 21) & 31, (ins >> 11) & 31

                def hook(u, pc, size, _, d=d, b=b):
                    src = u.reg_read(getattr(R, f"UC_PPC_REG_FPR{b}"))
                    u.reg_write(getattr(R, f"UC_PPC_REG_FPR{d}"), fres_bits(src))
                    u.reg_write(R.UC_PPC_REG_PC, pc + 4)

                m.u.hook_add(UC_HOOK_CODE, hook, begin=a + k, end=a + k)
    plain = dol_ppc.Interp.fp_single

    def fp_single(self, ins):
        if (ins >> 1) & 31 == 24:
            d, b = (ins >> 21) & 31, (ins >> 11) & 31
            v = fres(self.p0[b])
            self.p0[d] = v
            self.p1[d] = v
            return
        plain(self, ins)

    dol_ppc.Interp.fp_single = fp_single
