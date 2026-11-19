"""Execute A3VE 0x2700 heap ordering/OAM expansion on synthetic resources.

The original local ROM routine executes without dependency interception. Only
synthetic descriptor halfwords, scalar queue records and output RAM hashes are
published. The optional regeneration check authenticates the whole local ROM.
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

FIXTURE = Path(__file__).with_name("oam_render-vectors.json")
COUNT = 0x03000018
RECORDS = 0x03001250
ORDER = 0x03000BF0
OUTPUT_COUNT = 0x030004F4
HARDWARE = 0x03000E50
DESCRIPTORS = 0x03003000


def pattern(seed, count):
    return [(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(count)]


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines["compose"] = (0x2700, 0x28EC)
    rng = random.Random(0x2700)
    requests = []
    for output in [0, 127, 128, 255]:
        requests.append(("empty-" + str(output), [], output, "normal", 0))
    for flags in [0, 0x1F, 0x3E0, 0xFC00, 0xFFFF, 0xA55A]:
        requests.append(("single-flags-" + str(flags), [0xF0000000], 0, "normal", flags))
    for count in [2, 3, 4, 5, 16, 64, 127, 128]:
        requests.append(("ascending-" + str(count), list(range(count)), 0, "normal", None))
        requests.append(("descending-" + str(count), list(reversed(range(count))), 0, "normal", None))
        requests.append(("all-ties-" + str(count), [0xFFFFFFFF] * count, 127, "normal", None))
        requests.append(("mixed-ties-" + str(count), [index % 3 for index in range(count)], 1, "normal", None))
    requests.append(("single-descriptor-output-cap", [0], 0, "long", None))
    requests.append(("pre-full-skips-invalid-descriptor", [1, 0], 128, "invalid", None))
    requests.append(("pre-full-ff-skips-invalid-descriptor", [1, 0], 255, "invalid", None))
    requests.append(("single-two-byte-terminator", [0], 0, "empty", None))
    requests.append(("last-slot-stops-before-next-triplet", [0], 127, "one", None))
    for index in range(48):
        count = rng.randrange(129)
        keys = [rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)]) for _ in range(count)]
        requests.append(("random-" + str(index), keys, rng.choice([0, 1, 126, 127, 128, 129, 255]), "normal", None))
    cases = []
    for name, keys, output_count, mode, flags in requests:
        seed = rng.randrange(1 << 32)
        initial_records = pattern(seed, 128 * 7)
        records = []
        descriptor_words = []
        for index, key in enumerate(keys):
            pointer = DESCRIPTORS + len(descriptor_words) * 2
            length = rng.randrange(5)
            if mode == "long":
                length = 130
            elif mode == "one":
                length = 1
            elif mode == "empty":
                length = 0
            if mode == "normal" and flags is not None:
                length = 3
            for triplet in range(length):
                # FFFF in attr1/attr2 is data; only attr0 is a terminator.
                descriptor_words.extend([rng.randrange(65535),
                                         rng.choice([0xFFFF, rng.randrange(65536)]),
                                         rng.choice([0xFFFF, rng.randrange(65536)])])
            if mode != "one":
                descriptor_words.append(0xFFFF)
            record = initial_records[index * 7:(index + 1) * 7]
            record[2] = key
            record[3] = 0xFFFFFFFF if mode == "invalid" else pointer
            record[4] = rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)])
            record[5] = rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)])
            record[6] = (record[6] & 0xFFFF0000) | (rng.randrange(65536) if flags is None else flags)
            records.append(record)
            initial_records[index * 7:(index + 1) * 7] = record
        initial_order = pattern(seed ^ 0xA55A1234, 129)
        active_order = [RECORDS + index * 28 for index in range(len(keys))]
        if name.startswith("random"):
            rng.shuffle(active_order)
        initial_order[1:len(keys) + 1] = active_order
        hardware = pattern(seed ^ 0x7ACE1200, 256)
        oracle.uc.mem_write(RECORDS, struct.pack("<896I", *initial_records))
        oracle.uc.mem_write(ORDER, struct.pack("<129I", *initial_order))
        oracle.uc.mem_write(COUNT, bytes([len(keys)]))
        oracle.uc.mem_write(OUTPUT_COUNT, bytes([output_count]))
        oracle.uc.mem_write(HARDWARE, struct.pack("<256I", *hardware))
        # Reset all synthetic descriptor RAM so fixtures never depend on a
        # previous case. The bounded Rust resource stops at its actual length.
        oracle.uc.mem_write(DESCRIPTORS, bytes(0x4000))
        if descriptor_words:
            oracle.uc.mem_write(DESCRIPTORS, struct.pack("<" + "H" * len(descriptor_words), *descriptor_words))
        oracle.call("compose")
        cases.append({"name": name, "seed": seed, "queueCount": len(keys),
                      "records": records, "initialOrder": active_order,
                      "descriptorHalfwords": descriptor_words,
                      "initialOutputCount": output_count,
                      "finalOutputCount": oracle.uc.mem_read(OUTPUT_COUNT, 1)[0],
                      "orderSha256": hashlib.sha256(oracle.uc.mem_read(ORDER, 129 * 4)).hexdigest(),
                      "hardwareSha256": hashlib.sha256(oracle.uc.mem_read(HARDWARE, 128 * 8)).hexdigest()})
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Original 0x2700 executes without stubs; descriptors and all initial RAM are synthetic; DMA and linked-tree compositor are separate",
            "routine": {"offset": 0x2700, "length": 0x1EC,
                        "sha256": hashlib.sha256(oracle.rom[0x2700:0x28EC]).hexdigest()},
            "masks": {"offset": 0x66C28, "length": 20,
                      "sha256": hashlib.sha256(oracle.rom[0x66C28:0x66C3C]).hexdigest()},
            "descriptorBase": DESCRIPTORS,
            "syntheticPattern": "word[index] = (seed + index * 0x01020304) mod 2^32; order seed XOR A55A1234; hardware seed XOR 7ACE1200",
            "cases": cases}


class OamRenderReferenceTests(unittest.TestCase):
    def test_fixture_covers_heap_ties_capacity_and_descriptor_guards(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertGreaterEqual(len(data["cases"]), 90)
        counts = {case["queueCount"] for case in data["cases"]}
        self.assertTrue({0, 1, 127, 128}.issubset(counts))
        for case in data["cases"]:
            self.assertEqual(len(case["orderSha256"]), 64)
            self.assertEqual(len(case["hardwareSha256"]), 64)
            if case["initialOutputCount"] >= 128:
                self.assertEqual(case["initialOutputCount"], case["finalOutputCount"])
            else:
                self.assertLessEqual(case["finalOutputCount"], 128)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_machine_code_regenerates_staged_hardware_and_order_hashes(self):
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
        raise SystemExit("Native sorted descriptor compositor parity fixture mismatch")
    print(json.dumps({"sourceSha256": control.ROM_SHA256, "nativeCompositorCases": len(data["cases"]), "verified": True}))
