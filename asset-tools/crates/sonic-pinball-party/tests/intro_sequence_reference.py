"""Bounded original A3VE 6824 sequence oracle; native files remain local.

Original6824/613A8/60E7C/663C/2614/2FA8 execute. Transform236C, palette5724/
5784 and audio389C are intercepted no-ops. Synthetic cases replace only declared
resource tables/descriptor priority halfwords in emulator memory. Authentic
sequence hashes separately execute unchanged local tables and require that ROM
for Rust replay; timing and graphics bytes are never embedded in the fixture.
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

FIXTURE = spp_vectors.path("intro-sequence-vectors.json")
ROUTINES = {"sequence": (0x6824, 0x6DF0), "copy": (0x613A8, 0x61408),
            "unsignedDivide": (0x60E7C, 0x60EF4), "interpolation": (0x663C, 0x66F8),
            "submission": (0x2614, 0x2690), "pop": (0x2FA8, 0x2FBC),
            "queueReset": (0x2580, 0x258C), "complete": (0x6754, 0x6764),
            "startup": (0x81E8, 0x8260)}
ARITY = {0x236C: 5, 0x5724: 3, 0x5784: 3, 0x389C: 1}
RESOURCE_SPANS = [(0x0806A2E0, 0x4C), (0x0806A32C, 0x20), (0x086E7134, 0x1C), (0x082E33C0, 0xA0)]
DESCRIPTORS = 0x08700000
CONTEXT = 0x020199A0
STACK = 0x03002060


def pattern(seed, length):
    return struct.pack("<" + "I" * (length // 4), *[(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(length // 4)])


def synthetic_table():
    return struct.pack("<38H", *[index % 7 for index in range(19)], *[2 + index % 5 for index in range(19)])


def synthetic_audio():
    return struct.pack("<16H", *[3 + index % 4 for index in range(16)])


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    oracle.uc.mem_map(0x02000000, 0x40000)
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_R4, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC
    registers = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3]
    calls = []
    intercept_startup_update = False

    def hook(cpu, address, _size, _data):
        offset = address - control.ROM_BASE
        if offset in ARITY:
            arguments = [cpu.reg_read(registers[index]) for index in range(min(ARITY[offset], 4))]
            if ARITY[offset] == 5:
                arguments.append(struct.unpack("<I", cpu.mem_read(cpu.reg_read(UC_ARM_REG_SP), 4))[0])
            calls.append({"entryOffset": offset, "arguments": arguments})
        elif offset == 0x8168 and intercept_startup_update:
            calls.append({"entryOffset": offset, "arguments": [cpu.reg_read(UC_ARM_REG_R0)]})
        else:
            return
        cpu.reg_write(UC_ARM_REG_R0, 0)
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE, hook)
    intro = json.loads(spp_vectors.path("intro-vectors.json").read_text(encoding="utf8"))
    original_contexts = [struct.pack("<16H", *row) for row in intro["initializers"][0]["contextStates"]]
    prefix = intro["initializers"][0]["stackPrefix"]

    def resources(synthetic):
        for address, length in RESOURCE_SPANS:
            offset = address - control.ROM_BASE
            oracle.uc.mem_write(address, oracle.rom[offset:offset + length])
        if synthetic:
            oracle.uc.mem_write(0x0806A2E0, synthetic_table())
            oracle.uc.mem_write(0x0806A32C, synthetic_audio())
            oracle.uc.mem_write(0x086E7134, struct.pack("<7I", *[DESCRIPTORS + index * 8 for index in range(7)]))
            for index in range(39):
                oracle.uc.mem_write(DESCRIPTORS + index * 8, struct.pack("<4H", 0, 0, (index % 4) << 10, 0))

    def reset(case, synthetic=True):
        seed = case["seed"]
        oracle.uc.mem_write(0x02000000, pattern(seed, 0x40000))
        oracle.uc.mem_write(0x03000000, pattern(seed ^ 0xA55A1234, 0x8000))
        for index, original in enumerate(original_contexts):
            context = bytearray(original)
            if synthetic:
                struct.pack_into("<I", context, 12, DESCRIPTORS + index * 8)
            oracle.uc.mem_write(0x02001CA8 + index * 32, bytes(context))
        for address, value in [(0x02001CA4, case["state"]), (0x02002188, case["tile"]),
                               (0x0200218A, case["tileTimer"]), (0x0200218C, case["frame"]),
                               (0x0200218E, case["fade"]), (0x02002194, case["audioTimer"]),
                               (0x02002196, case["audioIndex"]), (STACK, case["depth"])]:
            oracle.uc.mem_write(address, struct.pack("<H", value))
        oracle.uc.mem_write(0x03000018, bytes([case["queueCount"]]))
        calls.clear()

    def snapshot():
        return {"ewramSha256": hashlib.sha256(oracle.uc.mem_read(0x02000000, 0x40000)).hexdigest(),
                "iwramWithoutStackSha256": hashlib.sha256(oracle.uc.mem_read(0x03000000, 0x7800)).hexdigest()}

    rng = random.Random(0x6824)
    resources(True)
    inputs = []

    def case(**changes):
        result = {"seed": rng.randrange(1 << 32), "state": 0, "tile": 0, "tileTimer": 0,
                  "frame": 0, "fade": 0, "audioTimer": 0, "audioIndex": 0, "depth": 2,
                  "queueCount": rng.choice([0, 120, 127, 128, 255]), "savedR4": CONTEXT}
        result.update(changes)
        inputs.append(result)

    for frame in [0, 58, 59, 60, 0xFFFF]:
        case(frame=frame)
    for tile in range(16):
        for timer in [0, 2 + tile % 5, 0xFFFF]:
            for audio_index in [0, 15, 16, 17]:
                case(state=1, tile=tile, tileTimer=timer, audioIndex=audio_index,
                     audioTimer=rng.choice([0, 2, 3, 0xFFFF]))
    for tile in [16, 17, 18]:
        for timer in [0, 2 + tile % 5, 0x8000]:
            for frame in [9, 30, 61]:
                case(state=2, tile=tile, tileTimer=timer, frame=frame)
    for frame in [0, 8, 9, 10, 28, 29, 30, 31, 38, 39, 40, 48, 49, 50, 60, 61, 62, 68, 69, 0xFFFF]:
        for fade in [0, 31, 32, 0xFFFF]:
            case(state=3, frame=frame, fade=fade)
    for state in [4, 99, 0x8000, 0xFFFF]:
        for depth in [0, 1, 10]:
            case(state=state, depth=depth)
    cases = []
    for item in inputs:
        reset(item)
        oracle.uc.reg_write(UC_ARM_REG_R4, item["savedR4"])
        oracle.call("sequence", rng.randrange(1 << 32))
        cases.append({**item, "calls": calls.copy(), **snapshot()})

    resources(False)
    initial = {"seed": rng.randrange(1 << 32), "state": 0, "tile": 0, "tileTimer": 0,
               "frame": 0, "fade": 0, "audioTimer": 0, "audioIndex": 0, "depth": 2,
               "queueCount": 0, "savedR4": CONTEXT}
    reset(initial, synthetic=False)
    oracle.uc.mem_write(0x02001CA0, struct.pack("<I", CONTEXT))
    oracle.uc.mem_write(CONTEXT, b"\0\0\0\0")
    oracle.uc.mem_write(0x03000E00, struct.pack("<H", 1))
    oracle.uc.mem_write(0x03000E04, struct.pack("<I", 0x080081E9))
    oracle.uc.mem_write(0x03000E2C, struct.pack("<H", 2))
    oracle.uc.mem_write(STACK + 4, struct.pack("<2I", 0x08006755, 0x08006825))
    oracle.uc.mem_write(STACK + 0x2C, b"\0\0\0\0")
    ticks = []
    max_audio_index = 0
    for _ in range(512):
        depth = struct.unpack("<H", oracle.uc.mem_read(STACK, 2))[0]
        callback = struct.unpack("<I", oracle.uc.mem_read(STACK + 4 + (depth - 1) * 4, 4))[0]
        calls.clear()
        oracle.call("queueReset")
        oracle.uc.reg_write(UC_ARM_REG_R4, CONTEXT)
        if callback == 0x08006825:
            oracle.call("sequence", CONTEXT)
        elif callback == 0x08006755:
            oracle.call("complete", CONTEXT)
        else:
            raise AssertionError("Unexpected callback in authentic sequence")
        max_audio_index = max(max_audio_index, struct.unpack("<H", oracle.uc.mem_read(0x02002196, 2))[0])
        ticks.append({"callbackId": callback, "calls": calls.copy(), **snapshot()})
        if struct.unpack("<I", oracle.uc.mem_read(CONTEXT, 4))[0]:
            break
    else:
        raise AssertionError("Authentic sequence exceeded512 ticks")
    calls.clear()
    intercept_startup_update = True
    oracle.call("startup")
    transition = {"calls": calls.copy(), **snapshot(),
                  "mainCallback": struct.unpack("<I", oracle.uc.mem_read(0x03000E04, 4))[0],
                  "auxiliaryDepth": struct.unpack("<H", oracle.uc.mem_read(STACK, 2))[0]}
    if transition["mainCallback"] != 0x080091C5 or transition["auxiliaryDepth"] != 0:
        raise AssertionError("Expected original startup next-screen replacement")

    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256,
            "scope": "Original6824/613A8/60E7C/663C/2614/2FA8 execute;236C/5724/5784/389C explicitly intercepted; synthetic cases patch only resource tables and descriptor priority halfwords; authentic local-ROM sequence uses real timing/descriptors; final81E8 transition intercepts already-run8168 update; no transform/audio/palette/PPU acceptance",
            "routines": {name: {"offset": start, "length": end - start, "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in ROUTINES.items()},
            "resourceSpans": [{"address": address, "length": length, "sha256": hashlib.sha256(oracle.rom[address - control.ROM_BASE:address - control.ROM_BASE + length]).hexdigest()} for address, length in RESOURCE_SPANS],
            "syntheticDescriptorBase": DESCRIPTORS, "constructorStackPrefix": prefix,
            "cases": cases,
            "authenticSequence": {"initial": initial, "maximumAudioIndex": max_audio_index,
                                  "ticks": ticks, "transition": transition}}


class IntroSequenceReferenceTests(unittest.TestCase):
    def test_fixture_exposes_synthetic_and_authentic_resource_boundaries(self):
        data = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"], control.ROM_SHA256)
        self.assertEqual(len(data["resourceSpans"]), 4)
        self.assertEqual(data["authenticSequence"]["maximumAudioIndex"], 16)
        self.assertEqual(data["authenticSequence"]["transition"]["mainCallback"], 0x080091C5)

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_original_sequence_regenerates_fixture(self):
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
        raise SystemExit("Native intro sequence fixture mismatch")
    print(json.dumps({"verified": True, "nativeCases": len(data["cases"]),
                      "authenticTicks": len(data["authenticSequence"]["ticks"]),
                      "nextCallback": data["authenticSequence"]["transition"]["mainCallback"]}))
