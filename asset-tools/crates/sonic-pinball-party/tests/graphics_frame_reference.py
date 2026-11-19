"""Native A3VE staged-OAM initialization/reset and DMA register trace oracle.

All original helper instructions execute without dependency interception.
DMA evidence covers register transactions only: Unicorn does not implement
the hardware transfer or its timing. Inputs and RAM hashes are synthetic.
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

FIXTURE = Path(__file__).with_name("graphics_frame-vectors.json")
HARDWARE = 0x03000E50
OUTPUT_COUNT = 0x030004F4
AFFINE_COUNT = 0x0300005C
QUEUE_COUNT = 0x03000018
ROOT = 0x030023B0
ROUTINES = {"begin": (0x1DC4, 0x1DDC), "hide": (0x1DDC, 0x1E28),
            "affine": (0x1E28, 0x1E64), "dma": (0x2560, 0x2580),
            "queue": (0x2580, 0x258C), "tree": (0x28EC, 0x2904)}


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    rng = random.Random(0x1DC4)
    cases = []
    for argument in [0, 1, 7, 8, 31, 32, 127, 128, 255, 256, 0xFFFFFFFF] + [rng.randrange(1 << 32) for _ in range(21)]:
        seed = rng.randrange(1 << 32)
        output = rng.randrange(256)
        affine = rng.randrange(256)
        queue = rng.randrange(256)
        root = rng.randrange(1 << 32)
        oracle.uc.mem_write(HARDWARE, struct.pack("<256I", *[(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(256)]))
        oracle.uc.mem_write(OUTPUT_COUNT, bytes([output]))
        oracle.uc.mem_write(AFFINE_COUNT, bytes([affine]))
        oracle.uc.mem_write(QUEUE_COUNT, bytes([queue]))
        oracle.uc.mem_write(ROOT, struct.pack("<I", root))
        # Unrelated record and traversal RAM must stay unchanged.
        unrelated = bytes(rng.randrange(256) for _ in range(128 * 28))
        order = bytes(rng.randrange(256) for _ in range(129 * 4))
        oracle.uc.mem_write(0x03001250, unrelated)
        oracle.uc.mem_write(0x03000BF0, order)
        steps = []
        operations = ["affine", "hide", "queue", "begin", "tree", "affine", "hide"]
        for operation in operations:
            oracle.call(operation, argument)
            if bytes(oracle.uc.mem_read(0x03001250, len(unrelated))) != unrelated or bytes(oracle.uc.mem_read(0x03000BF0, len(order))) != order:
                raise AssertionError("Native helper changed unrelated queue records/order")
            steps.append({"operation": operation, "argument": argument,
                          "outputCount": oracle.uc.mem_read(OUTPUT_COUNT, 1)[0],
                          "affineCount": oracle.uc.mem_read(AFFINE_COUNT, 1)[0],
                          "submissionCount": oracle.uc.mem_read(QUEUE_COUNT, 1)[0],
                          "root": struct.unpack("<I", oracle.uc.mem_read(ROOT, 4))[0],
                          "hardwareSha256": hashlib.sha256(oracle.uc.mem_read(HARDWARE, 1024)).hexdigest()})
        cases.append({"seed": seed, "initialOutputCount": output, "initialAffineCount": affine,
                      "initialSubmissionCount": queue, "initialRoot": root, "steps": steps})
    from unicorn import UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE, UC_MEM_WRITE
    events = []

    def memory_event(_uc, access, address, size, value, _data):
        if size != 4:
            raise AssertionError("Unexpected DMA register width")
        if access == UC_MEM_WRITE:
            events.append({"kind": "write32", "address": address, "value": value})
        else:
            events.append({"kind": "read32", "address": address})

    hook = oracle.uc.hook_add(UC_HOOK_MEM_READ | UC_HOOK_MEM_WRITE, memory_event, begin=0x040000D4, end=0x040000DF)
    oracle.call("dma")
    oracle.uc.hook_del(hook)
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Original init/reset helpers run without stubs; DMA evidence is original MMIO instruction sequence only, no transfer/timing emulated",
            "routines": {name: {"offset": start, "length": end - start,
                                 "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in ROUTINES.items()},
            "callers": {"intro": {"entry": 0x8168, "beginCall": 0x816E, "queueResetCall": 0x8172, "sortedComposeCall": 0x81C4},
                        "mainFrame": {"entry": 0xAB78, "beginCall": 0xAB7E, "queueResetCall": 0xAB82, "sortedComposeCall": 0xABA6},
                        "alternateTree": "No direct Thumb BL in [0,0x608CC) and no 08002A75/080028ED pointer literals in complete source; runtime reachability remains unproven"},
            "dmaCommands": events, "cases": cases}


class GraphicsFrameReferenceTests(unittest.TestCase):
    def test_fixture_covers_truncation_and_original_mmio_sequence(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertEqual(len(data["cases"]), 32)
        self.assertEqual(len(data["dmaCommands"]), 4)
        for case in data["cases"]:
            self.assertEqual(len(case["steps"]), 7)
            for step in case["steps"]:
                if step["operation"] in ["begin", "affine"]:
                    self.assertEqual(step["affineCount"], step["argument"] & 255)
                if step["operation"] in ["begin", "hide"]:
                    self.assertEqual(step["outputCount"], 0)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_native_code_regenerates_fixture(self):
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
        raise SystemExit("Native graphics-frame parity mismatch")
    print(json.dumps({"verified": True, "nativeFrameOperations": sum(len(case["steps"]) for case in data["cases"]), "dmaRegisterEvents": len(data["dmaCommands"])}))
