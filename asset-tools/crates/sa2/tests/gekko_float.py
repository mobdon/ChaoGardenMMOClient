"""Gekko floating-point behaviour Unicorn does not reproduce.

Unicorn (QEMU) computes `frsqrte` as an exact reciprocal square root. The
GameCube's Gekko returns a table-based estimate. This module transcribes
Dolphin's hardware-verified estimate (Source/Core/Common/FloatUtils.cpp,
ApproximateReciprocalSquareRoot, dolphin-emu/dolphin master) and installs
it over a known `frsqrte` instruction in an emulated code image.
"""
import struct

from unicorn import UC_HOOK_CODE
from unicorn import ppc_const as R

FRSQRTE_EXPECTED = [
    (0x1A7E800, -0x568), (0x17CB800, -0x4F3), (0x1552800, -0x48D), (0x130C000, -0x435),
    (0x10F2000, -0x3E7), (0x0EFF000, -0x3A2), (0x0D2E000, -0x365), (0x0B7C000, -0x32E),
    (0x09E5000, -0x2FC), (0x0867000, -0x2D0), (0x06FF000, -0x2A8), (0x05AB800, -0x283),
    (0x046A000, -0x261), (0x0339800, -0x243), (0x0218800, -0x226), (0x0105800, -0x20B),
    (0x3FFA000, -0x7A4), (0x3C29000, -0x700), (0x38AA000, -0x670), (0x3572000, -0x5F2),
    (0x3279000, -0x584), (0x2FB7000, -0x524), (0x2D26000, -0x4CC), (0x2AC0000, -0x47E),
    (0x2881000, -0x43A), (0x2665000, -0x3FA), (0x2468000, -0x3C2), (0x2287000, -0x38E),
    (0x20C1000, -0x35E), (0x1F12000, -0x332), (0x1D79000, -0x30A), (0x1BF4000, -0x2E6),
]
QNAN = 0x7FF8000000000000


def frsqrte_bits(bits):
    """Gekko frsqrte on a double's bit pattern; returns the result's bits."""
    mantissa = bits & ((1 << 52) - 1)
    sign = bits & (1 << 63)
    exponent = bits & (0x7FF << 52)
    if mantissa == 0 and exponent == 0:
        return 0xFFF0000000000000 if sign else 0x7FF0000000000000
    if exponent == 0x7FF << 52:
        if mantissa == 0:
            return QNAN if sign else 0
        return bits | 0x0008000000000000
    if sign:
        return QNAN
    if exponent == 0:
        while True:
            exponent -= 1 << 52
            mantissa <<= 1
            if mantissa & (1 << 52):
                break
        mantissa &= (1 << 52) - 1
        exponent += 1 << 52
    exponent_lsb = exponent & (1 << 52)
    # C++ signed division truncates toward zero.
    delta = exponent - (0x3FE << 52)
    half = abs(delta) // 2 * (1 if delta >= 0 else -1)
    exponent = ((0x3FF << 52) - half) & (0x7FF << 52)
    i = (exponent_lsb | mantissa) >> 37
    base, dec = FRSQRTE_EXPECTED[i // 2048]
    return (sign | exponent | ((base + dec * (i % 2048)) << 26)) & 0xFFFFFFFFFFFFFFFF


def install_frsqrte(cpu, address):
    """Replace the `frsqrte` instruction at `address` with the Gekko estimate."""
    ins = struct.unpack(">I", cpu.mem_read(address, 4))[0]
    assert ins >> 26 == 63 and (ins >> 1) & 0x1F == 26, f"no frsqrte at {address:#x}"
    d, b = (ins >> 21) & 31, (ins >> 11) & 31

    def hook(u, pc, size, _):
        src = u.reg_read(getattr(R, f"UC_PPC_REG_FPR{b}"))
        u.reg_write(getattr(R, f"UC_PPC_REG_FPR{d}"), frsqrte_bits(src))
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)

    cpu.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)
