"""Execute original A3VE intro auxiliaries with explicit external boundaries.

Callback bodies and real 0x2F74/0x2FA8 stack helpers execute in Unicorn. Audio
and palette requests are intercepted and logged; 0x39A0 writes synthetic data of
the authentic declared size. Independent local LZ77 hashes do not prove BIOS,
audio, palette, PPU, or complete intro playback. Native bytes stay local.
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

FIXTURE = Path(__file__).with_name("intro-callbacks-vectors.json")
CALLBACKS = {
    "fadeIn": (0x66F8, 0x6754),
    "complete": (0x6754, 0x6764),
    "paletteFade": (0x6764, 0x67A0),
    "wait": (0x67A0, 0x67DC),
    "waitAndStopAudio": (0x67DC, 0x6824),
    "fadeOut": (0x6DF0, 0x6E40),
    "transition": (0x6E40, 0x6EB4),
    "loadBackground": (0x6EB4, 0x6EF4),
}
HELPERS = {"auxiliaryPush": (0x2F74, 0x2FA8), "auxiliaryPop": (0x2FA8, 0x2FBC)}
ARITY = {0x389C: 1, 0x5784: 3, 0x5EBD4: 2}
RESOURCES = [0x082D4D48, 0x082D5670]
CONTEXT = 0x020199A0
STACK = 0x03002060
SHADOW = 0x03000770
PLAYER_POINTER = 0x086DD64C
INSTALLED = [0x08006E41, 0x080067DD, 0x080066F9, 0x08006EB5,
             0x08006DF1, 0x080067A1, 0x080066F9, 0x08006765]


def pattern(seed, length):
    return struct.pack("<" + "I" * (length // 4), *[(seed + index * 0x01020304) & 0xFFFFFFFF for index in range(length // 4)])


def synthetic_resource(source, length):
    return bytes(((source >> 1) + index * 37) & 255 for index in range(length))


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(CALLBACKS)
    oracle.routines.update(HELPERS)
    oracle.uc.mem_map(0x02000000, 0x40000)
    oracle.uc.mem_map(0x06000000, 0x20000)
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_LR, UC_ARM_REG_PC
    registers = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2]
    calls, resources, boundaries = [], [], []

    def half(address):
        return struct.unpack("<H", oracle.uc.mem_read(address, 2))[0]

    def hook(cpu, address, _size, _data):
        offset = address - control.ROM_BASE
        if offset in ARITY:
            calls.append({"entryOffset": offset, "arguments": [cpu.reg_read(registers[index]) for index in range(ARITY[offset])]})
            boundaries.append({"depth": half(STACK), "timer": half(CONTEXT + 4)})
        elif offset == 0x39A0:
            destination, source = [cpu.reg_read(registers[index]) for index in range(2)]
            raw = oracle.rom[source - control.ROM_BASE:]
            if raw[0] != 0x10:
                raise AssertionError("Intro auxiliary resource must be type10")
            length = int.from_bytes(raw[1:4], "little")
            cpu.mem_write(destination, synthetic_resource(source, length))
            resources.append({"sourceAddress": source, "destination": destination, "length": length})
        else:
            return
        cpu.reg_write(UC_ARM_REG_R0, 0)
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(0x66F8)

    def reset(seed, depth, parameter, timer, display):
        oracle.uc.mem_write(0x02000000, pattern(seed, 0x40000))
        oracle.uc.mem_write(0x03000000, pattern(seed ^ 0xA55A1234, 0x8000))
        oracle.uc.mem_write(0x06000000, pattern(seed ^ 0x7ACE1200, 0x20000))
        oracle.uc.mem_write(STACK, struct.pack("<H", depth))
        oracle.uc.mem_write(STACK + 0x2C + (depth - 1) * 2, struct.pack("<H", parameter))
        oracle.uc.mem_write(CONTEXT + 4, struct.pack("<H", timer))
        oracle.uc.mem_write(SHADOW + 4, struct.pack("<H", display))
        calls.clear()
        resources.clear()
        boundaries.clear()

    def snapshot():
        return {"ewramSha256": hashlib.sha256(oracle.uc.mem_read(0x02000000, 0x40000)).hexdigest(),
                "iwramWithoutStackSha256": hashlib.sha256(oracle.uc.mem_read(0x03000000, 0x7800)).hexdigest(),
                "vramTransactionWindowSha256": hashlib.sha256(oracle.uc.mem_read(0x06000000, 0x20000)).hexdigest()}

    cases = []
    for name in CALLBACKS:
        parameters = [0, 7, 8, 15, 16, 17, 31, 32, 0x7FFF, 0x8000, 0xFFFF]
        timers = [120]
        displays = [0x1F00]
        if name in ["wait", "waitAndStopAudio"]:
            parameters = [0, 1, 0x8000, 0xFFFF]
            timers = [0, 1, 2, 0x7FFF, 0x8000, 0xFFFF]
        if name == "complete":
            parameters = [0]
        if name == "loadBackground":
            parameters = [0]
            displays = [((bank << 2) | (screen << 8)) for bank in range(4) for screen in [0, 1, 30, 31]]
        for depth in [1, 2, 9, 10]:
            for parameter in parameters:
                for timer in timers:
                    for display in displays:
                        seed = rng.randrange(1 << 32)
                        reset(seed, depth, parameter, timer, display)
                        oracle.call(name, CONTEXT)
                        cases.append({"routine": name, "seed": seed, "depth": depth,
                                      "parameter": parameter, "timer": timer, "display": display,
                                      "calls": calls.copy(), "callBoundaries": boundaries.copy(),
                                      "resourceRequests": resources.copy(), **snapshot()})

    seed = rng.randrange(1 << 32)
    reset(seed, len(INSTALLED), 0, 120, 0x1F00)
    for index, callback in enumerate(INSTALLED):
        oracle.uc.mem_write(STACK + 4 + index * 4, struct.pack("<I", callback))
        oracle.uc.mem_write(STACK + 0x2C + index * 2, b"\0\0")
    by_id = {control.ROM_BASE + start + 1: name for name, (start, _end) in CALLBACKS.items()}
    ticks = []
    for _ in range(512):
        depth = half(STACK)
        if not depth:
            break
        callback = struct.unpack("<I", oracle.uc.mem_read(STACK + 4 + (depth - 1) * 4, 4))[0]
        if callback not in by_id:
            break
        calls.clear()
        resources.clear()
        boundaries.clear()
        oracle.call(by_id[callback], CONTEXT)
        ticks.append({"callbackId": callback, "calls": calls.copy(), "callBoundaries": boundaries.copy(),
                      "resourceRequests": resources.copy(), **snapshot()})
    else:
        raise AssertionError("Intro sequence exceeded512 ticks")
    stopped = struct.unpack("<I", oracle.uc.mem_read(STACK + 4 + (half(STACK) - 1) * 4, 4))[0]
    if stopped != 0x08006825:
        raise AssertionError("Expected exact unported animation callback boundary")

    actual_resources = []
    for address in RESOURCES:
        offset = address - control.ROM_BASE
        decoded, consumed = reference_parity.decode(oracle.rom[offset:], maximum=0x20000)
        actual_resources.append({"sourceAddress": address, "offset": offset, "storedLength": consumed,
                                 "storedSha256": hashlib.sha256(oracle.rom[offset:offset + consumed]).hexdigest(),
                                 "decodedLength": len(decoded), "decodedSha256": hashlib.sha256(decoded).hexdigest()})
    word_offset = PLAYER_POINTER - control.ROM_BASE
    return {"schemaVersion": 1, "romSha256": control.ROM_SHA256,
            "scope": "Original8 callback bodies and2F74/2FA8 stack helpers execute;389C/5784/5EBD4 intercepted and logged;39A0 intercepted with synthetic same-length output; transaction-window hashes do not prove native VRAM/PPU behavior",
            "routines": {name: {"offset": start, "length": end - start, "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in {**CALLBACKS, **HELPERS}.items()},
            "interceptedDirectEntries": sorted(ARITY), "cases": cases,
            "authenticLz77Resources": actual_resources,
            "audioPlayerWord": {"address": PLAYER_POINTER, "length": 4,
                                "sha256": hashlib.sha256(oracle.rom[word_offset:word_offset + 4]).hexdigest(),
                                "value": struct.unpack_from("<I", oracle.rom, word_offset)[0]},
            "sequence": {"seed": seed, "installed": INSTALLED, "timer": 120, "display": 0x1F00,
                         "ticks": ticks, "stoppedAtCallback": stopped}}


class IntroCallbacksReferenceTests(unittest.TestCase):
    def test_fixture_declares_native_and_intercepted_boundaries(self):
        data = json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"], control.ROM_SHA256)
        self.assertEqual(len(data["routines"]), 10)
        self.assertEqual(len(data["authenticLz77Resources"]), 2)
        self.assertEqual(data["sequence"]["stoppedAtCallback"], 0x08006825)
        self.assertTrue(all(1 <= case["depth"] <= 10 for case in data["cases"]))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_actual_native_callbacks_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]), json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("--rom or SPP_ROM is required")
    result = generate(args.rom)
    if args.write:
        args.write.write_text(json.dumps(result, separators=(",", ":")) + "\n", encoding="utf8")
    elif result != json.loads(FIXTURE.read_text(encoding="utf8")):
        raise SystemExit("Native intro callback fixture mismatch")
    print(json.dumps({"verified": True, "nativeCases": len(result["cases"]),
                      "sequenceTicks": len(result["sequence"]["ticks"]), "boundary": result["scope"]}))
