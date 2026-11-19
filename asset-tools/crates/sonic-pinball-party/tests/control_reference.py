"""Execute supplied A3VE control helpers as an independent native oracle.

Requires the user's local ROM and Python unicorn. No native code bytes or native
save contents are embedded in this file or its scalar input/output fixtures.
The generated snapshots are synthetic RAM halfwords, not executable contents.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import struct
import unittest

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
FIXTURE = Path(__file__).with_name("control-vectors.json")
ROM_BASE = 0x08000000
STACK_BASE = 0x03000E00
INPUT_BASE = 0x03000510
ROUTINES = {
    "initialize": (0x2C64, 0x2C9C),
    "poll": (0x2C9C, 0x2CC0),
    "group": (0x2CC0, 0x2DCC),
    "per-key": (0x2DCC, 0x2EC0),
    "clear": (0x2EC0, 0x2ECC),
    "push": (0x2ECC, 0x2F00),
    "pop": (0x2F00, 0x2F14),
    "replace": (0x2F14, 0x2F34),
}


class NativeOracle:
    def __init__(self, path):
        with open(path, "rb") as stream:
            self.rom = stream.read(0x800001)
        if len(self.rom) != 0x800000 or hashlib.sha256(self.rom).hexdigest() != ROM_SHA256:
            raise ValueError("Exact local A3VE revision 0 source required")
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB
        from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2
        from unicorn.arm_const import UC_ARM_REG_R3, UC_ARM_REG_SP, UC_ARM_REG_LR

        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.uc.mem_map(ROM_BASE, 0x800000)
        self.uc.mem_write(ROM_BASE, self.rom)
        self.uc.mem_map(0x03000000, 0x8000)
        self.uc.mem_map(0x04000000, 0x1000)
        self.registers = (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3)
        self.sp = UC_ARM_REG_SP
        self.lr = UC_ARM_REG_LR
        self.routines = dict(ROUTINES)

    def call(self, name, *arguments):
        for register, value in zip(self.registers, list(arguments) + [0] * 4):
            self.uc.reg_write(register, value & 0xFFFFFFFF)
        self.uc.reg_write(self.sp, 0x03007E00)
        self.uc.reg_write(self.lr, 0x03007F01)
        self.uc.emu_start(ROM_BASE + self.routines[name][0] + 1, 0x03007F00, count=100000)
        from unicorn.arm_const import UC_ARM_REG_PC
        if self.uc.reg_read(UC_ARM_REG_PC) != 0x03007F00:
            raise RuntimeError("Native helper failed to return within instruction bound")

    def write_words(self, address, values):
        self.uc.mem_write(address, struct.pack("<" + "H" * len(values), *values))

    def read_words(self, address, count):
        return list(struct.unpack("<" + "H" * count, self.uc.mem_read(address, count * 2)))


def stack_case(oracle, name, initial, operations):
    oracle.write_words(STACK_BASE, initial)
    snapshots = []
    for operation in operations:
        args = (operation["callback"], operation["parameter"]) if operation["kind"] in ("push", "replace") else ()
        oracle.call(operation["kind"], *args)
        snapshots.append(oracle.read_words(STACK_BASE, 32))
    return {"name": name, "initial": initial, "operations": operations, "snapshots": snapshots}


def input_case(oracle, name, initial, operations):
    oracle.write_words(INPUT_BASE, initial)
    snapshots = []
    for operation in operations:
        kind = operation["kind"]
        if kind == "initialize":
            oracle.call(kind, operation["initialDelay"], operation["minimumDelay"], operation["acceleration"])
        elif kind == "poll":
            oracle.write_words(0x04000130, [operation["raw"]])
            oracle.call(kind)
        else:
            oracle.call(kind, operation["mask"])
        snapshots.append(oracle.read_words(INPUT_BASE, 45))
    return {"name": name, "initial": initial, "operations": operations, "snapshots": snapshots}


def init(delay=32, minimum=4, acceleration=2):
    return {"kind": "initialize", "initialDelay": delay, "minimumDelay": minimum, "acceleration": acceleration}


def frame(raw, kind="group", mask=0x3FF):
    return [{"kind": "poll", "raw": raw}, {"kind": kind, "mask": mask}]


def generate(path):
    oracle = NativeOracle(path)
    rng = random.Random(0xA3CE)
    patterned_stack = [rng.randrange(65536) for _ in range(32)]
    patterned_stack[0] = 0
    ops = [{"kind": "pop"}, {"kind": "replace", "callback": 0x080081E9, "parameter": 0xFFFF8000}]
    ops += [{"kind": "push", "callback": (0x08009001 + index * 4), "parameter": 0xABC00000 + index} for index in range(12)]
    ops += [{"kind": "replace", "callback": 0x0801CF4D, "parameter": 0xFEDC1234}]
    ops += [{"kind": "pop"} for _ in range(13)]
    ops += [{"kind": "clear"}, {"kind": "push", "callback": 0xFFFFFFFF, "parameter": 0xFFFFFFFF}]
    stacks = [stack_case(oracle, "empty-full-pop-replace-preserves-inactive-slots", patterned_stack, ops)]
    patterned_full = [rng.randrange(65536) for _ in range(32)]
    patterned_full[0] = 10
    stacks.append(stack_case(oracle, "full-clear-retains-slots", patterned_full, [
        {"kind": "push", "callback": 0x12345678, "parameter": 0x12345678},
        {"kind": "replace", "callback": 0x87654321, "parameter": 0x87654321},
        {"kind": "clear"}, {"kind": "pop"},
        {"kind": "replace", "callback": 0, "parameter": 0x10000},
    ]))
    random_ops = []
    for _ in range(80):
        kind = rng.choice(["push", "push", "pop", "replace", "clear"])
        operation = {"kind": kind}
        if kind in ("push", "replace"):
            operation.update(callback=rng.randrange(1 << 32), parameter=rng.randrange(1 << 32))
        random_ops.append(operation)
    stacks.append(stack_case(oracle, "deterministic-mixed-stack-operations", [0] * 32, random_ops))

    cases = []
    for kind in ["group", "per-key"]:
        operations = [init(8, 2, 1)]
        for raw in [0x3FF] * 2 + [0x3AF] * 25 + [0x3BF] * 3 + [0x3BE] * 12 + [0x3FF] * 5:
            operations += frame(raw, kind)
        cases.append(input_case(oracle, kind + "-press-hold-diagonal-change-release", [0] * 45, operations))
        operations = [init()]
        for index in range(15):
            operations += frame(0x3EE, kind, 0xF0 if index % 2 else 0xF)
        operations += frame(0xFFFF, kind, 0)
        operations += frame(0, kind, 0xFFFF)
        cases.append(input_case(oracle, kind + "-partial-zero-mask-and-high-bits", [0xA55A] * 45, operations))
    operations = [init(0x1234FFFF, 0x43218000, 0xFFFF7FFF)]
    for raw in [0x3FE, 0x3FE, 0x3FF, 0x3EF, 0x3EF, 0, 0, 0xFFFF]:
        operations += frame(raw, "group")
        operations += [{"kind": "per-key", "mask": 0xFFFF}]
    cases.append(input_case(oracle, "halfword-truncation-and-signed-countdown-boundaries", [0x8123] * 45, operations))
    operations = []
    for _ in range(80):
        kind = rng.choice(["initialize", "poll", "group", "per-key"])
        if kind == "initialize":
            operations.append(init(rng.randrange(1 << 32), rng.randrange(1 << 32), rng.randrange(1 << 32)))
        elif kind == "poll":
            operations.append({"kind": kind, "raw": rng.randrange(65536)})
        else:
            operations.append({"kind": kind, "mask": rng.randrange(65536)})
    cases.append(input_case(oracle, "deterministic-arbitrary-halfword-states", [rng.randrange(65536) for _ in range(45)], operations))
    routines = [dict(name=name, offset=start, length=end-start,
                     sha256=hashlib.sha256(oracle.rom[start:end]).hexdigest())
                for name, (start, end) in ROUTINES.items()]
    routines.append(dict(name="per-key-mask-table-including-terminator", offset=0x66C3C, length=22,
                         sha256=hashlib.sha256(oracle.rom[0x66C3C:0x66C52]).hexdigest()))
    return {"schemaVersion": 1, "sourceSha256": ROM_SHA256,
            "oracle": "Unicorn ARM Thumb executes original supplied-ROM helper instructions with bounded returns; no callback targets invoked",
            "routines": routines, "stackCases": stacks, "inputCases": cases}


class ControlReferenceTests(unittest.TestCase):
    def test_fixture_has_only_scalar_synthetic_state(self):
        fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(fixture["sourceSha256"], ROM_SHA256)
        self.assertEqual(fixture["schemaVersion"], 1)
        for key, count in [("stackCases", 32), ("inputCases", 45)]:
            self.assertTrue(fixture[key])
            for case in fixture[key]:
                self.assertEqual(len(case["initial"]), count)
                self.assertEqual(len(case["operations"]), len(case["snapshots"]))
                for snapshot in case["snapshots"]:
                    self.assertEqual(len(snapshot), count)
                    self.assertTrue(all(type(word) is int and 0 <= word <= 65535 for word in snapshot))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_machine_code_regenerates_all_snapshots(self):
        expected = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(generate(os.environ["SPP_ROM"]), expected)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path, help="Write metadata-only scalar vectors")
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    actual = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(actual, separators=(",", ":")) + "\n", encoding="utf-8")
    elif actual != json.loads(FIXTURE.read_text(encoding="utf-8")):
        raise SystemExit("Native control parity fixture mismatch")
    print(json.dumps({"sourceSha256": ROM_SHA256,
                      "stackOperations": sum(len(case["operations"]) for case in actual["stackCases"]),
                      "inputOperations": sum(len(case["operations"]) for case in actual["inputCases"]),
                      "verified": True}))
