"""Execute A3VE 0x2A74 reverse-in-order OAM composition on synthetic trees.

No dependency interception is needed. Fixtures publish synthetic links,
descriptors and RAM hashes, never local ROM code or native graphics bytes.
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

FIXTURE = spp_vectors.path("oam_tree-vectors.json")
COUNT = 0x03000018
RECORDS = 0x03001250
ORDER = 0x03000BF0
ROOT = 0x030023B0
OUTPUT_COUNT = 0x030004F4
HARDWARE = 0x03000E50
DESCRIPTORS = 0x03003000


def pattern(seed, count):
    return [(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(count)]


def links_for(count, kind, rng):
    links = [[0, 0] for _ in range(count)]
    if count == 0:
        return 0, links
    indexes = list(range(count))
    if kind == "random":
        rng.shuffle(indexes)
    if kind in ["left", "right"]:
        child = 0 if kind == "left" else 1
        for index in range(count - 1):
            links[index][child] = RECORDS + (index + 1) * 28
        return RECORDS, links

    def build(nodes):
        if not nodes:
            return 0
        middle = len(nodes) // 2
        index = nodes[middle]
        links[index] = [build(nodes[:middle]), build(nodes[middle + 1:])]
        return RECORDS + index * 28

    return build(indexes), links


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines["tree"] = (0x2A74, 0x2B78)
    rng = random.Random(0x2A74)
    requests = []
    for count in [0, 1, 2, 3, 4, 7, 16, 64, 127, 128]:
        for kind in ["left", "right", "balanced", "random"]:
            requests.append((kind + "-" + str(count), count, kind, 0, "normal", None))
    for output in [1, 126, 127, 128, 129, 255]:
        for kind in ["left", "right", "balanced"]:
            requests.append(("output-" + str(output) + "-" + kind, 16, kind, output, "normal", None))
    for flags in [0, 0x1F, 0x3E0, 0xFC00, 0xFFFF, 0xA55A]:
        requests.append(("flags-" + str(flags), 3, "balanced", 0, "normal", flags))
    requests.extend([
        ("single-descriptor-output-cap", 1, "left", 0, "long", None),
        ("pre-full-skips-invalid-descriptors", 128, "right", 128, "invalid", None),
        ("pre-full-ff-skips-invalid-descriptors", 128, "left", 255, "invalid", None),
        ("two-byte-terminators", 7, "balanced", 0, "empty", None),
        ("last-slot-stops-before-next-triplet", 1, "left", 127, "one", None),
    ])
    for index in range(32):
        requests.append(("random-" + str(index), rng.randrange(129), "random",
                         rng.choice([0, 1, 126, 127, 128, 255]), "normal", None))
    cases = []
    for name, count, kind, output_count, mode, flags in requests:
        seed = rng.randrange(1 << 32)
        root, links = links_for(count, kind, rng)
        queue_count = rng.choice([0, count, 128, 255])
        initial_records = pattern(seed, 128 * 7)
        records = []
        descriptor_words = []
        for index in range(count):
            pointer = DESCRIPTORS + len(descriptor_words) * 2
            length = rng.randrange(5)
            if mode == "long":
                length = 130
            elif mode == "one":
                length = 1
            elif mode == "empty":
                length = 0
            if flags is not None:
                length = 3
            for _ in range(length):
                descriptor_words.extend([rng.randrange(65535),
                                         rng.choice([0xFFFF, rng.randrange(65536)]),
                                         rng.choice([0xFFFF, rng.randrange(65536)])])
            if mode != "one":
                descriptor_words.append(0xFFFF)
            record = initial_records[index * 7:(index + 1) * 7]
            record[0:2] = links[index]
            # Keys intentionally need not obey a BST ordering: this routine
            # follows links and does not consult sort keys.
            record[2] = rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)])
            record[3] = 0xFFFFFFFF if mode == "invalid" else pointer
            record[4] = rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)])
            record[5] = rng.choice([0, 0xFFFFFFFF, rng.randrange(1 << 32)])
            record[6] = (record[6] & 0xFFFF0000) | (rng.randrange(65536) if flags is None else flags)
            records.append(record)
            initial_records[index * 7:(index + 1) * 7] = record
        initial_order = pattern(seed ^ 0xA55A1234, 129)
        hardware = pattern(seed ^ 0x7ACE1200, 256)
        oracle.uc.mem_write(RECORDS, struct.pack("<896I", *initial_records))
        oracle.uc.mem_write(ORDER, struct.pack("<129I", *initial_order))
        oracle.uc.mem_write(COUNT, bytes([queue_count]))
        oracle.uc.mem_write(ROOT, struct.pack("<I", root))
        oracle.uc.mem_write(OUTPUT_COUNT, bytes([output_count]))
        oracle.uc.mem_write(HARDWARE, struct.pack("<256I", *hardware))
        oracle.uc.mem_write(DESCRIPTORS, bytes(0x4000))
        if descriptor_words:
            oracle.uc.mem_write(DESCRIPTORS, struct.pack("<" + "H" * len(descriptor_words), *descriptor_words))
        before_records = bytes(oracle.uc.mem_read(RECORDS, 128 * 28))
        oracle.call("tree")
        if bytes(oracle.uc.mem_read(RECORDS, 128 * 28)) != before_records:
            raise AssertionError("Native compositor unexpectedly changed records")
        if oracle.uc.mem_read(COUNT, 1)[0] != queue_count or struct.unpack("<I", oracle.uc.mem_read(ROOT, 4))[0] != root:
            raise AssertionError("Native compositor unexpectedly changed queue count/root")
        cases.append({"name": name, "seed": seed, "queueCount": queue_count,
                      "root": root, "records": records,
                      "descriptorHalfwords": descriptor_words,
                      "initialOutputCount": output_count,
                      "finalOutputCount": oracle.uc.mem_read(OUTPUT_COUNT, 1)[0],
                      "orderSha256": hashlib.sha256(oracle.uc.mem_read(ORDER, 129 * 4)).hexdigest(),
                      "hardwareSha256": hashlib.sha256(oracle.uc.mem_read(HARDWARE, 128 * 8)).hexdigest()})
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Original 0x2A74 executes without stubs; queue count ignored; reverse-in-order links, zero-based stack and staged OAM verified on synthetic inputs",
            "routine": {"offset": 0x2A74, "length": 0x104,
                        "sha256": hashlib.sha256(oracle.rom[0x2A74:0x2B78]).hexdigest()},
            "masks": {"offset": 0x66C28, "length": 20,
                      "sha256": hashlib.sha256(oracle.rom[0x66C28:0x66C3C]).hexdigest()},
            "descriptorBase": DESCRIPTORS,
            "syntheticPattern": "word[index] = (seed + index * 0x01020304) mod 2^32; order seed XOR A55A1234; hardware seed XOR 7ACE1200",
            "cases": cases}


class OamTreeReferenceTests(unittest.TestCase):
    def test_fixture_covers_deep_trees_scratch_reuse_and_ignored_count(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertGreaterEqual(len(data["cases"]), 100)
        counts = {len(case["records"]) for case in data["cases"]}
        self.assertTrue({0, 1, 127, 128}.issubset(counts))
        self.assertTrue(any(case["queueCount"] == 255 and case["records"] for case in data["cases"]))
        self.assertTrue(any(case["queueCount"] == 0 and len(case["records"]) == 128 for case in data["cases"]))
        for case in data["cases"]:
            self.assertEqual(len(case["orderSha256"]), 64)
            self.assertEqual(len(case["hardwareSha256"]), 64)
            if case["initialOutputCount"] >= 128:
                self.assertEqual(case["initialOutputCount"], case["finalOutputCount"])
            else:
                self.assertLessEqual(case["finalOutputCount"], 128)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_machine_code_regenerates_scratch_and_hardware_hashes(self):
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
        raise SystemExit("Native linked-tree compositor parity fixture mismatch")
    print(json.dumps({"sourceSha256": control.ROM_SHA256, "nativeTreeCompositorCases": len(data["cases"]), "verified": True}))
