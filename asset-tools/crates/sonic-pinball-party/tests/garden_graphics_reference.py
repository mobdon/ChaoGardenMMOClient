"""Execute the authenticated A3VE Garden graphics routines as a native oracle.

The original payload 163C/1718/D294 instructions execute in Unicorn. CpuFastSet
and LZ77 BIOS boundaries are explicitly intercepted, as are the three strict
Garden tail dependencies. Synthetic vectors contain no native resource bytes.
Authentic local cases use the independent bounded Python codec and publish
only source/output hashes. This is static scene parity, not a boot/PPU test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import struct
import unittest

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
    UC_ARM_REG_R3, UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7,
    UC_ARM_REG_R8, UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11,
    UC_ARM_REG_R12, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC)
import reference_parity

BASE = 0x02000000
SHA256 = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
LENGTH = 0x2E12C
SENTINEL = 0x03007F00
FIXTURE = Path(__file__).with_name("garden-graphics-vectors.json")
ROUTINES = [("registerScript", 0x163C, 0x34, "b072578615dc6e52b9acc3cc116b5807560ecabb5f19376ff2982e2ae48488be"),
    ("mapStamp", 0x1718, 0x48, "1ea0a9a7a33516d2b16df64a8f24309fc9dbfa4d70870ea0d51d93b73b6ae9e8"),
    ("initialize", 0xD294, 0x270, "d43e126197c66c6f15fbe3927eff5c0f6f76a1ff491c772339dfe158c6b0a7aa")]
RAW = [("objectPalette", 0x1606C, 224), ("backgroundPalette", 0x17074, 416),
    ("mapFirst", 0x2DD64, 60), ("mapFirstAlternate", 0x2DDA0, 24),
    ("mapSecond", 0x2DDB8, 54), ("mapThird", 0x2DDEE, 48),
    ("mapLeft", 0x2DE1E, 80), ("mapRight", 0x2DE6E, 320),
    ("registerTable", 0x2E074, 64)]
LZ = [("backgroundTilesLz77", 0x17214), ("objectTilesLz77", 0x1614C)]
WINDOWS = [("source", 0x02030000, 0x10000, 0xEABBCCDD),
    ("iwram", 0x03000000, 0x8000, 0xA55A1234),
    ("mmio", 0x04000000, 0x10000, 0x0400),
    ("palette", 0x05000000, 0x1000, 0x0500),
    ("vram", 0x06000000, 0x20000, 0x0600)]
REGISTERS = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3,
    UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_R8,
    UC_ARM_REG_R9, UC_ARM_REG_R10, UC_ARM_REG_R11, UC_ARM_REG_R12]


def pattern(seed, length):
    words = [(seed + i * 0x01020304) & 0xFFFFFFFF for i in range((length + 3) // 4)]
    return struct.pack("<" + "I" * len(words), *words)[:length]


def register_table(seed, count):
    return b"".join(struct.pack("<HH", (index * 0x12) & 0xFFFE,
        (seed + index * 0x123) & 0xFFFF) for index in range(count))


class Oracle:
    def __init__(self, payload):
        if len(payload) != LENGTH or hashlib.sha256(payload).hexdigest() != SHA256:
            raise ValueError("expected exact decoded A3VE Garden payload")
        self.payload = payload
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, length in [(BASE, 0x40000), (0x03000000, 0x8000),
                (0x04000000, 0x10000), (0x05000000, 0x1000), (0x06000000, 0x20000)]:
            self.uc.mem_map(address, length)
        # Install hooks before any block is translated.
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, cpu, address, _size, _data):
        if address == SENTINEL:
            cpu.emu_stop()
            return
        offset = address - BASE
        if offset == 0x11AE0:
            source, destination, control = [cpu.reg_read(r) for r in REGISTERS[:3]]
            count = control & 0x1FFFFF
            if count % 8:
                raise ValueError("oracle only covers aligned CpuFastSet word counts")
            fill = bool(control & (1 << 24))
            data = bytes(cpu.mem_read(source, 4)) * count if fill else bytes(cpu.mem_read(source, count * 4))
            self.bios.append({"kind": "cpuFastSet", "destination": destination,
                "control": control, **({"fillWord": int.from_bytes(data[:4], "little")} if fill else {"source": source})})
            cpu.mem_write(destination, data)
        elif offset == 0x11AF4:
            source, destination = [cpu.reg_read(r) for r in REGISTERS[:2]]
            if self.case.get("authentic", False):
                data, _ = reference_parity.decode(self.payload[source - BASE:])
            else:
                length = {0x02017214: 17760, 0x0201614C: 10848}[source]
                data = pattern(self.case["seed"] ^ source, length)
            self.bios.append({"kind": "lz77Vram", "source": source,
                "destination": destination, "decodedLength": len(data)})
            cpu.mem_write(destination, data)
        elif offset in [0x0DB0, 0x10698, 0xD504]:
            self.calls.append({"offset": offset, "arguments": [cpu.reg_read(UC_ARM_REG_R0)] if offset == 0x10698 else []})
        else:
            return
        # These declared boundaries are test-only no-ops/copies, not native
        # implementations of the BIOS or unported Garden subsystems.
        cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    def run(self, case):
        self.case = case
        cpu = self.uc
        cpu.mem_write(BASE, self.payload)
        for _, address, length, salt in WINDOWS:
            cpu.mem_write(address, pattern(case["seed"] ^ salt, length))
        cpu.mem_write(0x03004260, bytes([case.get("gardenFlag", 0)]))
        if case["routine"] == "initialize":
            if not case.get("authentic", False):
                for _, offset, length in RAW:
                    data = register_table(case["seed"], 16) if offset == 0x2E074 else pattern(case["seed"] ^ (BASE + offset), length)
                    cpu.mem_write(BASE + offset, data)
            arguments = []
        elif case["routine"] == "registerScript":
            cpu.mem_write(0x02030000, register_table(case["seed"], 255))
            arguments = [case["source"], case["count"]]
        else:
            arguments = [case["source"], case["destination"], case["width"], case["height"]]
        for register in REGISTERS:
            cpu.reg_write(register, 0)
        for register, value in zip(REGISTERS, arguments):
            cpu.reg_write(register, value)
        cpu.reg_write(UC_ARM_REG_SP, 0x03007E00)
        cpu.reg_write(UC_ARM_REG_LR, SENTINEL | 1)
        cpu.mem_write(0x03007E00, struct.pack("<I", case.get("delta", 0)))
        self.calls, self.bios = [], []
        entry = next(offset for name, offset, _, _ in ROUTINES if name == case["routine"])
        cpu.emu_start(BASE + entry + 1, SENTINEL, count=500000)
        if cpu.reg_read(UC_ARM_REG_PC) != SENTINEL:
            raise RuntimeError("bounded native Garden routine did not return")
        output = {"calls": self.calls.copy(), "bios": self.bios.copy()}
        for name, address, length, _ in WINDOWS:
            output[name + "Sha256"] = hashlib.sha256(cpu.mem_read(address, 0x7800 if name == "iwram" else length)).hexdigest()
        return {**case, **output}


def generate(payload):
    oracle = Oracle(payload)
    routines = []
    for name, offset, length, digest in ROUTINES:
        if hashlib.sha256(payload[offset:offset + length]).hexdigest() != digest:
            raise ValueError("routine mismatch")
        routines.append({"name": name, "offset": offset, "length": length, "sha256": digest})
    resources = [{"name": name, "offset": offset, "length": length,
        "sha256": hashlib.sha256(payload[offset:offset + length]).hexdigest()} for name, offset, length in RAW]
    for name, offset in LZ:
        data, consumed = reference_parity.decode(payload[offset:])
        resources.append({"name": name, "offset": offset, "length": consumed,
            "sha256": hashlib.sha256(payload[offset:offset + consumed]).hexdigest(),
            "decodedLength": len(data), "decodedSha256": hashlib.sha256(data).hexdigest()})
    rng = random.Random(0xD2941718)
    cases = []
    for count in [0, 1, 2, 16, 127, 254, 255, 256, 257, 0xFFFFFFFF]:
        cases.append({"routine": "registerScript", "seed": rng.randrange(1 << 32), "source": 0x02030000, "count": count})
    for width in [0, 1, 2, 6, 31, 32, 33, 64, 255, 256, 257, 0xFFFFFFFF]:
        for height in [0, 1, 2, 8, 256, 257]:
            cases.append({"routine": "mapStamp", "seed": rng.randrange(1 << 32), "source": 0x02030000,
                "destination": 0x0600C000, "width": width, "height": height,
                "delta": rng.choice([0, 1, 0x61B6, 0xFFFF, 0x10000, 0xFFFFFFFF])})
    for index in range(64):
        cases.append({"routine": "mapStamp", "seed": rng.randrange(1 << 32), "source": 0x02030000,
            "destination": [0x0600C000, 0x02030002, 0x02030000, 0x0202FFFE][index % 4],
            "width": rng.randrange(1, 65), "height": rng.randrange(1, 9), "delta": rng.randrange(1 << 32)})
    # One original-size maximum row and long height, while keeping all source
    # and destination transactions inside the declared mapped apertures.
    cases.append({"routine": "mapStamp", "seed": 0x98765432, "source": 0x02030000,
        "destination": 0x0600C000, "width": 255, "height": 128, "delta": 0xFFFF})
    for flag in [0, 1, 2, 255]:
        for _ in range(4):
            cases.append({"routine": "initialize", "seed": rng.randrange(1 << 32), "gardenFlag": flag})
    authentic = [{"routine": "initialize", "seed": 0x55AA1234, "gardenFlag": flag, "authentic": True} for flag in [0, 1]]
    return {"schemaVersion": 1, "romSha256": reference_parity.ROM_SHA256, "space": "garden", "payloadLength": LENGTH, "payloadSha256": SHA256,
        "oracle": "Unicorn 2.1.4; original Garden register/map/scene instructions; declared BIOS copy/decode and strict tail-call test doubles; no PPU, boot or end-to-end claim",
        "routines": routines, "resources": resources, "cases": [oracle.run(case) for case in cases],
        "authenticCases": [oracle.run(case) for case in authentic]}


def generate_from_rom(path):
    rom = reference_parity.bounded_read(Path(path), reference_parity.ROM_LENGTH)
    reference_parity.validate_rom(rom)
    payload, consumed = reference_parity.decode(rom[reference_parity.GARDEN_OFFSET:], LENGTH)
    if consumed != reference_parity.GARDEN_STORED:
        raise ValueError("Garden container consumed length differs")
    return generate(payload)


class GardenGraphicsReferenceTests(unittest.TestCase):
    def test_fixture_has_source_domain_and_boundaries(self):
        data = json.loads(FIXTURE.read_text())
        self.assertEqual(data["space"], "garden")
        self.assertEqual(data["payloadSha256"], SHA256)
        self.assertEqual(len(data["cases"]), 163)
        for case in data["cases"]:
            expected = [{"offset": 0xDB0, "arguments": []}, {"offset": 0x10698, "arguments": [4]}, {"offset": 0xD504, "arguments": []}]
            self.assertEqual(case["calls"], expected if case["routine"] == "initialize" else [])

    @unittest.skipUnless(os.environ.get("SPP_GARDEN_PAYLOAD"), "local native payload not configured")
    def test_regenerate(self):
        self.assertEqual(generate(Path(os.environ["SPP_GARDEN_PAYLOAD"]).read_bytes()), json.loads(FIXTURE.read_text()))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", default=os.environ.get("SPP_GARDEN_PAYLOAD"), type=Path)
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.payload:
        parser.error("--payload or SPP_GARDEN_PAYLOAD required")
    result = generate(args.payload.read_bytes())
    text = json.dumps(result, indent=2) + "\n"
    if args.write:
        args.write.write_text(text)
    else:
        print(text)
