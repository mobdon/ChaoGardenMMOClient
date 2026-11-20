#!/usr/bin/env python3
"""Execute isolated A3VE palette routines against synthetic test inputs only.

Unicorn is a development reference oracle, never part of the Rust runtime.
The only BIOS hook is SWI 0x0b's non-fill, halfword CpuSet copy. Machine code
and ROM palette data are read locally and are never included in the vectors.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import spp_vectors  # noqa: E402


ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROM_BASE = 0x08000000
SOURCE = 0x02010000
REFERENCE = 0x02011000
OUTPUT = 0x02012000
COPY_INPUT = 0x02013000
CONTEXT = 0x02001C50
PREVIOUS = CONTEXT + 12
EFFECTS = CONTEXT + 44
INHIBIT = 0x020319A0
STACK = 0x03007000
RETURN = 0x09000000
SPANS = [
    ("blend", 0x19D0, 0x1AA8),
    ("initialize", 0x5618, 0x5668),
    ("process", 0x5668, 0x5704),
    ("configure", 0x5714, 0x5724),
    ("copySource", 0x5724, 0x5784),
    ("setEffects", 0x5784, 0x57C0),
    ("markDirty", 0x57C0, 0x57FC),
    ("cpuSetWrapper", 0x5FEB4, 0x5FEB8),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def words(seed, count=512):
    state = seed & 0xFFFFFFFF
    result = []
    for _ in range(count):
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        state &= 0xFFFFFFFF
        result.append(state & 0xFFFF)
    return result


def packed(values):
    return struct.pack(f"<{len(values)}H", *values)


class NativeOracle:
    def __init__(self, rom):
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_INTR
        from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.uc.mem_map(ROM_BASE, 0x800000)
        self.uc.mem_write(ROM_BASE, rom)
        self.uc.mem_map(0x02000000, 0x40000)
        self.uc.mem_map(0x03000000, 0x8000)
        self.uc.mem_map(RETURN, 0x1000)
        self.uc.mem_write(0x03000E44, struct.pack("<I", OUTPUT))
        self.calls = 0
        self.cpu_sets = 0
        self.failure = None

        def interrupt(uc, number, _):
            from unicorn.arm_const import UC_ARM_REG_PC
            pc = uc.reg_read(UC_ARM_REG_PC)
            instruction = int.from_bytes(uc.mem_read(pc - 2, 2), "little")
            control = uc.reg_read(UC_ARM_REG_R2)
            if number != 2 or instruction != 0xDF0B or control > 512:
                self.failure = f"unsupported BIOS boundary: intr={number}, insn={instruction:x}, control={control:x}"
                uc.emu_stop()
                return
            source = uc.reg_read(UC_ARM_REG_R0)
            destination = uc.reg_read(UC_ARM_REG_R1)
            # CpuSet control has no fill/32-bit flags on this native path.
            for index in range(control):
                uc.mem_write(destination + index * 2, bytes(uc.mem_read(source + index * 2, 2)))
            self.cpu_sets += 1

        self.uc.hook_add(UC_HOOK_INTR, interrupt)

    def call(self, entry, arguments=(), fifth=0):
        from unicorn.arm_const import (
            UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3,
            UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC,
        )
        for index, register in enumerate((UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3)):
            self.uc.reg_write(register, arguments[index] if index < len(arguments) else 0)
        self.uc.reg_write(UC_ARM_REG_SP, STACK)
        self.uc.reg_write(UC_ARM_REG_LR, RETURN | 1)
        self.uc.mem_write(STACK, struct.pack("<I", fifth))
        self.failure = None
        self.uc.emu_start(ROM_BASE + entry + 1, RETURN, count=250000)
        if self.failure:
            raise ValueError(self.failure)
        if self.uc.reg_read(UC_ARM_REG_PC) != RETURN:
            raise ValueError(f"native palette routine {entry:x} exhausted instruction budget")
        if self.uc.reg_read(UC_ARM_REG_SP) != STACK:
            raise ValueError("native routine did not restore stack")
        self.calls += 1

    def initialize(self, seed):
        self.uc.mem_write(SOURCE, packed(words(seed)))
        self.uc.mem_write(OUTPUT, packed(words(seed ^ 0xA5A5A5A5)))
        self.call(0x5618, [SOURCE])


def generate(rom):
    native = NativeOracle(rom)
    color_vectors = []
    colors = [0, 1, 0x1F, 0x20, 0x3E0, 0x400, 0x7C00, 0x4210, 0x7FFF, 0x8000, 0xFFFF]
    factors = [*range(34), 63, 64, 255, 0xFFFF, 0x10000, 0x1001F, 0x10020]
    for index, source in enumerate(colors + words(0x41B3, 13)):
        reference = (colors + words(0x4C55, 13))[-index - 1]
        for factor in factors:
            native.uc.mem_write(SOURCE, packed([source]))
            native.uc.mem_write(REFERENCE, packed([reference]))
            native.uc.mem_write(OUTPUT, packed([0xCAFE]))
            native.call(0x19D0, [SOURCE, REFERENCE, 0, 1], factor)
            output = struct.unpack("<H", native.uc.mem_read(OUTPUT, 2))[0]
            color_vectors.append({"source": source, "reference": reference, "factor": factor, "output": output})
    setter_vectors = []
    for start, count, coefficient in [(0, 32, 32), (12, 1, 64), (31, 1, 255), (256, 1, 0x10021), (0x1000C, 0x101, 0x10040), (255, 0, 7), (0, 256, 7)]:
        initial = [(i * 19 + 3) & 255 for i in range(32)]
        native.uc.mem_write(EFFECTS, bytes(initial))
        native.call(0x5784, [start, count, coefficient])
        setter_vectors.append({"start": start, "count": count, "coefficient": coefficient,
                               "before": initial, "after": list(native.uc.mem_read(EFFECTS, 32))})
    copy_vectors = []
    for seed, start, count in [(0x4321, 0, 32), (0x4322, 15, 16), (0x4323, 511, 1), (0x4324, 0x1000F, 0x10010), (0x4325, 0xFFFF, 0)]:
        native.initialize(seed)
        previous = [(index * 7 + 1) & 255 for index in range(32)]
        native.uc.mem_write(PREVIOUS, bytes(previous))
        native.uc.mem_write(COPY_INPUT, packed(words(seed ^ 0x55CC33AA)))
        native.call(0x5724, [COPY_INPUT, start, count])
        copy_vectors.append({"seed": seed, "start": start, "count": count, "previousBefore": previous,
                             "previousAfter": list(native.uc.mem_read(PREVIOUS, 32)),
                             "sourceSha256": digest(native.uc.mem_read(SOURCE, 1024))})
    update_vectors = []
    coefficients = [0, 1, 16, 31, 32, 33, 48, 63, 64, 65, 127, 128, 255]
    for case in range(12):
        seed = 0x6531 + case
        native.initialize(seed)
        effects = [coefficients[(index + case) % len(coefficients)] for index in range(32)]
        previous = [255 if case % 3 == 0 else effects[index] if index % 3 == 0 else (effects[index] + 1) & 255 for index in range(32)]
        inhibit = 1 if case == 10 else 0xFFFF if case == 11 else 0
        enabled = case != 9
        native.uc.mem_write(EFFECTS, bytes(effects))
        native.uc.mem_write(PREVIOUS, bytes(previous))
        native.uc.mem_write(INHIBIT, struct.pack("<H", inhibit))
        if not enabled:
            native.uc.mem_write(CONTEXT, struct.pack("<I", 0))
        native.call(0x5668)
        previous_after = list(native.uc.mem_read(PREVIOUS, 32))
        update_vectors.append({"seed": seed, "effects": effects, "previousBefore": previous,
                               "inhibit": inhibit, "enabled": enabled, "previousAfter": previous_after,
                               "changedBanks": sum(a != b for a, b in zip(previous, previous_after)),
                               "outputSha256": digest(native.uc.mem_read(OUTPUT, 1024))})
    return {
        "schemaVersion": 1, "romSha256": ROM_SHA256,
        "oracle": "isolated original Thumb routines in Unicorn; BIOS CpuSet halfword-copy hook only",
        "sourceSpans": [{"name": name, "offset": start, "length": end - start, "sha256": digest(rom[start:end])} for name, start, end in SPANS],
        "nativeCalls": native.calls, "biosCpuSetCalls": native.cpu_sets,
        "colorVectors": color_vectors, "setterVectors": setter_vectors,
        "copyVectors": copy_vectors, "updateVectors": update_vectors,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--python-modules", type=Path)
    parser.add_argument("--check", action="store_true", help="compare the checked-in synthetic scalar vectors")
    args = parser.parse_args()
    if args.python_modules:
        sys.path.insert(0, str(args.python_modules.resolve(strict=True)))
    if args.rom.stat().st_size != 0x800000:
        raise ValueError("wrong A3VE ROM length")
    with args.rom.open("rb") as handle:
        rom = handle.read(0x800001)
    if len(rom) != 0x800000 or digest(rom) != ROM_SHA256:
        raise ValueError("wrong A3VE ROM identity")
    if rom[0x67120:0x67520] != bytes(1024) or rom[0x67520:0x67920] != packed([0x7FFF] * 512):
        raise ValueError("native default black/white palette references differ")
    vectors = generate(rom)
    destination = spp_vectors.path("palette-vectors.json")
    if args.check:
        if json.loads(destination.read_text(encoding="utf-8")) != vectors:
            raise ValueError("native palette scalar vectors differ")
    else:
        destination.write_text(json.dumps(vectors, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"nativeCalls": vectors["nativeCalls"], "biosCpuSetCalls": vectors["biosCpuSetCalls"],
                      "colorVectors": len(vectors["colorVectors"]), "setterVectors": len(vectors["setterVectors"]),
                      "copyVectors": len(vectors["copyVectors"]), "updateVectors": len(vectors["updateVectors"]),
                      "scalarVectorSha256": digest(json.dumps(vectors, sort_keys=True, separators=(",", ":")).encode())}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, ImportError) as error:
        print(f"palette reference failed: {error}", file=sys.stderr)
        sys.exit(1)
