"""Independent original-instruction oracle for A3VE flipper control.

Synthetic state and synthetic line geometry replace only test data in Unicorn
memory. Original code executes unchanged; the supplied ROM file is never edited.
Native sound calls are captured explicitly. No BIOS service is substituted.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
UPDATE = (0x2AC18, 0x258, "b87031892b924596255532029cea638f6edb72671aca3647cf216af2da83c576")
SIDE = (0x2AE70, 0x310, "dd0a4cfde79c661192950741317d2c7e934fbb6d255770bc411faff9986353a9")
INITIALIZE = (0x2A778, 0x21C, "8f4b03a036c173bd866cb6b42db80931e08f38500352761ecbee0b4556bc693a")
STATE = 0x02000000
STATE_LENGTH = 0x108
SENTINEL = 0x03007FF0


class Oracle:
    def __init__(self, rom, geometry):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, size in [(0x08000000, 0x800000), (0x02000000, 0x40000), (0x03000000, 0x8000)]:
            self.uc.mem_map(address, size)
        self.uc.mem_write(0x08000000, rom)
        self.uc.mem_write(0x086E91C0, geometry)
        self.sounds = []
        self.initializing = False
        self.outer_writes = []
        self.uc.hook_add(UC_HOOK_CODE, self.hook)
        self.uc.hook_add(UC_HOOK_MEM_WRITE, self.write_hook)

    def write_hook(self, uc, access, address, size, value, _):
        if self.initializing and STATE + STATE_LENGTH <= address < STATE + 0x1E0:
            self.outer_writes.append([address - STATE, size, value])

    def hook(self, uc, address, size, _):
        if address == SENTINEL:
            uc.emu_stop()
        elif address == 0x080038D4:
            self.sounds.append(uc.reg_read(UC_ARM_REG_R0))
            uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def run(self, state, inputs, entry=0x2AC18):
        uc = self.uc
        uc.mem_write(0x02000000, bytes(0x40000))
        uc.mem_write(0x03000000, bytes(0x8000))
        uc.mem_write(STATE, bytes(state))
        uc.mem_write(0x030040B0, struct.pack("<I", STATE))
        uc.mem_write(0x03003068, struct.pack("<H", inputs["table_code"]))
        uc.mem_write(0x030040C4, struct.pack("<HH", inputs["left_mask"], inputs["right_mask"]))
        uc.mem_write(0x03000514, struct.pack("<HH", inputs["held"], inputs["pressed"]))
        uc.mem_write(STATE + 0x6E2, struct.pack("<H", inputs["audio_enabled"]))
        uc.reg_write(UC_ARM_REG_SP, 0x03007F00)
        uc.reg_write(UC_ARM_REG_LR, SENTINEL | 1)
        self.sounds = []
        self.outer_writes = []
        self.initializing = entry == 0x2A778
        uc.emu_start(0x08000000 + entry | 1, SENTINEL, count=200000)
        if uc.reg_read(UC_ARM_REG_PC) != SENTINEL:
            raise RuntimeError("bounded native flipper update did not return")
        result = {"state": bytes(uc.mem_read(STATE, STATE_LENGTH)).hex(), "sound_calls": self.sounds}
        if self.initializing:
            result["outer_writes"] = self.outer_writes
        return result


def generate(rom, native_geometry=False):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact supplied A3VE ROM")
    for offset, length, digest in [INITIALIZE, UPDATE, SIDE]:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"native routine mismatch at {offset:#x}")
    rng = random.Random(0xA3BEF11F)
    if native_geometry:
        geometry = rom[0x6E91C0:0x6E9328]
    else:
        geometry = bytearray(rng.randbytes(0x168))
        for bank in range(3):
            for angle in range(13):
                row = [rng.randint(-32, 32) for _ in range(4)]
                if angle % 4 == 0:
                    row[2] = row[0]  # exercise native division by zero
                struct.pack_into("<4h", geometry, bank * 0x80 + angle * 8, *row)
    oracle = Oracle(rom, bytes(geometry))
    vectors = {"schemaVersion": 1, "romSha256": ROM_SHA256, "oracle": "Unicorn 2.1.4; unchanged native code; synthetic state; captured sound calls", "geometrySource": "exact A3VE ROM" if native_geometry else "synthetic test geometry", "cases": [], "initialize_cases": []}
    vectors["routines"] = [{"offset": offset, "length": length, "sha256": digest} for offset, length, digest in [INITIALIZE, UPDATE, SIDE]]
    vectors["resources"] = [{"offset": 0x6E91C0, "length": 0x168, "sha256": "5da92d0db33d0213ce881598ca9127a70c25fd16eac938c588de21068793038e"}] if native_geometry else []
    if not native_geometry:
        vectors["synthetic_geometry"] = bytes(geometry).hex()
    for i in range(256):
        state = bytearray(rng.randbytes(STATE_LENGTH))
        count = [0, 1, 2, 3, 4, 0xFFFF][i % 6]
        struct.pack_into("<H", state, 0xF8, count)
        struct.pack_into("<H", state, 0x6E, 10 if i % 7 == 0 else rng.randrange(10))
        for index in range(4):
            record = 0xC4 + index * 12
            struct.pack_into("<H", state, record, (i + index) % 13)
            state[record + 8] = [0, 1, 0xFF][(i + index) % 3]
            state[record + 9] = [0, 2, 3, 0x7F, 0xFF][(i + index) % 5]
        inputs = {"table_code": i % 4, "left_mask": [1, 0x200, 0x100, 0][i % 4], "right_mask": [2, 0x100, 0x200, 0xFFFF][i % 4], "held": [0, 1, 2, 0x100, 0x200, 0x300, 0xFFFF][i % 7], "pressed": [0, 1, 2, 0x100, 0x200, 0x300, 0xFFFF][(i // 7) % 7], "audio_enabled": 0 if i % 3 == 0 else 1}
        case = {"input": bytes(state).hex(), "inputs": inputs}
        case.update(oracle.run(state, inputs))
        vectors["cases"].append(case)
    for i in range(24):
        state = rng.randbytes(STATE_LENGTH)
        inputs = {"table_code": [0, 1, 2, 3, 4, 0xFFFF][i % 6], "left_mask": 1, "right_mask": 2, "held": 0, "pressed": 0, "audio_enabled": 0}
        case = {"input": state.hex(), "inputs": inputs}
        case.update(oracle.run(state, inputs, entry=0x2A778))
        vectors["initialize_cases"].append(case)
    return vectors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--native-geometry", action="store_true", help="Use original A3VE geometry; do not put it in the vector file")
    args = parser.parse_args()
    result = generate(args.rom.read_bytes(), args.native_geometry)
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps({"cases": len(result["cases"]), "initializeCases": len(result["initialize_cases"]), "outputSha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
