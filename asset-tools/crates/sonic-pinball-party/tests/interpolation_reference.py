"""Native A3VE interpolation constructor/update with actual OAM submission.

All original instructions execute without interception. The constructor's
uninitialized first stack word is seeded explicitly and published as opaque
scalar input. Descriptor halfwords, contexts and queue RAM are synthetic.
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
import spp_vectors  # noqa: E402

FIXTURE = spp_vectors.path("interpolation-vectors.json")
OBJECT = 0x03005000
DESCRIPTOR = 0x03006000
RECORDS = 0x03001250
ORDER = 0x03000BF0
COUNT = 0x03000018
ROUTINES = {"constructor": (0x6544, 0x663C), "update": (0x663C, 0x66F8),
            "submission": (0x2614, 0x2690)}


def halfwords(data):
    return list(struct.unpack("<" + "H" * (len(data) // 2), data))


def pattern(seed, count):
    return [(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(count)]


def arguments(sx, sy, ex, ey, speed, flags=0, descriptor=DESCRIPTOR):
    return {"startX": sx & 0xFFFFFFFF, "startY": sy & 0xFFFFFFFF,
            "endX": ex & 0xFFFFFFFF, "endY": ey & 0xFFFFFFFF,
            "speed": speed & 0xFFFFFFFF, "flags": flags & 0xFFFFFFFF,
            "descriptorAddress": descriptor}


def construct(oracle, args, prefix):
    # Native pushes32 bytes then reserves44 bytes. Its local firstword is
    # copied into the destination without any source initialization store.
    oracle.uc.mem_write(0x03007E00 - 0x4C, struct.pack("<I", prefix))
    oracle.uc.mem_write(0x03007E00, struct.pack("<4I", args["endY"], args["descriptorAddress"], args["flags"], args["speed"]))
    oracle.uc.mem_write(OBJECT, bytes([0xA5]) * 32)
    oracle.call("constructor", OBJECT, args["startX"], args["startY"], args["endX"])
    return bytes(oracle.uc.mem_read(OBJECT, 32))


def call_sites(rom, target):
    result = []
    for offset in range(0, 0x608CC, 2):
        a, z = struct.unpack_from("<HH", rom, offset)
        if a & 0xF800 == 0xF000 and z & 0xF800 == 0xF800:
            displacement = ((a & 0x7FF) << 12) | ((z & 0x7FF) << 1)
            if displacement & 0x400000:
                displacement -= 0x800000
            if offset + 4 + displacement == target:
                result.append(offset)
    return result


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    rng = random.Random(0x6544)
    endpoints = [(0, 0, 0, 0), (0, 0, 5, 2), (5, 2, 0, 0),
                 (0, 0, 2, 5), (2, 5, 0, 0), (-4, 5, 7, -2),
                 (0x7FFF, 0, -0x8000, 0), (-0x8000, 0, 0x7FFF, 0),
                 (0, 0, -0x8000, 0x7FFF), (0x7FFF, -0x8000, 0, 0),
                 (0, 0, 0x7FFF, 0x7FFE), (-0x8000, -0x8000, 0x7FFF, 0x7FFF)]
    constructor_inputs = [arguments(*endpoints_row, speed, rng.randrange(1 << 32))
                          for endpoints_row in endpoints for speed in [0, 1, 2, 3, 0x7FFF, 0x8000, 0xFFFF]]
    for _ in range(128):
        constructor_inputs.append(arguments(*[rng.randrange(1 << 32) for _ in range(4)], rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32)))
    constructors = []
    for args in constructor_inputs:
        prefix = rng.randrange(1 << 32)
        output = construct(oracle, args, prefix)
        if struct.unpack_from("<I", output)[0] != prefix:
            raise AssertionError("Native constructor no longer preserves explicitly seeded stack residue")
        constructors.append({"arguments": args, "stackPrefix": prefix, "state": halfwords(output)})
    requests = []
    for endpoints_row in endpoints[:6] + [endpoints[10], endpoints[8]]:
        for speed in [0, 1, 2, 3, 0xFFFF]:
            args = arguments(*endpoints_row, speed, rng.randrange(65536))
            initial = construct(oracle, args, rng.randrange(1 << 32))
            requests.append(("constructed-" + str(len(requests)), initial, rng.choice([0, 1, 126, 127, 128, 255]), 18))
    for count in [0, 126, 127, 128, 129, 255]:
        initial = construct(oracle, arguments(0, 0, 130, 5, 1, 0xFFFF), rng.randrange(1 << 32))
        requests.append(("capacity-" + str(count), initial, count, 134))
    for dx, dy, error, progress, speed in [(0x7FFF, 0x7FFE, 0x7FFF, 0, 1),
                                         (10, 5, 0xFFFF, 0, 2), (5, 10, 0xFFFF, 0, 2),
                                         (10, 5, 0x7FFF, 0, 3), (5, 10, 0x7FFF, 0, 3),
                                         (0, 0, 0, 0, 0xFFFF), (10, 5, 0, 0xFFFF, 1),
                                         (0x8000, 1, 0, 0, 1), (0xFFFF, 0x8000, 0, 0, 1)]:
        initial = bytearray(construct(oracle, arguments(0x7FFF, 0xFFFF, 0, 0, speed, 0xA55A), rng.randrange(1 << 32)))
        for at, value in [(0x10, dx), (0x12, dy), (0x14, error), (0x1C, progress)]:
            struct.pack_into("<H", initial, at, value)
        requests.append(("raw-boundary-" + str(len(requests)), bytes(initial), rng.choice([0, 127, 255]), 8))
    from unicorn.arm_const import UC_ARM_REG_R0
    updates = []
    for name, initial, count, ticks in requests:
        seed = rng.randrange(1 << 32)
        descriptor_word = rng.randrange(65536)
        oracle.uc.mem_write(OBJECT, initial)
        oracle.uc.mem_write(RECORDS, struct.pack("<896I", *pattern(seed, 128 * 7)))
        oracle.uc.mem_write(ORDER, struct.pack("<129I", *pattern(seed ^ 0xA55A1234, 129)))
        oracle.uc.mem_write(COUNT, bytes([count]))
        oracle.uc.mem_write(DESCRIPTOR + 4, struct.pack("<H", descriptor_word))
        steps = []
        for _ in range(ticks):
            oracle.call("update", OBJECT)
            result = oracle.uc.reg_read(UC_ARM_REG_R0)
            if result not in [0, 1]:
                raise AssertionError("Unexpected native completion ABI")
            steps.append({"state": halfwords(oracle.uc.mem_read(OBJECT, 32)), "complete": result != 0,
                          "queueCount": oracle.uc.mem_read(COUNT, 1)[0],
                          "recordsSha256": hashlib.sha256(oracle.uc.mem_read(RECORDS, 128 * 28)).hexdigest(),
                          "orderSha256": hashlib.sha256(oracle.uc.mem_read(ORDER, 129 * 4)).hexdigest()})
        updates.append({"name": name, "seed": seed, "initial": halfwords(initial),
                        "initialCount": count, "descriptorHalfwordAt4": descriptor_word, "steps": steps})
    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256,
            "scope": "Original constructor/update and actual0x2614 submission execute without stubs; descriptors/context/queue are synthetic; original uninitialized stack-prefix word supplied explicitly",
            "descriptorAddress": DESCRIPTOR,
            "routines": {name: {"offset": start, "length": end - start,
                                 "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in ROUTINES.items()},
            "callers": {"constructor": call_sites(oracle.rom, 0x6544),
                        "update": call_sites(oracle.rom, 0x663C),
                        "updateCallerEntry": 0x6824,
                        "introConstructors": {"first": 0x6EF4, "second": 0x7840, "contextsEach": 39}},
            "constructors": constructors, "updates": updates}


class InterpolationReferenceTests(unittest.TestCase):
    def test_fixture_covers_native_signed_wrapping_and_queue_boundaries(self):
        data = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"], control.ROM_SHA256)
        self.assertGreaterEqual(len(data["constructors"]), 200)
        self.assertGreaterEqual(len(data["updates"]), 50)
        self.assertEqual(len(data["callers"]["constructor"]), 78)
        self.assertEqual(len(data["callers"]["update"]), 38)
        counts = {case["initialCount"] for case in data["updates"]}
        self.assertTrue({0, 127, 128, 255}.issubset(counts))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_native_code_regenerates_context_and_queue_ram_hashes(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    data = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf8")
    elif data != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise SystemExit("Native interpolation parity fixture mismatch")
    print(json.dumps({"verified": True, "constructors": len(data["constructors"]), "updateOperations": sum(len(case["steps"]) for case in data["updates"])}))
