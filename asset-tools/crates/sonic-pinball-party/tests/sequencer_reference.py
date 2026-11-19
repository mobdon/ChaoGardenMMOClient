"""A3VE original-instruction sequencer oracle over synthetic RAM/streams.

Controller cases intercept every outgoing helper/callback and record exact
consumed operands, order and controlled mutations. Native-leaf cases execute
the original command/fade/parameter/link helpers and intercept only explicitly
listed note, pitch, presync and PSG-key-off effects. This is not mixing or
device execution. Retail source/tables are read locally, never emitted.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
                              UC_ARM_REG_R4, UC_ARM_REG_R5, UC_ARM_REG_LR,
                              UC_ARM_REG_PC, UC_ARM_REG_SP)

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES = [
    ("tick", 0x5E3A8, 0x268, "4aed271a94f8dd45897ee0b891ccf1101219125a813807c9d2178f379602e625"),
    ("fade", 0x5F294, 0xC8, "79f701b4059c25040b466e336b099e718eb6bd85d0d0fcb3aee7cc2e218aa39f"),
    ("parameters", 0x5F35C, 0xB4, "00879cc3e04cd6caf909a6f1763a16ae1ceff8b9f40b83afea83b283b3dc35f5"),
    ("volume", 0x5E654, 0x30, "04988bf8e911a12b477d9aafa24342b68e83960a06796e555869cc67b24bd4d2"),
    ("end_tie", 0x5E884, 0x40, "3e069790b65fccbf8336f5eea85d29e3fbc9fc91a2d6dcd363df78663584ad9d"),
    ("clear_lfo", 0x5E8C4, 0x1C, "4317009f4748028f27c2ac3672106a2c22d1eb90ad97d417fbe8f76d31477982"),
    ("raw_next", 0x5E8E0, 0xC, "e4238e77b732dc6af7508b2c5602ff2be8caac3462dadeeadd9a5d437dd31120"),
    ("speed", 0x5E8EC, 0x14, "8735de46279db18f4c72f2c0c315877a1f90e673db2edb263b2f911a36cd5162"),
    ("depth", 0x5E900, 0x14, "00ae8370e42929ddb727f93cd133ae72abb36a748bf3452625f99cb4504b4596"),
    ("dispatch", 0xF5F58, 144, "07070902ece8f71e1302f45bc09ae93555c715dc7c578a7eda7901a981f6358f"),
    ("waits", 0xF61CC, 49, "952d81c712713fc630d9d622d5f79eec8189f2a4cf05a78442857b635f0d23f8"),
]
PLAYER, TRACK, CHANNEL, SOUND = 0x02001000, 0x02002000, 0x02003000, 0x02005000
TABLE, COMMAND, VOICE = 0x02006000, 0x02008000, 0x02009000
STOP, SLOT = 0x03007F00, 0x03007FF0
SIGNATURE = 0x68736D53
PRESYNC, NOTE, PSG, KEYOFF = 0x0200A001, 0x0200A011, 0x0200A021, 0x0200A031


def u32(buf, off, value):
    struct.pack_into("<I", buf, off, value)


def u16(buf, off, value):
    struct.pack_into("<H", buf, off, value)


def span(address, data):
    return dict(address=address, bytes=bytes(data).hex())


class Oracle:
    def __init__(self, rom):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, length in [(0, 0x10000), (0x02000000, 0x40000),
                                (0x03000000, 0x8000), (0x04000000, 0x1000), (0x08000000, 0x800000)]:
            self.uc.mem_map(address, length)
        self.uc.mem_write(0x08000000, rom)
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, uc, address, _size, _user):
        if address not in self.intercept:
            return
        target, operands = self.intercept[address]
        call = dict(target=target, r0=None, r1=None, r2=None, r4=None, r5=None)
        registers = {0: UC_ARM_REG_R0, 1: UC_ARM_REG_R1, 2: UC_ARM_REG_R2,
                     4: UC_ARM_REG_R4, 5: UC_ARM_REG_R5}
        for index in operands:
            call[f"r{index}"] = uc.reg_read(registers[index])
        self.calls.append(call)
        occurrence = self.occurrences.get(target, 0) + 1
        self.occurrences[target] = occurrence
        result = 0
        for action in self.actions:
            if action["target"] != target or action["occurrence"] != occurrence:
                continue
            for at, width, value in action.get("writes", []):
                uc.mem_write(at, value.to_bytes(4, "little")[:width])
            result = action.get("return", result)
        uc.reg_write(UC_ARM_REG_R0, result)
        uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def run(self, entry, spans, actions, mode, args=None, resources=None):
        self.uc.mem_write(0x02000000, bytes(0x40000))
        self.uc.mem_write(0x03000000, bytes(0x8000))
        for record in (resources or []) + spans:
            self.uc.mem_write(record["address"], bytes.fromhex(record["bytes"]))
        self.calls, self.occurrences, self.actions = [], {}, actions
        self.intercept = {
            PRESYNC & ~1: (PRESYNC, [0]), NOTE & ~1: (NOTE, [0, 1, 2]),
            PSG & ~1: (PSG, [0, 1, 2]), KEYOFF & ~1: (KEYOFF, [0]),
            0x0805E914: (0x0805E914, [0, 1, 2]),
        }
        if mode == "controller":
            for target, operands in [(0x0805F294, [0]), (0x0805F35C, [0, 1]),
                                     (0x0805EDA4, [0]), (0x0805ED90, [0]), (0x0805E654, [4, 5])]:
                self.intercept[target] = (target, operands)
            for index in range(30):
                address = 0x0200B000 + 4 * index
                self.intercept[address] = (address | 1, [0, 1])
        if mode == "fade":
            self.intercept[0x0805E610] = (0x0805E610, [0, 1])
        registers = {0: UC_ARM_REG_R0, 1: UC_ARM_REG_R1, 2: UC_ARM_REG_R2,
                     4: UC_ARM_REG_R4, 5: UC_ARM_REG_R5}
        for index, register in registers.items():
            self.uc.reg_write(register, (args or {}).get(index, 0))
        self.uc.reg_write(UC_ARM_REG_SP, 0x03007D00)
        self.uc.reg_write(UC_ARM_REG_LR, STOP | 1)
        self.uc.emu_start(0x08000000 + entry | 1, STOP, count=1000000)
        if self.uc.reg_read(UC_ARM_REG_PC) != STOP:
            raise RuntimeError(f"bounded original {entry:x} did not return")
        return [span(row["address"], self.uc.mem_read(row["address"], len(row["bytes"]) // 2)) for row in spans]


def initial(rng, count=1):
    player = bytearray(rng.randbytes(64))
    player[8] = count
    u32(player, 4, 1)
    u32(player, 0xC, 0xFFFFFFFF)
    u16(player, 0x20, 150)
    u16(player, 0x22, 0)
    u16(player, 0x24, 0)
    u32(player, 0x2C, TRACK)
    u32(player, 0x30, VOICE)
    u32(player, 0x34, SIGNATURE)
    u32(player, 0x38, 0)
    u32(player, 0x3C, 0x12345678)
    tracks = bytearray(rng.randbytes(80 * max(count, 1)))
    for index in range(max(count, 1)):
        at = index * 80
        tracks[at] = 0x80
        tracks[at + 1] = 2
        tracks[at + 0x17] = tracks[at + 0x19] = tracks[at + 0x1C] = 0
        u32(tracks, at + 0x20, 0)
        u32(tracks, at + 0x40, COMMAND + 64 * index)
    sound = bytearray(64)
    u32(sound, 0x2C, KEYOFF)
    u32(sound, 0x30, PSG)
    u32(sound, 0x34, TABLE)
    u32(sound, 0x38, NOTE)
    return player, tracks, sound


def generate(rom):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact local A3VE source")
    for name, offset, length, digest in ROUTINES:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"source mismatch: {name}")
    rng, oracle = random.Random(0xA3E5E3A8), Oracle(rom)
    controller, leaves, native = [], [], []
    waits = bytes([0] + [1 + i % 17 for i in range(48)])
    fake_table = struct.pack("<30I", *[0x0200B001 + i * 4 for i in range(30)])
    for i in range(256):
        scenario, count = i % 16, (i // 16) % 4
        p, t, s = initial(rng, count)
        channels = bytearray(rng.randbytes(3 * 64))
        for j in range(3):
            channels[j * 64] = [0, 1, 8, 0x40, 0x80, 0xFF][(i + j) % 6]
            channels[j * 64 + 1] = j
            channels[j * 64 + 0x10] = [0, 1, 2, 255][(i + j) % 4]
            u32(channels, j * 64 + 0x34, CHANNEL + (j + 1) * 64 if j < 2 else 0)
        actions = []
        streams = bytearray([0x81] * (64 * max(count, 1)))
        if i % 3 == 0:
            u32(p, 0x38, PRESYNC)
        if scenario == 0:
            u32(p, 0x34, SIGNATURE ^ 1)
        elif scenario == 1:
            u32(p, 4, 0x80000000)
        elif scenario == 2:
            actions.append(dict(target=0x0805F294, occurrence=1, writes=[[PLAYER + 4, 4, 0x80000002]]))
        elif scenario == 3:
            u16(p, 0x20, 0)
            u16(p, 0x22, 149)
            t[0] = 0x8F
        elif scenario == 4:
            u16(p, 0x20, 450)
        elif scenario == 5:
            t[1], streams[0] = 0, 0xB1
            actions.append(dict(target=0x0200B001, occurrence=1, writes=[[TRACK, 1, 0]]))
        elif scenario == 6:
            t[1], t[7], streams[0] = 0, 0xCF, 17
            actions.append(dict(target=NOTE, occurrence=1, writes=[[TRACK + 0x40, 4, COMMAND + 1], [TRACK + 1, 1, 3]]))
        elif scenario == 7:
            t[1], streams[0] = 0, 0xBD
            actions.append(dict(target=0x0200B031, occurrence=1, writes=[[TRACK + 1, 1, 4]]))
        elif scenario == 8:
            t[0] = 0xC0
            actions.append(dict(target=0x0805EDA4, occurrence=1, writes=[[TRACK + j, 4, 0] for j in range(0, 64, 4)]))
        elif scenario == 9:
            for j in range(max(count, 1)):
                off = j * 80
                t[off + 0x17], t[off + 0x19] = 255, 255
                t[off + 0x1A], t[off + 0x18] = [0, 63, 64, 127, 128, 191, 255][(i + j) % 7], j % 3
        elif scenario == 10:
            channels[0] = 0
            u32(t, 0x20, CHANNEL)
            actions.append(dict(target=0x0805ED90, occurrence=1, writes=[[CHANNEL + 0x34, 4, 0]]))
        elif scenario == 11:
            u32(t, 0x20, CHANNEL)
            t[0] = 0x8F
            actions.append(dict(target=0x0805F294, occurrence=1, writes=[[SLOT, 4, 0]]))
        elif scenario == 12:
            t[0] = 0
        elif scenario == 13:
            u16(p, 0x20, 65535)
            u16(p, 0x22, 1)
            for j in range(max(count, 1)):
                t[j * 80] = 0
        elif scenario == 14:
            t[0] = 0x8F
            u32(t, 0x20, CHANNEL)
        else:
            t[1] = 0
            streams[:4] = bytes([0x80, 0x80, 0x80, 0xB0])
        for target in [PSG, 0x0805E914]:
            for occurrence in range(1, 4):
                actions.append(dict(target=target, occurrence=occurrence, **{"return": 0x98760000 + occurrence}))
        spans = [span(PLAYER, p), span(TRACK, t), span(CHANNEL, channels), span(SOUND, s),
                 span(TABLE, fake_table), span(COMMAND, streams), span(SLOT, struct.pack("<I", SOUND)),
                 span(0x080F61CC, waits)]
        after = oracle.run(0x5E3A8, spans, actions, "controller", {0: PLAYER})
        controller.append(dict(input=spans, after=after, actions=actions, calls=oracle.calls))

    # These routines run without helper/callback interception.
    for entry in [0x5F35C, 0x5E654, 0x5E8EC, 0x5E900, 0x5E884]:
        for i in range(128):
            p, t, _ = initial(rng)
            t = bytearray(rng.randbytes(80))
            t[0x18] = i % 4
            u32(t, 0x40, COMMAND)
            u32(t, 0x20, CHANNEL)
            channels = bytearray(rng.randbytes(192))
            for j in range(3):
                channels[j * 64] = [0, 1, 2, 4, 0x40, 0x80, 0xFF][(i + j) % 7]
                channels[j * 64 + 0x11] = t[5] if j == i % 3 else i % 128
                u32(channels, j * 64 + 0x34, CHANNEL + (j + 1) * 64 if j < 2 else 0)
            command = bytes([[0, 1, 127, 128, 255][i % 5]])
            spans = [span(PLAYER, p), span(TRACK, t), span(CHANNEL, channels), span(COMMAND, command)]
            after = oracle.run(entry, spans, [], "leaf", {0: PLAYER, 1: TRACK, 4: CHANNEL, 5: TRACK})
            if oracle.calls:
                raise RuntimeError("pure leaf unexpectedly crossed callback boundary")
            leaves.append(dict(entry=entry, input=spans, after=after, actions=[], calls=[]))
    for i in range(128):
        p, t, _ = initial(rng, i % 4)
        u16(p, 0x24, [0, 1, 2, 65535][i % 4])
        u16(p, 0x26, [0, 1, 2][(i // 4) % 3])
        u16(p, 0x28, [0, 1, 2, 3, 15, 16, 17, 18, 240, 242, 255, 256, 32768, 65535][i % 14])
        for j in range(max(p[8], 1)):
            t[j * 80] = [0, 0x80, 0x8F][(i + j) % 3]
        actions = []
        if i % 7 == 0:
            actions.append(dict(target=0x0805E610, occurrence=1, writes=[[PLAYER + 0x28, 2, 1]]))
        spans = [span(PLAYER, p), span(TRACK, t)]
        after = oracle.run(0x5F294, spans, actions, "fade", {0: PLAYER})
        leaves.append(dict(entry=0x5F294, input=spans, after=after, actions=actions, calls=oracle.calls))

    # Whole original controller with actual local dispatch/wait resources and
    # all ported children executing. Streams and instruments are synthetic.
    for i in range(128):
        p, t, s = initial(rng, 1 + i % 3)
        count = p[8]
        u16(p, 0x20, [0, 150, 300, 450][(i // 3) % 4])
        streams = bytearray([0x81] * (count * 64))
        voices = None
        for j in range(count):
            off = j * 80
            t[off] = [0x80, 0xC0, 0x8F][(i + j) % 3]
            t[off + 1] = 0
            t[off + 2] = 0
            t[off + 0x17] = (i + j) % 4
            t[off + 0x19] = 22
            u32(t, off + 0x20, 0)
            # Native proven scalar commands, raw LFO and wait, then finish.
            stream = [0xBC, i % 128, 0xBE, 64, 0xBF, 64, 0xC1, 2,
                      0xC2, i % 128, 0xC5, 1, 0xC8, 64, 0xC4, j % 3, 0xB0, 0xB1]
            streams[j * 64:j * 64 + len(stream)] = bytes(stream)
            target = COMMAND + j * 64 + 32
            variant = i % 8
            if variant == 1:
                streams[j * 64:j * 64 + 5] = bytes([0xB2]) + struct.pack("<I", target)
                streams[j * 64 + 32:j * 64 + 36] = bytes([0xBE, 64, 0xB0, 0xB1])
            elif variant == 2:
                streams[j * 64:j * 64 + 7] = bytes([0xB3]) + struct.pack("<I", target) + bytes([0xB0, 0xB1])
                streams[j * 64 + 32:j * 64 + 35] = bytes([0xBE, 64, 0xB4])
            elif variant == 3:
                t[off + 3] = 0
                streams[j * 64:j * 64 + 8] = bytes([0xB5, 2]) + struct.pack("<I", COMMAND + j * 64) + bytes([0xB0, 0xB1])
            elif variant == 4:
                voices = bytes((index * 37 + i) % 256 for index in range(3072))
                streams[j * 64:j * 64 + 4] = bytes([0xBD, (i + j) % 256, 0xB0, 0xB1])
            elif variant == 5:
                t[off], t[off + 2] = 0x80, 3
                streams[j * 64:j * 64 + 5] = bytes([0xB3]) + struct.pack("<I", target)
            elif variant == 6:
                streams[j * 64:j * 64 + 7] = bytes([0xBE, 64, 1, 2, 3, 0xB0, 0xB1])
            elif variant == 7:
                streams[j * 64:j * 64 + 4] = bytes([0xCE, 42, 0xB0, 0xB1])
        if i % 5 == 0:
            u16(p, 0x24, 1)
            u16(p, 0x26, 1)
            u16(p, 0x28, [1, 16, 18, 242][(i // 5) % 4])
        channels = bytearray(rng.randbytes(192))
        u32(t, 0x20, CHANNEL)
        for j in range(3):
            at = j * 64
            channels[at] = [0, 1, 2, 8, 0x40, 0x80, 0xFF][(i + j) % 7]
            channels[at + 1] = j
            channels[at + 0x10] = [0, 1, 2, 255][(i + j) % 4]
            channels[at + 0x11] = 42
            u32(channels, at + 0x24, 0x0200C000 + j * 0x100)
            u32(channels, at + 0x2C, TRACK)
            u32(channels, at + 0x30, CHANNEL + (j - 1) * 64 if j else 0)
            u32(channels, at + 0x34, CHANNEL + (j + 1) * 64 if j < 2 else 0)
        if i % 7 == 0:
            streams[0] = 0xB1
        spans = [span(PLAYER, p), span(TRACK, t), span(CHANNEL, channels), span(SOUND, s), span(COMMAND, streams),
                 span(SLOT, struct.pack("<I", SOUND)), span(0x020387C8, struct.pack("<2I", 0x0805E149, 0x0805E131))]
        if voices is not None:
            spans.append(span(VOICE, voices))
        resources = [span(TABLE, rom[0xF5F58:0xF5F58 + 144]), span(0x080F61CC, rom[0xF61CC:0xF61CC + 49])]
        after = oracle.run(0x5E3A8, spans, [], "native", {0: PLAYER}, resources)
        native.append(dict(input=spans, after=after, actions=[], calls=oracle.calls))
    return dict(schemaVersion=1, romSha256=ROM_SHA256,
                scope="Synthetic states/streams; controller helpers intercepted; pure leaves unhooked; native compositions execute local command/fade/link leaves; no mixer/device/playback claim",
                routines=[dict(name=n, offset=o, length=l, sha256=d) for n, o, l, d in ROUTINES],
                controller=controller, leaves=leaves, native=native)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = generate(args.rom.read_bytes())
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps(dict(controller=len(result["controller"]), leaves=len(result["leaves"]), native=len(result["native"]), sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))


if __name__ == "__main__":
    main()
