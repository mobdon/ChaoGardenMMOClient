"""Original-instruction oracle for native A3VE directional nudge and decay.

The synthetic-curve mode overrides only test data in Unicorn memory. Native
mode retains the unmodified curve, including the counter-zero preceding read.
Neither mode intercepts dependencies; the original routine is a complete leaf.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINE = (0x39EE4, 0x1B4, "c0ec8ad410575cd2bf3034a983dee8e237bf4e63a047ab7b47c5722d639667ed")
RESOURCE = (0x6E9ECE, 20, "1c92b60ec86694dfdcaf503b699753480e1aedfde4dedd375e07bbb39832286a")
STATE, GLOBALS, STOP = 0x02000000, 0x02035240, 0x03007FF0


class Oracle:
    def __init__(self, rom, curve):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, size in [(0x08000000, 0x800000), (0x02000000, 0x40000), (0x03000000, 0x8000)]:
            self.uc.mem_map(address, size)
        self.uc.mem_write(0x08000000, rom)
        if curve is not None:
            self.uc.mem_write(0x086E9ECE, curve)
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, uc, address, size, _):
        if address == STOP:
            uc.emu_stop()

    def run(self, state, globals_, inputs):
        uc = self.uc
        uc.mem_write(0x02000000, bytes(0x40000))
        uc.mem_write(0x03000000, bytes(0x8000))
        uc.mem_write(STATE, bytes(state))
        uc.mem_write(GLOBALS, bytes(globals_))
        uc.mem_write(0x030040B0, struct.pack("<I", STATE))
        uc.mem_write(0x03000516, struct.pack("<H", inputs["pressed"]))
        uc.mem_write(0x030040C8, struct.pack("<3H", inputs["left_mask"], inputs["right_mask"], inputs["up_mask"]))
        uc.reg_write(UC_ARM_REG_SP, 0x03007F00)
        uc.reg_write(UC_ARM_REG_LR, STOP | 1)
        uc.emu_start(0x08039EE5, STOP, count=20000)
        if uc.reg_read(UC_ARM_REG_PC) != STOP:
            raise RuntimeError("bounded original nudge routine did not return")
        return {"state": bytes(uc.mem_read(STATE, 0xC4)).hex(), "globals": bytes(uc.mem_read(GLOBALS, 16)).hex()}


def metadata(items):
    return [{"offset": offset, "length": length, "sha256": digest} for offset, length, digest in items]


def generate(rom, native_curve=False):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact supplied A3VE ROM")
    for offset, length, digest in [ROUTINE, RESOURCE]:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"source mismatch at {offset:#x}")
    rng = random.Random(0xA3FE39EE)
    curve = None if native_curve else rng.randbytes(20)
    oracle = Oracle(rom, curve)
    result = {"schemaVersion": 1, "romSha256": ROM_SHA256,
              "oracle": "Unicorn 2.1.4; complete original nudge instructions; no dependency interception; " + ("native curve retained" if native_curve else "synthetic test curve only"),
              "routines": metadata([ROUTINE]), "resources": metadata([RESOURCE]) if native_curve else [], "cases": [], "sequences": []}
    if curve is not None:
        result["synthetic_curve"] = curve.hex()
    flags = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (0xFFFF, 0, 0), (1, 1, 1), (0, 0xFFFF, 0), (0, 0, 0xFFFF)]
    for i in range(320):
        state = bytearray(rng.randbytes(0xC4))
        state[0x14] = [0, 1, 0x7F, 0x80, 0xFF][i % 5]
        global_state = bytearray(rng.randbytes(16))
        struct.pack_into("<3H", global_state, 0, *flags[i % len(flags)])
        struct.pack_into("<3H", global_state, 6, i % 10, (i // 10) % 10, (i // 100) % 10)
        inputs = {"pressed": [0, 1, 2, 4, 7, 0xFFFF][(i // 8) % 6], "left_mask": [1, 0, 0xFFFF][i % 3], "right_mask": 2, "up_mask": 4}
        case = {"input_state": state.hex(), "input_globals": global_state.hex(), "inputs": inputs}
        case.update(oracle.run(state, global_state, inputs))
        result["cases"].append(case)
    for i in range(16):
        state = bytearray(rng.randbytes(0xC4))
        state[0x14] = 1
        global_state = bytearray(rng.randbytes(16))
        struct.pack_into("<3H", global_state, 0, 0, 0, 0)
        sequence = {"input_state": state.hex(), "input_globals": global_state.hex(), "frames": []}
        for frame in range(24):
            inputs = {"pressed": (1 << (i % 3)) if frame == 0 else ((1 << ((i + 1) % 3)) if frame == 12 else 0), "left_mask": 1, "right_mask": 2, "up_mask": 4}
            case = {"inputs": inputs}
            case.update(oracle.run(state, global_state, inputs))
            sequence["frames"].append(case)
            state, global_state = bytes.fromhex(case["state"]), bytes.fromhex(case["globals"])
        result["sequences"].append(sequence)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--native-curve", action="store_true")
    args = parser.parse_args()
    result = generate(args.rom.read_bytes(), args.native_curve)
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps({"cases": len(result["cases"]), "sequences": len(result["sequences"]), "nativeCurve": args.native_curve, "sha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
