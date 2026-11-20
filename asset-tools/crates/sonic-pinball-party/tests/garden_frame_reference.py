"""A3VE Garden input and one main-loop iteration against native instructions.

VBlank, boot, scene callback, sound and reset services are intercepted. Input
code and the native indirect-call trampoline execute. Tests prove source order
and retained RAM, not boot/hardware/scene completion or an end-to-end game.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import struct
import unittest
import control_reference as control
import reference_parity as parity

FIXTURE = Path(__file__).with_name("garden-frame-vectors.json")
BASE = 0x02000000
GLOBALS = 0x03004250
INPUT = 0x03005310


def generate(path):
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
    o = control.NativeOracle(path)
    payload, consumed = parity.decode(o.rom[parity.GARDEN_OFFSET:], parity.GARDEN_LENGTH)
    if consumed != parity.GARDEN_STORED or parity.sha256(payload) != parity.GARDEN_SHA256:
        raise ValueError("Exact A3VE Garden source required")
    o.uc.mem_map(BASE, 0x40000)
    o.uc.mem_write(BASE, bytes(payload))
    calls = []
    mutation = None

    def helper(cpu, address, _size, _user):
        off = address - BASE
        args = [cpu.reg_read(UC_ARM_REG_R0)] if off == 0x11b0c else []
        calls.append([off, args])
        if off == 0x1d68 and mutation is not None:
            cpu.mem_write(GLOBALS + 8, struct.pack("<I", mutation[0]))
            cpu.mem_write(INPUT, struct.pack("<H", mutation[1]))
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    for off in (0xc4, 0x107a0, 0x10660, 0x11b04, 0x10c70, 0x11b0c, 0x1d68):
        o.uc.hook_add(UC_HOOK_CODE, helper, begin=BASE + off, end=BASE + off)
    rng = random.Random(0x0bd00178)
    cases = []
    for i in range(544):
        routine = "initializeInput" if i < 16 else "initialize" if i < 32 else "poll" if i < 416 else "tick"
        inp = bytearray(rng.randbytes(12))
        glob = bytearray(rng.randbytes(20))
        glob[0:4] = struct.pack("<I", BASE + 0x1d69)
        raw = rng.randrange(65536)
        if i % 3 == 0:
            raw = (raw & 0xfc00) | (0x3ff ^ (1 << (i % 10)))
        if i % 11 == 0:
            raw = (raw & 0xfc00) | 0x3f0
            glob[15] &= ~0x40
        if i % 7 == 0:
            inp[0:2] = struct.pack("<H", (~raw) & 0x3ff)
            inp[8] = [0, 1, 2, 255][i % 4]
        mutation = [0xffffffff if i % 2 else i, i & 1023] if routine == "tick" and i % 4 == 0 else None
        o.uc.mem_write(INPUT, bytes(inp))
        o.uc.mem_write(GLOBALS, bytes(glob))
        o.uc.mem_write(0x04000000, b"Z" * 0x400)
        o.uc.mem_write(0x04000130, struct.pack("<H", raw))
        calls.clear()
        o.uc.reg_write(UC_ARM_REG_SP, 0x03007e00)
        o.uc.reg_write(UC_ARM_REG_LR, 0x03007f01)
        if routine == "tick":
            o.uc.reg_write(UC_ARM_REG_R4, GLOBALS)
            o.uc.reg_write(UC_ARM_REG_R5, 1)
        off, stop = {"initializeInput": (0xc6c, 0x03007f00), "initialize": (0x168, BASE + 0x178),
                     "poll": (0xbd0, 0x03007f00), "tick": (0x178, BASE + 0x192)}[routine]
        o.uc.emu_start(BASE + off + 1, stop, count=10000)
        if o.uc.reg_read(UC_ARM_REG_PC) != stop:
            raise ValueError("Native Garden frame exceeded bounded stop")
        cases.append({"routine": routine, "input": list(inp), "globals": list(glob), "raw": raw,
                      "mutation": mutation, "calls": list(calls),
                      "inputFinal": list(o.uc.mem_read(INPUT, 12)), "globalsFinal": list(o.uc.mem_read(GLOBALS, 20)),
                      "mmioSha256": hashlib.sha256(o.uc.mem_read(0x04000000, 0x400)).hexdigest()})
    spans = [("main", 0x168, 0x19c), ("input", 0xbd0, 0xc90), ("callbackTrampoline", 0x12498, 0x1249a)]
    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256, "payloadSha256": parity.GARDEN_SHA256,
            "scope": __doc__.strip(), "routines": [{"name": name, "space": "garden", "offset": a,
            "length": b-a, "sha256": parity.sha256(payload[a:b])} for name, a, b in spans], "cases": cases}


class GardenFrameReferenceTests(unittest.TestCase):
    def test_frame_and_reset_boundaries_are_explicit(self):
        f = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(len(f["cases"]), 544)
        self.assertEqual(sum(c["routine"] == "tick" for c in f["cases"]), 128)
        self.assertTrue(any([0x11b0c, [32]] in c["calls"] for c in f["cases"]))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local ROM and Unicorn")
    def test_native_instructions_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", required=True, type=Path)
    p.add_argument("--write", type=Path)
    a = p.parse_args()
    f = generate(a.rom)
    if a.write:
        a.write.write_text(json.dumps(f, separators=(",", ":")) + "\n", encoding="utf8")
    elif f != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise ValueError("Native Garden frame differs")
    print(json.dumps({"cases": len(f["cases"]), "wholeGardenPorted": False}))
