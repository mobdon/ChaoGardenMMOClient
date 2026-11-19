"""Separate A3VE-ROM and primary numerical BIOS math oracle checks.

Division and trig execute original supplied-ROM machine code in Unicorn.
Sqrt/ArcTan2 reference cases are mathematical/primary-reference results; they
are not authentic BIOS execution. Optional --bios-reference-tool cross-checks
all such cases against a locally compiled pinned primary reference. Neither
native ROM/BIOS bytes nor third-party implementation source is published here.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import random
import subprocess
import unittest

import control_reference as control

FIXTURE = Path(__file__).with_name("math-vectors.json")
PRIMARY_URL = "https://raw.githubusercontent.com/mgba-emu/mgba/0.10.3/src/gba/bios.c"
PRIMARY_SHA256 = "8885962e1ca2d8c79a7f13a5d1e074bb2b39704a64aa164f76e1506868c8759a"


def signed(value, bits=32):
    value &= (1 << bits) - 1
    return value - (1 << bits) if value & (1 << (bits - 1)) else value


def quotient(a, b):
    if b == 0:
        return 0
    result = abs(a) // abs(b)
    return -result if (a < 0) != (b < 0) else result


def arctan_reference(ratio):
    squared = -((ratio * ratio) >> 14)
    result = 0xA9
    for coefficient in [0x390, 0x91C, 0xFB6, 0x16AA, 0x2081, 0x3651, 0xA2F9]:
        result = ((result * squared) >> 14) + coefficient
    return signed((ratio * result) >> 16, 16)


def atan2_reference(x, y):
    if y == 0:
        return 0 if x >= 0 else 0x8000
    if x == 0:
        return 0x4000 if y >= 0 else 0xC000
    if y >= 0:
        if x >= y:
            angle = arctan_reference(quotient(y << 14, x))
        elif x < 0 and -x >= y:
            angle = 0x8000 + arctan_reference(quotient(y << 14, x))
        else:
            angle = 0x4000 - arctan_reference(quotient(x << 14, y))
    else:
        if x <= 0 and -x > -y:
            angle = 0x8000 + arctan_reference(quotient(y << 14, x))
        elif x > 0 and x >= -y:
            angle = 0x10000 + arctan_reference(quotient(y << 14, x))
        else:
            angle = 0xC000 - arctan_reference(quotient(x << 14, y))
    return angle & 65535


def generate(path, bios_reference_tool=None):
    from unicorn.arm_const import UC_ARM_REG_R0
    oracle = control.NativeOracle(path)
    helpers = {"divide": (0x608CC, 0x60964), "sine": (0x2DF90, 0x2DFE0), "cosine": (0x2DFE0, 0x2DFF8)}
    oracle.routines.update(helpers)
    rng = random.Random(0xA3B105)
    division = []
    edges = [-2147483648, -2147483647, -65536, -32768, -1, 0, 1, 32767, 65535, 2147483647]
    pairs = [(a, b) for a in edges for b in edges]
    pairs += [(signed(rng.randrange(1 << 32)), signed(rng.randrange(1 << 32))) for _ in range(256)]
    for a, b in pairs:
        oracle.call("divide", a, b)
        division.append([a, b, signed(oracle.uc.reg_read(UC_ARM_REG_R0))])
        if division[-1][2] != signed(quotient(a, b)):
            raise AssertionError((a, b, division[-1]))
    angles = [0, 1, 15, 16, 31, 0x3FF0, 0x3FFF, 0x4000, 0x400F, 0x7FFF, 0x8000,
              0xBFFF, 0xC000, 0xC00F, 0xFFFF, 0x10000, 0xFFFFFFFF, 0xABCD1234]
    angles += [rng.randrange(1 << 32) for _ in range(96)]
    trig = []
    for angle in angles:
        oracle.call("sine", angle)
        sine = signed(oracle.uc.reg_read(UC_ARM_REG_R0))
        oracle.call("cosine", angle)
        cosine = signed(oracle.uc.reg_read(UC_ARM_REG_R0))
        trig.append([angle, sine, cosine])
    square_inputs = [0, 1, 2, 3, 4, 15, 16, 17, 262143, 262144, 262145, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFE, 0xFFFFFFFF]
    square_inputs += [rng.randrange(1 << 32) for _ in range(128)]
    roots = [[value, math.isqrt(value)] for value in square_inputs]
    coordinates = [(x, y) for x in [-32768, -32767, -512, -1, 0, 1, 512, 32767]
                   for y in [-32768, -32767, -512, -1, 0, 1, 512, 32767]]
    coordinates += [(rng.randrange(-32768, 32768), rng.randrange(-32768, 32768)) for _ in range(256)]
    atan = [[x, y, atan2_reference(x, y)] for x, y in coordinates]
    if bios_reference_tool:
        requests = [f"S {row[0]} 0" for row in roots] + [f"A {row[0]} {row[1]}" for row in atan]
        result = subprocess.run([str(bios_reference_tool)], input="\n".join(requests) + "\n",
                                text=True, capture_output=True, check=True, timeout=30)
        results = [int(value) for value in result.stdout.splitlines()]
        expected = [row[-1] for row in roots] + [row[-1] for row in atan]
        if results != expected:
            raise AssertionError("Pinned primary BIOS reference disagrees with scalar cases")
    spans = [(name, a, z) for name, (a, z) in helpers.items()]
    spans += [("bios-atan2-wrapper-only", 0x5FEAC, 0x5FEB0), ("bios-sqrt-wrapper-only", 0x5FEF4, 0x5FEF8),
              ("local-quarter-wave-resource", 0xAF022, 0xAF824)]
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "nativeScope": "Division and trigonometry execute original A3VE helper instructions; quarter-wave resource remains local",
            "biosScope": "Numerical reference only; authentic BIOS bytes, cycle timing and clobber-register parity are unavailable",
            "primaryBiosReference": {"url": PRIMARY_URL, "sha256": PRIMARY_SHA256,
                                     "reuse": "Numerical polynomial/quadrant specification reviewed; no third-party implementation source incorporated"},
            "routines": [dict(name=name, offset=a, length=z-a, sha256=hashlib.sha256(oracle.rom[a:z]).hexdigest()) for name, a, z in spans],
            "nativeDivision": division, "nativeTrig": trig,
            "referenceBiosSqrt": roots, "referenceBiosAtan2": atan}


class MathReferenceTests(unittest.TestCase):
    def test_fixture_declares_separate_bios_evidence(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertEqual(data["primaryBiosReference"]["sha256"], PRIMARY_SHA256)
        self.assertIn("Numerical reference only", data["biosScope"])
        self.assertGreater(len(data["nativeDivision"]), 300)
        self.assertGreater(len(data["nativeTrig"]), 100)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_native_rom_regenerates_and_reference_math_matches(self):
        self.assertEqual(generate(os.environ["SPP_ROM"], os.environ.get("SPP_BIOS_REFERENCE_TOOL")),
                         json.loads(FIXTURE.read_text(encoding="utf-8")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--bios-reference-tool", type=Path)
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    data = generate(args.rom, args.bios_reference_tool)
    if args.write:
        args.write.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")
    elif data != json.loads(FIXTURE.read_text(encoding="utf-8")):
        raise SystemExit("Native/reference math fixture mismatch")
    print(json.dumps({"sourceSha256": control.ROM_SHA256,
                      "nativeDivision": len(data["nativeDivision"]), "nativeTrig": len(data["nativeTrig"]),
                      "referenceBiosSqrt": len(data["referenceBiosSqrt"]), "referenceBiosAtan2": len(data["referenceBiosAtan2"]),
                      "primaryCompiledCrosscheck": bool(args.bios_reference_tool), "verified": True}))
