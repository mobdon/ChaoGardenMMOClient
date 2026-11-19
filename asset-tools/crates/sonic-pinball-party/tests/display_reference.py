"""Original A3VE display-shadow/MMIO transaction oracle.

Only 0x0D5C is intercepted: no display interrupt, hardware side effects or
VBlank timing is simulated. Complete shadow and IO snapshots are synthetic.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import unittest

import control_reference as control

FIXTURE = Path(__file__).with_name("display-vectors.json")
SHADOW = 0x03000770
IO = 0x04000000
ROUTINES = {"initialize": (0x3010, 0x30fc), "flush": (0x30fc, 0x3230),
            "wait": (0x3230, 0x3250), "clear": (0x3250, 0x3278), "set": (0x3278, 0x32cc)}


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_WRITE
    from unicorn.arm_const import UC_ARM_REG_LR, UC_ARM_REG_PC
    writes = []
    waits = []

    def code(uc, address, size, _):
        if address == control.ROM_BASE + 0xd5c:
            waits.append(address)
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def write(uc, access, address, size, value, _):
        if (SHADOW <= address < SHADOW + 84 or IO <= address < IO + 0x210):
            if size != 2:
                raise ValueError("Unexpected native display write size")
            writes.append([address, value])

    oracle.uc.hook_add(UC_HOOK_CODE, code)
    oracle.uc.hook_add(UC_HOOK_MEM_WRITE, write)
    rng = random.Random(0x3010)
    cases = []
    for operation in ROUTINES:
        arguments = [0, 1, 2, 7, 31, 255, 256] if operation == "wait" else range(32)
        for argument in arguments:
            shadow = [rng.randrange(65536) for _ in range(42)]
            io = [rng.randrange(65536) for _ in range(0x210//2)]
            if operation in ("set", "clear"):
                # Exhaust both live DISPCNT, DISPSTAT and IE gate bits.
                io[0] = (io[0] & ~0x80) | ((argument & 1) << 7)
                io[2] = (io[2] & ~8) | ((argument & 2) << 2)
                io[0x200//2] = (io[0x200//2] & ~1) | ((argument >> 2) & 1)
            oracle.write_words(SHADOW, shadow)
            oracle.write_words(IO, io)
            writes.clear()
            waits.clear()
            oracle.call(operation, argument)
            cases.append(dict(operation=operation, argument=argument, shadow=shadow, io=io,
                              writes=list(writes), waits=len(waits), afterShadow=oracle.read_words(SHADOW,42),
                              afterIo=oracle.read_words(IO,0x210//2)))
    return dict(schemaVersion=1, sourceSha256=control.ROM_SHA256,
                scope="Native register transactions; only VBlank dependency0D5C intercepted, no hardware/timing emulation",
                routines={name:dict(offset=a,length=b-a,sha256=hashlib.sha256(oracle.rom[a:b]).hexdigest())
                          for name,(a,b) in ROUTINES.items()}, cases=cases)


class DisplayReferenceTests(unittest.TestCase):
    def test_fixture_covers_all_display_gate_branches(self):
        data = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(len(data["cases"]), 135)
        sets = [c for c in data["cases"] if c["operation"] == "set"]
        self.assertEqual(sum(c["waits"] for c in sets), 4)
        for case in data["cases"]:
            if case["operation"] == "flush":
                self.assertEqual(len(case["writes"]), 42)
                self.assertEqual([w[0] for w in case["writes"][-2:]], [IO+0x208, IO+0x200])

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Unicorn")
    def test_native_instructions_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    p.add_argument("--write", type=Path)
    a = p.parse_args()
    if not a.rom:
        p.error("--rom or SPP_ROM is required")
    data = generate(a.rom)
    if a.write:
        a.write.write_text(json.dumps(data,separators=(",", ":"))+"\n", encoding="utf8")
    elif data != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise SystemExit("Native display oracle differs")
    print(json.dumps({"verified":True,"cases":len(data["cases"])}))
