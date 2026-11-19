"""Execute A3VE's complete frame wrapper as an independent test oracle.

All external calls are intercepted in wrapper-only mode. --native-leaves runs
the original flipper-update and ball-integrate instructions and native geometry;
other calls are still explicitly intercepted. No full frame/game parity claim
follows from these tests. All committed memory inputs/results are synthetic.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
FRAME = (0xA438, 0x230, "22e671310c7d77a36a67a0ad3190e54db07edbc61037b428062adddde38b3ce1")
LEAVES = [
    (0x2AC18, 0x258, "b87031892b924596255532029cea638f6edb72671aca3647cf216af2da83c576"),
    (0x2AE70, 0x310, "dd0a4cfde79c661192950741317d2c7e934fbb6d255770bc411faff9986353a9"),
    (0x2A04C, 0x138, "fcbf0d803980db7bdde77b122d12d0b321095e17b05addf69e68233d8ffda34e"),
]
GEOMETRY = (0x6E91C0, 0x168, "5da92d0db33d0213ce881598ca9127a70c25fd16eac938c588de21068793038e")
CALLS = {0x3C170, 0x2AC18, 0x2E1A0, 0x2EA04, 0x2D698, 0x39EE4, 0x2A04C,
         0x2BA18, 0x29C7C, 0x27180, 0x1FA30, 0x20FB0, 0x264A0, 0x50A08,
         0x31190, 0x2FD68, 0x1FC30, 0x20528, 0x20CA8, 0x461F8, 0x274B8,
         0x43D30, 0x3E0B0, 0x3E10C, 0x43D5C, 0x3E44C, 0x3E158}
ARGUMENTS = {0x2D698, 0x461F8, 0x3E0B0, 0x3E10C, 0x43D5C}
STATE = 0x02000000
SECOND_STATE = 0x02001000
SENTINEL = 0x03007FF0
STATE_POINTER = 0x030040B0
PAUSE_A, PAUSE_B = 0x020319AC, 0x020319A4
TABLE = 0x03003068
MULTIBALL = 0x020328C0
BALL_Y = 0x02031AE4
INITIAL_GATE = 0x0300296C


def packed(value, width):
    return int(value).to_bytes(width, "little")


class Oracle:
    def __init__(self, rom, native_leaves):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, size in [(0x08000000, 0x800000), (0x02000000, 0x40000), (0x03000000, 0x8000)]:
            self.uc.mem_map(address, size)
        self.uc.mem_write(0x08000000, rom)
        self.uc.hook_add(UC_HOOK_CODE, self.hook)
        self.native_leaves = native_leaves

    def read(self, address, width):
        return int.from_bytes(self.uc.mem_read(address, width), "little")

    def hook(self, uc, address, size, _):
        if address == SENTINEL:
            uc.emu_stop()
            return
        offset = address - 0x08000000
        if offset not in CALLS:
            return
        self.occurrences[offset] = self.occurrences.get(offset, 0) + 1
        pointer = self.read(STATE_POINTER, 4)
        self.calls.append({"offset": offset,
                           "argument0": uc.reg_read(UC_ARM_REG_R0) if offset in ARGUMENTS else None,
                           "pointer": pointer, "byte2": self.read(pointer + 2, 1), "byte3": self.read(pointer + 3, 1)})
        if self.native_leaves and offset in {0x2AC18, 0x2A04C}:
            return
        value = 0
        for action in self.actions:
            if action["offset"] == offset and action["occurrence"] == self.occurrences[offset]:
                for target, width, data in action.get("writes", []):
                    uc.mem_write(target, packed(data, width))
                value = action.get("return", value)
        uc.reg_write(UC_ARM_REG_R0, value)
        uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def run(self, spans, actions):
        uc = self.uc
        uc.mem_write(0x02000000, bytes(0x40000))
        uc.mem_write(0x03000000, bytes(0x8000))
        for span in spans:
            uc.mem_write(span["address"], bytes.fromhex(span["bytes"]))
        self.calls, self.actions, self.occurrences = [], actions, {}
        uc.reg_write(UC_ARM_REG_SP, 0x03007F00)
        uc.reg_write(UC_ARM_REG_LR, SENTINEL | 1)
        uc.emu_start(0x0800A439, SENTINEL, count=2000000)
        if uc.reg_read(UC_ARM_REG_PC) != SENTINEL:
            raise RuntimeError("bounded native frame wrapper did not return")
        return {"calls": self.calls, "spans": [{"address": span["address"], "bytes": bytes(uc.mem_read(span["address"], len(bytes.fromhex(span["bytes"])))).hex()} for span in spans]}


def metadata(items):
    return [{"offset": offset, "length": length, "sha256": digest} for offset, length, digest in items]


def generate(rom, native_leaves=False):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact supplied A3VE ROM")
    routines = [FRAME] + (LEAVES if native_leaves else [])
    resources = [GEOMETRY] if native_leaves else []
    for offset, length, digest in routines + resources:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"source mismatch at {offset:#x}")
    rng = random.Random(0xA3FEA438)
    oracle = Oracle(rom, native_leaves)
    result = {"schemaVersion": 1, "romSha256": ROM_SHA256,
              "oracle": "Unicorn 2.1.4; original complete frame controller; " + ("original flipper/integrator leaves + native geometry; all other dependencies explicitly intercepted" if native_leaves else "all dependencies explicitly intercepted; wrapper-only ordering/gates parity"),
              "routines": metadata(routines), "resources": metadata(resources), "nativeLeaves": native_leaves, "cases": []}
    for i in range(128 if not native_leaves else 48):
        states = [bytearray(rng.randbytes(0x108)) for _ in range(2)]
        mode = 10 if i % 2 else 2
        for state in states:
            struct.pack_into("<H", state, 0x6E, mode)
            struct.pack_into("<H", state, 0x28, i % 3)
            state[3] = 1 if i % 7 == 0 else 0
            struct.pack_into("<H", state, 0xF8, i % 5)
            for flipper in range(4):
                struct.pack_into("<H", state, 0xC4 + 12 * flipper, (i + flipper) % 13)
            struct.pack_into("<H", state, 0xB8, [0, 0, 1][i % 3])
        words = {STATE_POINTER: STATE, INITIAL_GATE: [0, 1, 2][i % 3], PAUSE_A: [0, 1, 0, 2][(i // 2) % 4], PAUSE_B: [0, 0, 1, 2][(i // 8) % 4], 0x020328BC: 0 if i % 3 else 1}
        halves = {TABLE: i % (4 if native_leaves else 6), BALL_Y: rng.randrange(0x10000), MULTIBALL + 0xA8: (i // 2) % 4,
                  0x030040C4: 0x100, 0x030040C6: 0x200, 0x03000514: [0, 0x100, 0x200, 0x300][i % 4], 0x03000516: 0,
                  STATE + 0x6E2: 0, SECOND_STATE + 0x6E2: 0}
        for index in range(3):
            halves[MULTIBALL + index * 56 + 0x30] = 0 if (i + index) % 3 == 0 else 1
        spans = [{"address": address, "bytes": state.hex()} for address, state in zip([STATE, SECOND_STATE], states)]
        spans += [{"address": address, "bytes": packed(value, 4).hex()} for address, value in sorted(words.items())]
        spans += [{"address": address, "bytes": packed(value, 2).hex()} for address, value in sorted(halves.items())]
        actions = []
        if not native_leaves:
            # Deliberate host-side test doubles; these are not native behavior.
            actions += [{"offset": 0x2A04C, "occurrence": 2, "writes": [[STATE + 3, 1, 1]]},
                        {"offset": 0x2A04C, "occurrence": 4, "writes": [[STATE + 3, 1, 0]]},
                        {"offset": 0x29C7C, "occurrence": 1, "writes": [[STATE + 0x9A, 2, 0xA000 + i]]},
                        {"offset": 0x3E0B0, "occurrence": 2, "writes": [[STATE_POINTER, 4, SECOND_STATE]]}]
            if i % 5 == 0:
                actions.append({"offset": 0x1FA30, "occurrence": 1, "writes": [[PAUSE_A, 4, 1]]})
            if i % 11 == 0:
                actions.append({"offset": 0x3C170, "occurrence": 1, "writes": [[STATE + 0x6E, 2, 10]]})
        if i % 4 == 1:
            actions.append({"offset": 0x43D5C, "occurrence": 1, "return": 0x10001})
        elif i % 4 == 3:
            actions.append({"offset": 0x43D5C, "occurrence": 1, "return": 0x10000})
        case = {"input": spans, "actions": actions}
        case.update(oracle.run(spans, actions))
        result["cases"].append(case)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--native-leaves", action="store_true")
    args = parser.parse_args()
    result = generate(args.rom.read_bytes(), args.native_leaves)
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps({"cases": len(result["cases"]), "nativeLeaves": args.native_leaves, "sha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
