"""Execute A3VE Garden1D68 palette branches with synthetic RAM/palette sources.

The eight external Garden helpers are intercepted in source order. SWI0B is
only a 16-halfword copy; this proves data transactions, not BIOS timing or PPU.
Native program/resource bytes are never emitted.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import unittest
import control_reference as control
import reference_parity as parity

FIXTURE = Path(__file__).with_name("garden-palette-vectors.json")
BASE = 0x02000000
FLAGS = 0x030015a8
HELPERS = [0x16d8, 0x12e0, 0x1420, 0x590, 0x1374, 0xeac, 0x117c, 0x1088]


def pattern(seed, length):
    return bytes((seed + i * 37) & 255 for i in range(length))


def generate(path):
    from unicorn import UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
    o = control.NativeOracle(path)
    payload, consumed = parity.decode(o.rom[parity.GARDEN_OFFSET:], parity.GARDEN_LENGTH)
    if consumed != parity.GARDEN_STORED or parity.sha256(payload) != parity.GARDEN_SHA256:
        raise ValueError("Exact embedded A3VE Garden required")
    o.uc.mem_map(BASE, 0x40000)
    o.uc.mem_write(BASE, bytes(payload))
    o.uc.mem_map(0x05000000, 0x1000)
    calls = []
    mutation = None

    def helper(cpu, address, _size, _user):
        offset = address - BASE
        calls.append(offset)
        if offset == 0x1420 and mutation is not None:
            cpu.mem_write(FLAGS, bytes(mutation))
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    for off in HELPERS:
        o.uc.hook_add(UC_HOOK_CODE, helper, begin=BASE + off, end=BASE + off)

    def copy(cpu, interrupt, _user):
        if interrupt != 2 or cpu.reg_read(UC_ARM_REG_PC) != BASE + 0x11ae6:
            raise ValueError("Unexpected Garden BIOS call")
        source, destination, count = [cpu.reg_read(r) for r in (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2)]
        if count != 16 or destination not in (0x05000200, 0x050003c0):
            raise ValueError("Unexpected palette copy")
        # Copy in halfwords, preserving native source/destination ordering.
        for i in range(count):
            cpu.mem_write(destination + 2*i, bytes(cpu.mem_read(source + 2*i, 2)))

    o.uc.hook_add(UC_HOOK_INTR, copy)
    cases = []
    # Every byte selector value, including14+ RAM fallback and the unguarded
    # second selector. Nonzero flags and dirty bytes vary independently.
    for i in range(320):
        seed = (i * 59 + 17) & 255
        flags = [0 if i % 5 == 0 else (i % 255) + 1, i & 255, 0x99, 0x37,
                 0 if i % 7 == 0 else (i * 3 % 255) + 1, (255 - i) & 255, 0xa5, 0x63]
        mutation = [1, (i * 17) & 255] if i >= 256 else None
        o.uc.mem_write(0x03000000, pattern(seed, 0x8000))
        o.uc.mem_write(0x05000000, pattern(seed + 19, 0x400))
        o.uc.mem_write(BASE + 0x24050, pattern(seed + 7, 0x21c0))
        o.uc.mem_write(FLAGS, bytes(flags))
        calls.clear()
        o.uc.reg_write(UC_ARM_REG_SP, 0x03007e00)
        o.uc.reg_write(UC_ARM_REG_LR, 0x03007f01)
        o.uc.emu_start(BASE + 0x1d69, 0x03007f00, count=10000)
        if o.uc.reg_read(UC_ARM_REG_PC) != 0x03007f00:
            raise ValueError("Native Garden palette routine exceeded budget")
        cases.append({"seed": seed, "flags": flags, "mutation": mutation,
                      "calls": list(calls),
                      "iwramSha256": hashlib.sha256(o.uc.mem_read(0x03000000, 0x7800)).hexdigest(),
                      "paletteSha256": hashlib.sha256(o.uc.mem_read(0x05000000, 0x400)).hexdigest()})
    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256,
            "payloadSha256": parity.GARDEN_SHA256,
            "scope": __doc__.strip(),
            "routines": [{"name": "gardenPaletteUpdate", "space": "garden", "offset": 0x1d68,
                          "length": 0x98, "sha256": parity.sha256(payload[0x1d68:0x1e00])}],
            "cases": cases}


class GardenPaletteReferenceTests(unittest.TestCase):
    def test_all_byte_indices_and_dependency_mutations_are_recorded(self):
        v = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(len(v["cases"]), 320)
        self.assertEqual({c["flags"][1] for c in v["cases"]}, set(range(256)))
        self.assertTrue(all(c["calls"] == HELPERS for c in v["cases"]))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local ROM and Unicorn")
    def test_local_instructions_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", required=True, type=Path)
    p.add_argument("--write", type=Path)
    a = p.parse_args()
    result = generate(a.rom)
    if a.write:
        a.write.write_text(json.dumps(result, separators=(",", ":")) + "\n", encoding="utf8")
    elif result != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise ValueError("Garden palette oracle mismatch")
    print(json.dumps({"cases": len(result["cases"]), "wholeGardenPorted": False}))
