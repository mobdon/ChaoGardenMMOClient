"""A3VE initial-wrapper oracle with explicitly intercepted intro dependencies.

Only the 0x81E8 wrapper and its native stack helpers execute. The three intro
dependencies are intercepted, their argument is recorded, and synthetic return
effects are injected. This is wrapper branch/order parity, not intro/game parity.
No ROM code bytes are published in the scalar fixtures.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import unittest

import control_reference as control

FIXTURE = Path(__file__).with_name("startup-vectors.json")
POINTER_SLOT = 0x02001CA0
INITIAL_CONTEXT = 0x020199A0
DEPENDENCIES = [0x6EF4, 0x7840, 0x8168]


def generate(path):
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_R0

    oracle = control.NativeOracle(path)
    oracle.uc.mem_map(0x02000000, 0x40000)
    oracle.routines["startup-wrapper"] = (0x81E8, 0x8260)
    intercepted = []
    returns = {}

    def hook(uc, address, size, user):
        offset = address - control.ROM_BASE
        if offset not in DEPENDENCIES:
            return
        intercepted.append({"entry_offset": offset, "argument0": uc.reg_read(UC_ARM_REG_R0)})
        if offset == 0x8168:
            uc.mem_write(POINTER_SLOT, returns["pointer"].to_bytes(4, "little"))
            uc.mem_write(returns["pointer"], returns["completion"].to_bytes(4, "little"))
        uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(0x81E8)
    cases = []
    states = [0, 1, 2, 3, 0x7FFF, 0x8000, 0xFFFF]
    for state in states:
        for completed in [0, 1, 0x80000000, 0xFFFFFFFF]:
            main = [rng.randrange(65536) for _ in range(32)]
            depth = rng.randrange(1, 11)
            main[0] = depth
            main[22 + depth - 1] = state
            main[2 + (depth - 1) * 2] = 0x81E9
            main[3 + (depth - 1) * 2] = 0x0800
            auxiliary = [rng.randrange(65536) for _ in range(32)]
            auxiliary[0] = rng.randrange(11)
            pointer = 0x02002000
            return_pointer = 0x02003000 if completed & 1 else pointer
            initial_completion = rng.randrange(1 << 32)
            oracle.write_words(control.STACK_BASE, main)
            oracle.write_words(0x03002060, auxiliary)
            oracle.uc.mem_write(POINTER_SLOT, pointer.to_bytes(4, "little"))
            oracle.uc.mem_write(pointer, initial_completion.to_bytes(4, "little"))
            returns.update(pointer=return_pointer, completion=completed)
            intercepted.clear()
            oracle.call("startup-wrapper")
            pointer_final = int.from_bytes(oracle.uc.mem_read(POINTER_SLOT, 4), "little")
            cases.append({
                "name": f"state-{state:04x}-completion-{completed:08x}",
                "mainInitial": main, "auxInitial": auxiliary,
                "pointerInitial": pointer, "completionInitial": initial_completion,
                "returnPointer": return_pointer, "returnCompletion": completed,
                "mainFinal": oracle.read_words(control.STACK_BASE, 32),
                "auxFinal": oracle.read_words(0x03002060, 32),
                "pointerFinal": pointer_final,
                "calls": list(intercepted),
            })
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Native wrapper and stack-helper execution only; all intro initialization/update dependencies explicitly intercepted",
            "wrapper": {"offset": 0x81E8, "length": 0x78,
                        "sha256": hashlib.sha256(oracle.rom[0x81E8:0x8260]).hexdigest()},
            "interceptedEntryOffsets": DEPENDENCIES, "cases": cases}


class StartupReferenceTests(unittest.TestCase):
    def test_fixture_declares_dependency_interception(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertEqual(data["interceptedEntryOffsets"], DEPENDENCIES)
        self.assertEqual(len(data["cases"]), 28)
        for case in data["cases"]:
            self.assertEqual(len(case["mainInitial"]), 32)
            self.assertEqual(len(case["auxInitial"]), 32)
            self.assertEqual(len(case["calls"]), 1)
            self.assertIn(case["calls"][0]["entry_offset"], DEPENDENCIES)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_native_wrapper_with_intercepted_dependencies_regenerates(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf-8")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    actual = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(actual, separators=(",", ":")) + "\n", encoding="utf-8")
    elif actual != json.loads(FIXTURE.read_text(encoding="utf-8")):
        raise SystemExit("Native startup wrapper fixture mismatch")
    print(json.dumps({"sourceSha256": control.ROM_SHA256, "wrapperCases": len(actual["cases"]),
                      "interceptedDependencies": DEPENDENCIES, "verifiedWrapperOnly": True}))
