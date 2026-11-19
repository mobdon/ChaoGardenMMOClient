"""Execute A3VE animation init/dispatch/handlers with synthetic command streams.

Original ROM instructions run without dependency interception. The dispatcher
calls actual known native handlers via its actual Thumb thunk. Fixtures contain
synthetic object halfwords, scalar handler IDs and synthetic descriptors only.
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

FIXTURE = Path(__file__).with_name("animation-vectors.json")
OBJECT = 0x03005000
COMMANDS = 0x03003000
SIZE = 0x58
ROUTINES = {"initialize": (0x5B9C, 0x5BD0), "dispatch": (0x5BD0, 0x5BF4),
            "end": (0x5BF4, 0x5C00), "frame": (0x5C00, 0x5C30),
            "blank": (0x5C30, 0x5C60), "immediate": (0x5C60, 0x5C74),
            "wait": (0x5C74, 0x5C9C), "loopBegin": (0x5C9C, 0x5CC8),
            "loopEnd": (0x5CC8, 0x5D08)}


def handler(name):
    return control.ROM_BASE + ROUTINES[name][0] + 1


def halfwords(data):
    return list(struct.unpack("<" + "H" * (len(data) // 2), data))


def state(rng):
    return bytearray(rng.randrange(256) for _ in range(SIZE))


def put_word(data, offset, value):
    struct.pack_into("<I", data, offset, value & 0xFFFFFFFF)


def put_half(data, offset, value):
    struct.pack_into("<H", data, offset, value & 65535)


def command(name, *arguments):
    return [handler(name), *arguments]


def generate(path):
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    rng = random.Random(0x5BD0)
    initializers = []
    for _ in range(24):
        initial = state(rng)
        oracle.uc.mem_write(OBJECT, bytes(initial))
        oracle.call("initialize", OBJECT)
        initializers.append({"initial": halfwords(initial), "final": halfwords(oracle.uc.mem_read(OBJECT, SIZE))})
    requests = []
    for _ in range(8):
        requests.append(("end", 0, 0, 0))
        requests.append(("immediate", 0, 0, 0))
    for name in ["frame", "blank", "wait"]:
        for duration in [0, 1, 2, 0x7FFF, 0x8000, 0xFFFF]:
            for counter in [0, 1, 0x7FFF, 0x8000, 0xFFFF]:
                requests.append((name, duration, counter, 0))
    for depth in [0, 1, 2, 3, 4, 255]:
        for repetitions in [0, 1, 2, 0x7FFF, 0x8000, 0xFFFF]:
            requests.append(("loopBegin", repetitions, 0, depth))
    for depth in range(5):
        for repetitions in [0, 1, 2, 3, 0x80000000, 0xFFFFFFFF]:
            requests.append(("loopEnd", repetitions, 0, depth))
    from unicorn.arm_const import UC_ARM_REG_R0
    handlers = []
    for name, argument, counter, depth in requests:
        initial = state(rng)
        put_word(initial, 0x34, COMMANDS)
        put_half(initial, 0x18, counter)
        if name in ["loopBegin", "loopEnd"]:
            initial[0x13] = depth
        descriptor = rng.randrange(1 << 32)
        attributes = rng.randrange(1 << 32)
        if name == "frame":
            words = command(name, descriptor, argument | ((attributes & 65535) << 16))
        elif name == "immediate":
            words = command(name, descriptor, attributes)
        elif name in ["blank", "wait", "loopBegin"]:
            words = command(name, argument)
        else:
            words = command(name)
        if name == "loopEnd" and depth:
            put_word(initial, 0x38 + (depth - 1) * 8, COMMANDS + 0x100)
            put_word(initial, 0x3C + (depth - 1) * 8, argument)
        raw = struct.pack("<" + "I" * len(words), *words)
        oracle.uc.mem_write(OBJECT, bytes(initial))
        oracle.uc.mem_write(COMMANDS, raw)
        oracle.call(name, OBJECT)
        handlers.append({"name": name + "-" + str(argument) + "-" + str(counter) + "-" + str(depth),
                         "handler": handler(name), "initial": halfwords(initial), "commands": halfwords(raw),
                         "continue": oracle.uc.reg_read(UC_ARM_REG_R0) != 0,
                         "final": halfwords(oracle.uc.mem_read(OBJECT, SIZE))})
    scripts = []
    for duration in [0, 1, 2, 3, 0xFFFF, 0x8000]:
        scripts.append(("timed-frame-" + str(duration), command("frame", 0xDEAD1000, duration | 0xA55A0000) + command("end"), 8, 0, 0, 1))
    scripts.append(("immediate-frame-then-end", command("immediate", 0xFEDC3000, 0xDEAD8000) + command("end"), 3, 0, 0, 1))
    scripts.append(("mixed-frames-waits-blank", command("immediate", 0x12345678, 0xA55A) + command("wait", 2) + command("blank", 2) + command("frame", 0xFEDCBA98, 1) + command("end"), 12, 0, 0, 1))
    for repetitions in [0, 1, 2, 3, 0xFFFF, 0x8000]:
        scripts.append(("loop-" + str(repetitions), command("loopBegin", repetitions) + command("frame", 0xAABBCCDD, 1) + command("loopEnd") + command("end"), 12, 0, 0, 1))
    scripts.append(("four-nested-finite-loops", command("loopBegin", 2) * 4 + command("frame", 0x12349876, 1) + command("loopEnd") * 4 + command("end"), 24, 0, 0, 1))
    scripts.append(("signed-timer-wrap", command("frame", 0xABCDEF01, 1) + command("end"), 4, 0x7FFF, 0, 1))
    for flags in [0x80000000, 0xFFFFFFFF]:
        scripts.append(("inactive-negative-flags-" + str(flags), [], 2, 0, flags, 1))
    scripts.append(("inactive-null-script", [], 2, 0, 0, 0))
    # Install the observer before compiling any handler blocks. A fresh
    # instance prevents Unicorn's already-cached direct-handler blocks from
    # bypassing a subsequently added code observer.
    oracle = control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    from unicorn import UC_HOOK_CODE
    entries = {control.ROM_BASE + start for name, (start, _end) in ROUTINES.items() if name not in ["initialize", "dispatch"]}
    trace = []

    def code_event(_uc, address, _size, _data):
        if address in entries:
            trace.append(address)

    hook = oracle.uc.hook_add(UC_HOOK_CODE, code_event, begin=control.ROM_BASE + 0x5BF4, end=control.ROM_BASE + 0x5D07)
    dispatchers = []
    for name, words, ticks, timer, flags, script in scripts:
        initial = state(rng)
        put_word(initial, 0x0C, flags)
        put_word(initial, 0x30, COMMANDS if script else 0)
        put_word(initial, 0x34, COMMANDS)
        put_half(initial, 0x18, timer)
        initial[0x13] = 0
        raw = struct.pack("<" + "I" * len(words), *words)
        oracle.uc.mem_write(COMMANDS, bytes(0x1000))
        if raw:
            oracle.uc.mem_write(COMMANDS, raw)
        oracle.uc.mem_write(OBJECT, bytes(initial))
        snapshots = []
        command_counts = []
        for _ in range(ticks):
            trace.clear()
            oracle.call("dispatch", OBJECT)
            command_counts.append(len(trace))
            snapshots.append(halfwords(oracle.uc.mem_read(OBJECT, SIZE)))
        dispatchers.append({"name": name, "initial": halfwords(initial), "commands": halfwords(raw),
                            "snapshots": snapshots, "commandCounts": command_counts})
    oracle.uc.hook_del(hook)
    return {"schemaVersion": 1, "sourceSha256": control.ROM_SHA256,
            "scope": "Original initializer, dispatcher thunk and seven handlers execute without stubs; all command streams and object bytes are synthetic; unknown handlers are unported",
            "commandBase": COMMANDS,
            "routines": {name: {"offset": start, "length": end-start,
                                 "sha256": hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name, (start, end) in ROUTINES.items()},
            "initializers": initializers, "handlers": handlers, "dispatchers": dispatchers}


class AnimationReferenceTests(unittest.TestCase):
    def test_fixture_covers_actual_dispatcher_and_signed_native_boundaries(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(data["sourceSha256"], control.ROM_SHA256)
        self.assertEqual(len(data["initializers"]), 24)
        self.assertGreaterEqual(len(data["handlers"]), 170)
        self.assertGreaterEqual(len(data["dispatchers"]), 19)
        self.assertTrue(any(max(case["commandCounts"]) >= 5 for case in data["dispatchers"]))
        self.assertTrue(any(max(case["commandCounts"]) == 0 for case in data["dispatchers"]))

    @unittest.skipUnless(os.environ.get("SPP_ROM"), "Requires local SPP_ROM and Python unicorn")
    def test_native_machine_code_regenerates_complete_object_snapshots(self):
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
        raise SystemExit("Native animation parity mismatch")
    print(json.dumps({"verified": True, "nativeInitializers": len(data["initializers"]), "nativeHandlers": len(data["handlers"]), "nativeDispatcherTicks": sum(len(case["snapshots"]) for case in data["dispatchers"])}))
