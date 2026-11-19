"""Generate synthetic A3VE main-ball vectors by executing the supplied ROM.

Requires the local-only `unicorn` Python package and an exact A3VE ROM. This is
an independent test oracle, never a runtime implementation or an asset bundle.
The optional native graphics call is captured, not silently implemented.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import (
    UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R5, UC_ARM_REG_R6, UC_ARM_REG_R7, UC_ARM_REG_SP,
    UC_ARM_REG_LR, UC_ARM_REG_PC,
)

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
FUNCTIONS = [
    (0x29A40, 0x168, "209276c2813b578ac79743dbb4ce212b769b846c532d9ddf9349c8cc20099ef6"),
    (0x2A04C, 0x138, "fcbf0d803980db7bdde77b122d12d0b321095e17b05addf69e68233d8ffda34e"),
    (0x2A4D0, 0x8C, "d6b43ba7a8e7790794aaf23b2c8baa3dd11b5fd95b0a266f9d3ab0bbffbd3542"),
    (0x2D698, 0x74, "fada3d2e88e4369914d4ef42710822543d80cd1bda6f5ba2be9273dff77af366"),
    (0x29C90, 0xF8, "254e95a868038aef1f099e44de329cc7c34fd2ead9cc04497d7b6aafbd8abd39"),
    (0x29DB8, 0x12, "1773241ff7cd0d45863ee5a306c62e3327347be8b988d133ed467f74996d4316"),
]
STATE_LENGTH = 0xC4
STATE = 0x02000000
SENTINEL = 0x03007FF0


class Oracle:
    def __init__(self, rom):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, size in [(0x08000000, 0x800000), (0x02000000, 0x40000), (0x03000000, 0x8000)]:
            self.uc.mem_map(address, size)
        self.uc.mem_write(0x08000000, rom)
        self.palette_calls = []
        self.executed = set()
        self.projection = False
        self.frame_probe = False
        self.bios_outputs = None
        self.bios_calls = []
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, uc, address, size, _):
        self.executed.add(address)
        if address == 0x0805FEF4 and self.bios_outputs is not None:
            value = self.bios_outputs["sqrt"]
            self.bios_calls.append({"service": 8, "x": uc.reg_read(UC_ARM_REG_R0), "result": value})
            uc.reg_write(UC_ARM_REG_R0, value)
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))
        elif address == 0x0805FEAC and self.bios_outputs is not None:
            signed = lambda value: struct.unpack("<h", struct.pack("<H", value & 0xFFFF))[0]
            value = self.bios_outputs["atan2"]
            self.bios_calls.append({"service": 10, "x": signed(uc.reg_read(UC_ARM_REG_R0)), "y": signed(uc.reg_read(UC_ARM_REG_R1)), "result": value})
            uc.reg_write(UC_ARM_REG_R0, value)
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))
        elif address == 0x08029DCA and self.frame_probe:
            uc.emu_stop()
        elif address == 0x08029D88 and self.projection:
            uc.emu_stop()
        elif address == SENTINEL:
            uc.emu_stop()
        elif address == 0x08005724:
            self.palette_calls.append([uc.reg_read(r) for r in [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2]])
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def halfwords(self, address, values):
        self.uc.mem_write(address, struct.pack("<" + "H" * len(values), *values))

    def run(self, entry, state, table, freeze=0, auxiliary=None, bottom_extent=0, bios_outputs=None):
        uc = self.uc
        uc.mem_write(0x02000000, bytes(0x40000))
        uc.mem_write(0x03000000, bytes(0x8000))
        uc.mem_write(STATE, bytes(state))
        uc.mem_write(0x030040B0, struct.pack("<I", STATE))
        uc.mem_write(0x03003068, struct.pack("<H", table))
        uc.mem_write(0x030040E0, struct.pack("<H", bottom_extent))
        uc.mem_write(0x020328BC, struct.pack("<I", freeze))
        if auxiliary is not None:
            self.halfwords(0x0203296C, auxiliary)
        uc.reg_write(UC_ARM_REG_SP, 0x03007F00)
        uc.reg_write(UC_ARM_REG_LR, SENTINEL | 1)
        if entry == 0x2D698:
            uc.reg_write(UC_ARM_REG_R0, STATE + 0xA8)
        self.palette_calls = []
        self.executed = set()
        self.projection = entry == 0x29C90
        self.frame_probe = False
        self.bios_outputs = bios_outputs
        self.bios_calls = []
        uc.emu_start(0x08000000 + entry | 1, SENTINEL, count=200000)
        if uc.reg_read(UC_ARM_REG_PC) != (0x08029D88 if self.projection else SENTINEL):
            raise RuntimeError("bounded native function did not return")
        result = {"state": bytes(uc.mem_read(STATE, STATE_LENGTH)).hex()}
        if bios_outputs is not None:
            result["bios_calls"] = self.bios_calls
            result["clamped"] = len(self.bios_calls) == 2
        if self.projection:
            result["left"] = struct.unpack("<h", struct.pack("<H", uc.reg_read(UC_ARM_REG_R6) & 0xFFFF))[0]
            result["top"] = struct.unpack("<h", struct.pack("<H", uc.reg_read(UC_ARM_REG_R5) & 0xFFFF))[0]
            # Execute the original source-pointer calculation too, rather than
            # deriving its expected frame with the Rust implementation's formula.
            self.frame_probe = True
            uc.reg_write(UC_ARM_REG_R7, 0x030040B0)
            uc.emu_start(0x08029DB9, SENTINEL, count=100)
            if uc.reg_read(UC_ARM_REG_PC) != 0x08029DCA:
                raise RuntimeError("native ball frame probe did not stop")
            result["frame"] = (uc.reg_read(UC_ARM_REG_R1) - 0x084599AC) // 128
        if auxiliary is not None:
            result["auxiliary"] = list(struct.unpack("<4H", uc.mem_read(0x0203296C, 8)))
            result["trail"] = {"count": struct.unpack("<H", uc.mem_read(0x020328B8, 2))[0]}
            for name, address in [("phase", 0x020327D0), ("x", 0x02032800), ("y", 0x02032830), ("zero", 0x02032860), ("camera_y", 0x02032890)]:
                result["trail"][name] = list(struct.unpack("<20H", uc.mem_read(address, 40)))
            result["palette_calls"] = self.palette_calls
        return result


def generate(rom):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact supplied A3VE revision 0 ROM")
    for offset, length, digest in FUNCTIONS:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"native routine mismatch at {offset:#x}")
    rng = random.Random(0xA3BEBA11)
    oracle = Oracle(rom)
    vectors = {"schemaVersion": 1, "romSha256": ROM_SHA256, "oracle": "Unicorn 2.1.4; original ROM instructions; synthetic input state; cap BIOS wrapper returns explicitly controlled, not authentic BIOS execution", "integrator": [], "initialize": [], "projection": [], "velocity_cap": []}
    vectors["routines"] = [{"offset": offset, "length": length, "sha256": digest} for offset, length, digest in FUNCTIONS]
    vectors["resources"] = [{"offset": 0xAF022, "length": 2050, "sha256": "9c9e60ec4bd598b1551293dc50521c9b4d011cb3822e53865d16647cb01ac008"}]
    # Boundary cases vary native branch conditions and signed/wrapping fields.
    boundaries = [-0x80000000, -0x7FFFFF81, -129, -128, -127, -1, 0, 1, 127, 128, 129, 0x7FFFFFFF]
    for i in range(192):
        state = bytearray(rng.randbytes(STATE_LENGTH))
        table = [0, 1, 2, 3, 4, 0xFFFF][i % 6]
        freeze = 0 if i % 4 != 0 else rng.randrange(1, 0x100000000)
        struct.pack_into("<H", state, 0x6E, [0, 7, 8, 0xFFFF][i % 4])
        struct.pack_into("<H", state, 0xB8, [0, 0, 1, 0xFFFF][i % 4])
        struct.pack_into("<H", state, 0xB6, [0, 1, 2, 0xFFFF][(i // 4) % 4])
        struct.pack_into("<h", state, 0xA8, [-32768, -1, 0, 1, 32767][i % 5])
        struct.pack_into("<h", state, 0xAA, [-32768, -1, 0, 1, 32767][(i // 5) % 5])
        struct.pack_into("<i", state, 0x90, boundaries[i % len(boundaries)])
        struct.pack_into("<i", state, 0x94, boundaries[(i // len(boundaries)) % len(boundaries)])
        case = {"table": table, "freeze": freeze, "input": bytes(state).hex()}
        case.update(oracle.run(0x2A04C, state, table, freeze))
        vectors["integrator"].append(case)
    for i in range(24):
        state = rng.randbytes(STATE_LENGTH)
        table = [0, 1, 2, 3, 4, 0xFFFF][i % 6]
        auxiliary = [0 if i < 6 else rng.randrange(1, 0x10000), rng.randrange(0x10000), rng.randrange(0x10000), rng.randrange(0x10000)]
        case = {"table": table, "input": state.hex(), "input_auxiliary": auxiliary}
        case.update(oracle.run(0x29A40, state, table, auxiliary=auxiliary))
        vectors["initialize"].append(case)
    for i in range(96):
        state = bytearray(rng.randbytes(STATE_LENGTH))
        table = [0, 1, 2, 3, 4, 0xFFFF][i % 6]
        bottom_extent = [0, 1, 88, 160, 512, 0xFFFF][(i // 6) % 6]
        struct.pack_into("<h", state, 0xA0, [-32768, -17, -1, 0, 1, 17, 478, 32767][i % 8])
        struct.pack_into("<h", state, 0xA2, [-32768, -1, 0, 174, 176, 512, 1024, 32767][(i // 8) % 8])
        case = {"table": table, "bottom_extent": bottom_extent, "input": bytes(state).hex()}
        case.update(oracle.run(0x29C90, state, table, bottom_extent=bottom_extent))
        vectors["projection"].append(case)
    for i in range(32):
        state = bytearray(rng.randbytes(STATE_LENGTH))
        struct.pack_into("<h", state, 0xA8, [-32768, -512, -1, 0, 1, 512, 32767][i % 7])
        struct.pack_into("<h", state, 0xAA, [-32768, -512, -1, 0, 1, 512, 32767][(i // 7) % 7])
        outputs = {"sqrt": [0, 512, 513, 65535][i % 4], "atan2": [0, 1, 0x1234, 0x4000, 0x7FFF, 0x8000, 0xC000, 0xFFFF][(i // 4) % 8]}
        case = {"input": bytes(state).hex(), "controlled_bios_outputs": outputs}
        case.update(oracle.run(0x2D698, state, 0, bios_outputs=outputs))
        vectors["velocity_cap"].append(case)
    return vectors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = generate(args.rom.read_bytes())
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps({"integratorCases": len(result["integrator"]), "initializeCases": len(result["initialize"]), "projectionCases": len(result["projection"]), "velocityCapCases": len(result["velocity_cap"]), "outputSha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
