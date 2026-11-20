"""A3VE intro init/resume/update control oracle with declared dependencies.

Original wrappers, interpolation constructors and 0x3628 address lookup execute.
Listed external helpers/callbacks are intercepted as no-ops. The 0x39A0 resource
boundary writes synthetic same-length data, not authentic BIOS output. Separate
local-ROM LZ77 hashes verify resource decoding without publishing native bytes.
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
import reference_parity
import spp_vectors  # noqa: E402

FIXTURE = spp_vectors.path("intro-vectors.json")
ROUTINES = {"initialize": (0x6EF4, 0x7840), "resume": (0x7840, 0x8168), "update": (0x8168, 0x81E8),
            "constructor": (0x6544, 0x663C), "addressLookup": (0x3628, 0x3640), "codecDispatch": (0x39A0, 0x3A20)}
ARITY = {0x3250: 0, 0x5F0BC: 0, 0x0884: 0, 0x5F040: 0, 0x3278: 0, 0x3010: 0,
         0x30FC: 0, 0x5618: 1, 0x5784: 3, 0x5724: 3, 0x5668: 0, 0x1DC4: 1,
         0x2560: 0, 0x48DCC: 0, 0x37D4: 2, 0x2F68: 0, 0x2F74: 2, 0x2580: 0, 0x2700: 0}
RESOURCES = [0x082D453C, 0x082D5468, 0x082E1BE0, 0x082E2D88, 0x082E3480]
CONTEXT = 0x020199A0
CALLBACK = 0x03006101


def pattern(seed, length):
    return struct.pack("<" + "I" * (length // 4), *[(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(length // 4)])


def synthetic_resource(source, length):
    return bytes(((source >> 1) + index * 37) & 255 for index in range(length))


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    oracle.uc.mem_map(0x02000000, 0x40000)
    oracle.uc.mem_map(0x06000000, 0x20000)
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
    registers = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3]
    calls, resources, constructors, constructed_values = [], [], [], []
    pending_constructor = None

    def hook(cpu, address, _size, _data):
        nonlocal pending_constructor
        if pending_constructor is not None and address == pending_constructor[0]:
            _return, destination, index = pending_constructor
            constructed_values[index] = bytes(cpu.mem_read(destination,32))
            pending_constructor = None
        offset = address - control.ROM_BASE
        if offset in ARITY:
            calls.append({"entryOffset": offset, "arguments": [cpu.reg_read(registers[index]) for index in range(ARITY[offset])]})
        elif offset == 0x39A0:
            destination, source = [cpu.reg_read(registers[index]) for index in range(2)]
            raw = oracle.rom[source - control.ROM_BASE:]
            if raw[0] != 0x10:
                raise AssertionError("Intro resource is not original type10")
            length = int.from_bytes(raw[1:4], "little")
            cpu.mem_write(destination, synthetic_resource(source, length))
            resources.append({"sourceAddress": source, "destination": destination, "length": length})
            cpu.reg_write(UC_ARM_REG_R0, length)
            cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))
            return
        elif offset == 0x6544:
            sp = cpu.reg_read(UC_ARM_REG_SP)
            end_y, descriptor, flags, speed = struct.unpack("<4I", cpu.mem_read(sp, 16))
            constructors.append({"startX": cpu.reg_read(UC_ARM_REG_R1) & 65535,
                                 "startY": cpu.reg_read(UC_ARM_REG_R2) & 65535,
                                 "endX": cpu.reg_read(UC_ARM_REG_R3) & 65535, "endY": end_y & 65535,
                                 "descriptorAddress": descriptor, "flags": flags & 65535, "speed": speed & 65535})
            constructed_values.append(None)
            pending_constructor = (cpu.reg_read(UC_ARM_REG_LR) & ~1,cpu.reg_read(UC_ARM_REG_R0),len(constructors)-1)
            return
        elif address == CALLBACK - 1:
            calls.append({"callbackId": CALLBACK, "arguments": [cpu.reg_read(UC_ARM_REG_R0)]})
        else:
            return
        cpu.reg_write(UC_ARM_REG_R0, 0)
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(0x6EF4)
    initializers = []
    rows = {}
    destination_indices = None

    def reset(seed):
        oracle.uc.mem_write(0x02000000, pattern(seed, 0x40000))
        oracle.uc.mem_write(0x03000000, pattern(seed ^ 0xA55A1234, 0x8000))
        oracle.uc.mem_write(0x06000000, pattern(seed ^ 0x7ACE1200, 0x20000))
        oracle.uc.mem_write(0x02001CA0, struct.pack("<I", CONTEXT))
        calls.clear()
        resources.clear()
        constructors.clear()
        constructed_values.clear()

    def snapshot():
        return {"ewramSha256": hashlib.sha256(oracle.uc.mem_read(0x02000000, 0x40000)).hexdigest(),
                "iwramWithoutStackSha256": hashlib.sha256(oracle.uc.mem_read(0x03000000, 0x7800)).hexdigest(),
                "vramSha256": hashlib.sha256(oracle.uc.mem_read(0x06000000, 0x20000)).hexdigest()}

    for routine in ["initialize", "resume"]:
        for mode in range(8):
            seed = rng.randrange(1 << 32)
            prefix = rng.randrange(1 << 32)
            reset(seed)
            shadow = struct.unpack("<H", oracle.uc.mem_read(0x03000770, 2))[0]
            oracle.uc.mem_write(0x03000770, struct.pack("<H", (shadow & ~7) | mode))
            oracle.uc.mem_write(0x030078A0, struct.pack("<I", prefix))
            oracle.call(routine, rng.randrange(1 << 32))
            if len(constructors) != 39:
                raise AssertionError("Expected39 actualnativeconstructorcalls")
            if routine in rows and constructors != rows[routine]:
                raise AssertionError("Constructor arguments depend on unspecified state")
            rows[routine] = constructors.copy()
            final_contexts = [bytes(oracle.uc.mem_read(0x02001CA8+index*32,32)) for index in range(39)]
            if len(set(constructed_values)) != 39 or set(final_contexts) != set(constructed_values):
                raise AssertionError("Native constructor/context copy relationship is not one-to-one")
            permutation = [final_contexts.index(value) for value in constructed_values]
            if destination_indices is not None and permutation != destination_indices:
                raise AssertionError("Native constructor destination permutation depends on state/variant")
            destination_indices = permutation
            initializers.append({"routine": routine, "seed": seed, "mode": mode, "stackPrefix": prefix,
                                 "calls": calls.copy(), "resourceRequests": resources.copy(),
                                 "contextStates": [list(struct.unpack("<16H",oracle.uc.mem_read(0x02001CA8+index*32,32))) for index in range(39)], **snapshot()})
    updates = []
    for skip in [0, 1, 2, 0xFFFFFFFF]:
        for pressed in [0, 1, 0x3FF, 0xFFFF]:
            for depth in [0, 1, 10]:
                seed = rng.randrange(1 << 32)
                reset(seed)
                oracle.uc.mem_write(0x02002190, struct.pack("<I", skip))
                oracle.uc.mem_write(0x03000514, struct.pack("<H", pressed))
                oracle.uc.mem_write(0x03002060, struct.pack("<H", depth))
                if depth:
                    oracle.uc.mem_write(0x03002064 + (depth - 1) * 4, struct.pack("<I", CALLBACK))
                oracle.call("update", CONTEXT)
                updates.append({"seed": seed, "skipState": skip, "pressed": pressed, "auxiliaryDepth": depth,
                                "calls": calls.copy(), **snapshot()})
    actual_resources = []
    for address in RESOURCES:
        offset = address - control.ROM_BASE
        decoded, consumed = reference_parity.decode(oracle.rom[offset:], maximum=0x20000)
        actual_resources.append({"sourceAddress": address, "offset": offset, "storedLength": consumed,
                                 "storedSha256": hashlib.sha256(oracle.rom[offset:offset+consumed]).hexdigest(),
                                 "decodedLength": len(decoded), "decodedSha256": hashlib.sha256(decoded).hexdigest()})
    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256,
            "scope": "Original6EF4/7840/8168 wrappercontrol/writes, native6544 constructor and3628 lookup run; listed direct/indirect dependencies intercepted as no-ops;39A0 intercepted with synthetic same-length output; no BIOS/audio/screen acceptance",
            "routines": {name: {"offset": start, "length": end-start, "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in ROUTINES.items()},
            "interceptedDirectEntries": sorted(ARITY), "interceptedCallbackId": CALLBACK,
            "lookup": {"offset": 0x66C54, "length": 32, "sha256": hashlib.sha256(oracle.rom[0x66C54:0x66C74]).hexdigest(),
                       "values": list(struct.unpack_from("<8I", oracle.rom, 0x66C54))},
            "contextArguments": rows, "contextDestinationIndices": destination_indices, "authenticLz77Resources": actual_resources,
            "initializers": initializers, "updates": updates}


class IntroReferenceTests(unittest.TestCase):
    def test_fixture_declares_interception_and_all_native_contexts(self):
        data = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"], control.ROM_SHA256)
        self.assertEqual(len(data["initializers"]), 16)
        self.assertEqual(len(data["updates"]), 48)
        for rows in data["contextArguments"].values():
            self.assertEqual(len(rows), 39)
        self.assertEqual(len(data["authenticLz77Resources"]), 5)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_wrapper_control_and_writes_regenerate_fixture(self):
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
        raise SystemExit("Native intro wrapper fixture mismatch")
    print(json.dumps({"verified": True, "nativeInitResumeCases": len(data["initializers"]), "nativeUpdateCases": len(data["updates"]), "boundary": data["scope"]}))
