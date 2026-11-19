"""Execute A3VE's 0x2614 object-queue submission helper on synthetic RAM.

No dependency interception is required. Original machine code, count guard,
priority merge and all queue stores execute from the exact supplied local ROM.
Published vectors contain scalar arguments and hashes of synthetic output RAM;
they do not include native graphics, descriptor contents, or ROM code bytes.
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

FIXTURE = Path(__file__).with_name("oam-vectors.json")
COUNT = 0x03000018
RECORDS = 0x03001250
ORDER = 0x03000BF0
DESCRIPTOR = 0x03006000


def pattern(seed, count):
    return [(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(count)]


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines["submit"] = (0x2614, 0x2690)
    rng = random.Random(0x2614)
    requests = []
    for descriptor_priority in [0, 0x400, 0x800, 0xC00]:
        for flags_priority in [0, 0x400, 0x800, 0xC00]:
            requests.append((0, descriptor_priority | 0x135, flags_priority | 0xA15A, 0xFEDC1234, 0x12348001, 0xABCDF00D))
    for count in [0, 1, 126, 127, 128, 129, 255]:
        for _ in range(4):
            requests.append((count, rng.randrange(65536), rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32)))
    for _ in range(64):
        requests.append((rng.randrange(256), rng.randrange(65536), rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32)))
    cases = []
    for count, descriptor_word, flags, key, x, y in requests:
        seed = rng.randrange(1 << 32)
        auxiliary = rng.randrange(1 << 32)
        initial_records = pattern(seed, 128 * 7)
        initial_order = pattern(seed ^ 0xA55A1234, 129)
        oracle.uc.mem_write(RECORDS, struct.pack("<896I", *initial_records))
        oracle.uc.mem_write(ORDER, struct.pack("<129I", *initial_order))
        oracle.uc.mem_write(COUNT, bytes([count]))
        oracle.uc.mem_write(DESCRIPTOR + 4, descriptor_word.to_bytes(2, "little"))
        # The routine's fifth/sixth ABI arguments are at original SP/SP+4.
        oracle.uc.mem_write(0x03007E00, struct.pack("<II", flags, auxiliary))
        oracle.call("submit", key, x, y, DESCRIPTOR)
        final_count = oracle.uc.mem_read(COUNT, 1)[0]
        cases.append({"seed": seed, "initialCount": count,
                      "arguments": {"sortKey": key, "x": x, "y": y, "descriptorAddress": DESCRIPTOR,
                                    "descriptorHalfwordAt4": descriptor_word, "flags": flags, "auxiliary": auxiliary},
                      "accepted": final_count != count, "finalCount": final_count,
                      "recordsSha256": hashlib.sha256(oracle.uc.mem_read(RECORDS, 128 * 28)).hexdigest(),
                      "orderSha256": hashlib.sha256(oracle.uc.mem_read(ORDER, 129 * 4)).hexdigest()})
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Original 0x2614 queue submission executes without stubs; descriptor-to-hardware-OAM expansion remains separate",
            "routine": {"offset": 0x2614, "length": 0x7C,
                        "sha256": hashlib.sha256(oracle.rom[0x2614:0x2690]).hexdigest()},
            "syntheticPattern": "word[index] = (seed + index * 0x01020304) mod 2^32; order uses seed XOR 0xA55A1234",
            "cases": cases}


class OamReferenceTests(unittest.TestCase):
    def test_fixture_covers_count_guard_and_priority_merges(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertGreaterEqual(len(data["cases"]), 100)
        counts = {case["initialCount"] for case in data["cases"]}
        self.assertTrue({0, 127, 128, 255}.issubset(counts))
        for case in data["cases"]:
            self.assertEqual(case["accepted"], case["initialCount"] < 128)
            self.assertEqual(len(case["recordsSha256"]), 64)
            self.assertEqual(len(case["orderSha256"]), 64)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_machine_code_regenerates_queue_hashes(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf-8")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    data = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")
    elif data != json.loads(FIXTURE.read_text(encoding="utf-8")):
        raise SystemExit("Native object submission parity fixture mismatch")
    print(json.dumps({"sourceSha256": control.ROM_SHA256, "nativeSubmissionCases": len(data["cases"]), "verified": True}))
