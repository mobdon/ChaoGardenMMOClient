"""Execute original adjacent A3VE sequence commands, unlink and track finish.

No helper or BIOS call is intercepted. Hardware-byte commands execute into
mapped synthetic register storage; a write observer records the native store.
That capture is not hardware execution or audible playback. All records and
command bytes in the resulting fixture are synthetic, never retail assets.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_SP

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES = [
    ("unlink", 0x5E148, 0x20, "c3d23903d0b87a7b19fcd84aa0234c674e3014a9a2835853727defeaad3422dc"),
    ("finish", 0x5E168, 0x30, "767cb12459588832366a031b8088826493705f8746b882354bb470db7bdb62de"),
    ("protected", 0x5E1B0, 0x1C, "af7ead00c7d4a888a7dfad6d44dde69af96aa53f107179d0c1d9531b54dc1f08"),
    ("next_byte", 0x5E1CC, 0xC, "010e01a515d6e7eee6249524429999a1b68537114c487bd9ace668ecf5841cf0"),
    ("conditional18", 0x5E318, 0x18, "c734d511a1b0ea4d505481fc760e8dbdcc7d0e3c3c3b601d08d15a6e7c8e2012"),
    ("centered0c", 0x5E330, 0x14, "4e96fbb359b181f6b7735b9f7a21a7a020589d161c418add086baf01f2bba781"),
    ("register_byte", 0x5E344, 0x18, "3d9067b73920a3d56ed65fd9885a2c9182b5882ede1c618c0c3b279f669e3feb"),
]
PLAYER, TRACK, STOP = 0x02001000, 0x02002000, 0x03007F00


class Oracle:
    def __init__(self, rom):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, length in [(0, 0x10000), (0x01FFF000, 0x1000), (0x02000000, 0x40000),
                                (0x03000000, 0x8000), (0x04000000, 0x1000), (0x08000000, 0x800000)]:
            self.uc.mem_map(address, length)
        self.uc.mem_write(0x08000000, rom)
        self.uc.hook_add(UC_HOOK_MEM_WRITE, self.write)
        self.writes = []

    def write(self, uc, access, address, size, value, _):
        if 0x04000000 <= address < 0x04001000:
            self.writes.append([address, size, value])

    def run(self, entry, argument0, argument1):
        self.writes = []
        self.uc.reg_write(UC_ARM_REG_R0, argument0)
        self.uc.reg_write(UC_ARM_REG_R1, argument1)
        self.uc.reg_write(UC_ARM_REG_SP, 0x03007D00)
        self.uc.reg_write(UC_ARM_REG_LR, STOP | 1)
        self.uc.emu_start(0x08000000 + entry | 1, STOP, count=200000)
        if self.uc.reg_read(UC_ARM_REG_PC) != STOP:
            raise RuntimeError("bounded original sequence routine did not return")


def generate(rom):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact local A3VE ROM")
    for name, offset, length, digest in ROUTINES:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"native {name} source mismatch")
    rng = random.Random(0xA3E5E318)
    oracle, commands, graphs = Oracle(rom), [], []
    uc = oracle.uc
    for entry in [0x5E318, 0x5E330, 0x5E344]:
        for i in range(128):
            player = rng.randbytes(64)
            track = bytearray(rng.randbytes(80))
            cursor = [0x1000, 0x02008000, 0x01FFFFFF, 0x01FFFFF8][i % 4]
            command = bytearray(rng.randbytes(8))
            command[0] = [0, 1, 63, 64, 65, 127, 128, 255][(i // 4) % 8]
            command[1] = [255, 65, 64, 63, 1, 0, 127, 128][(i // 8) % 8]
            struct.pack_into("<I", track, 0x40, cursor)
            protected = command[0] if cursor >= 0x02000000 else 0
            track[0x18] = protected if i % 3 == 0 else protected ^ 255
            uc.mem_write(PLAYER, player)
            uc.mem_write(TRACK, bytes(track))
            uc.mem_write(cursor, bytes(command))
            oracle.run(entry, PLAYER, TRACK)
            if bytes(uc.mem_read(PLAYER, 64)) != player:
                raise RuntimeError("adjacent handler unexpectedly changed player state")
            commands.append(dict(entry=entry, cursor=cursor, command=command.hex(), track=track.hex(),
                                 after=bytes(uc.mem_read(TRACK, 80)).hex(), device_writes=oracle.writes))
    track_addresses = [TRACK, TRACK + 0x100]
    channel_addresses = [0x02003000 + i * 64 for i in range(5)]
    for entry in [0x5E148, 0x5E168]:
        for i in range(256):
            tracks = [bytearray(rng.randbytes(80)) for _ in track_addresses]
            channels = [bytearray(rng.randbytes(64)) for _ in channel_addresses]
            target = i % 2
            count = (i // 2) % 6
            tracks[target][0] = [0, 0x80, 0x40, 0xFF][(i // 6) % 4]
            for index, channel in enumerate(channels):
                struct.pack_into("<3I", channel, 0x2C, 0, 0, 0)
                channel[0] = [0, 1, 2, 4, 8, 16, 32, 64, 128, 255][(i + index) % 10]
            for track in tracks:
                struct.pack_into("<I", track, 0x20, 0)
            struct.pack_into("<I", tracks[target], 0x20, channel_addresses[0] if count else 0)
            for index in range(count):
                owner = 0 if (i + index) % 7 == 0 else track_addresses[target]
                previous = channel_addresses[index - 1] if index else 0
                next_ = channel_addresses[index + 1] if index + 1 < count else 0
                struct.pack_into("<3I", channels[index], 0x2C, owner, previous, next_)
            if count < 5:
                struct.pack_into("<I", tracks[1 - target], 0x20, channel_addresses[count])
                struct.pack_into("<I", channels[count], 0x2C, track_addresses[1 - target])
            selected_channel = channel_addresses[(i // 3) % 5]
            if entry == 0x5E148 and i % 9 == 0:
                # Arbitrary, explicitly supplied backlinks exercise the helper's
                # exact writes without requiring a semantic linked-list repair.
                index = channel_addresses.index(selected_channel)
                struct.pack_into("<3I", channels[index], 0x2C, track_addresses[target],
                                 channel_addresses[(index + 1) % 5], channel_addresses[(index + 2) % 5])
            before_tracks = [{"address": address, "bytes": record.hex()} for address, record in zip(track_addresses, tracks)]
            before_channels = [{"address": address, "bytes": record.hex()} for address, record in zip(channel_addresses, channels)]
            for address, record in zip(track_addresses, tracks):
                uc.mem_write(address, bytes(record))
            for address, record in zip(channel_addresses, channels):
                uc.mem_write(address, bytes(record))
            oracle.run(entry, selected_channel if entry == 0x5E148 else PLAYER, track_addresses[target])
            graphs.append(dict(entry=entry, argument=selected_channel if entry == 0x5E148 else track_addresses[target],
                               tracks=before_tracks, channels=before_channels,
                               after_tracks=[{"address": address, "bytes": bytes(uc.mem_read(address, 80)).hex()} for address in track_addresses],
                               after_channels=[{"address": address, "bytes": bytes(uc.mem_read(address, 64)).hex()} for address in channel_addresses]))
    return dict(schemaVersion=1, romSha256=ROM_SHA256,
                scope="Complete original command/unlink/finish leaves; no helper or BIOS interception; hardware byte stores captured in synthetic mapped storage, not device execution or playback",
                routines=[dict(name=name, offset=offset, length=length, sha256=digest) for name, offset, length, digest in ROUTINES],
                commands=commands, graphs=graphs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = generate(args.rom.read_bytes())
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps(dict(commands=len(result["commands"]), graphCalls=len(result["graphs"]), sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))


if __name__ == "__main__":
    main()
