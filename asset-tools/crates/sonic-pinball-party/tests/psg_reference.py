"""Original A3VE PSG update/key-off/gain reference with synthetic registers.

No helper, BIOS or oscillator call is intercepted. Native MMIO accesses run
against mapped synthetic storage. Observers record byte reads, byte writes and
32-bit wave-RAM writes in exact instruction order. This is capture-only: it
does not emulate GBA register behavior, timing, wave banks or audible output.
All channel state, wave data and initial register bytes are synthetic. The
retail amplitude-lookup extent is local-only; its bytes are never emitted.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_SP

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES = [
    ("key_off", 0x5F4B8, 0x50, "67a2670376360f300df5a7d18b458792da50002de9bcde7580342dcf8c366fcf"),
    ("gain", 0x5F508, 0x68, "584762a3caa5fe9d71f2afd7fe58f2e5a8d2ef8efb2c4d68df84a621b84c9b88"),
    ("update", 0x5F570, 0x44C, "1a723c2dca0fd2363118819467f369cd5814accdc367a432e760158af600e5a3"),
    ("curve", 0xF61BC, 16, "86480498f3bc57863a295f2c9a9752be09fd91cb28c998650fa2b7ae3bdefd24"),
    ("lookup_extent", 0xF61BC, 256, "2a91d7b83d8f677f2901d30d3c05074db85ee857a6416fdad13db84e45e318a7"),
]
SOUND, CHANNEL, WAVE, DEVICE, SLOT, STOP = 0x02001000, 0x02002000, 0x02008000, 0x04000060, 0x03007FF0, 0x03007F00


def span(address, data):
    return dict(address=address, bytes=bytes(data).hex())


def u32(buf, offset, value):
    struct.pack_into("<I", buf, offset, value)


class Oracle:
    def __init__(self, rom):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, length in [(0x02000000, 0x40000), (0x03000000, 0x8000), (0x04000000, 0x1000), (0x08000000, 0x800000)]:
            self.uc.mem_map(address, length)
        self.uc.mem_write(0x08000000, rom)
        self.uc.hook_add(UC_HOOK_MEM_READ, self.read)
        self.uc.hook_add(UC_HOOK_MEM_WRITE, self.write)
        self.accesses = []

    def read(self, uc, _access, address, size, _value, _user):
        if 0x04000000 <= address < 0x04001000:
            self.accesses.append(["read", address, size, int.from_bytes(uc.mem_read(address, size), "little")])

    def write(self, _uc, _access, address, size, value, _user):
        if 0x04000000 <= address < 0x04001000:
            self.accesses.append(["write", address, size, value])

    def run(self, entry, rows, device, argument=0, resources=None):
        self.uc.mem_write(0x02000000, bytes(0x40000))
        self.uc.mem_write(0x03000000, bytes(0x8000))
        self.uc.mem_write(0x04000000, bytes(0x1000))
        for row in (resources or []) + rows:
            self.uc.mem_write(row["address"], bytes.fromhex(row["bytes"]))
        self.uc.mem_write(DEVICE, bytes(device))
        self.accesses = []
        self.uc.reg_write(UC_ARM_REG_R0, argument)
        self.uc.reg_write(UC_ARM_REG_SP, 0x03007D00)
        self.uc.reg_write(UC_ARM_REG_LR, STOP | 1)
        self.uc.emu_start(0x08000000 + entry | 1, STOP, count=200000)
        if self.uc.reg_read(UC_ARM_REG_PC) != STOP:
            raise RuntimeError(f"bounded original {entry:x} did not return")
        after = [span(row["address"], self.uc.mem_read(row["address"], len(row["bytes"]) // 2)) for row in rows]
        return after, bytes(self.uc.mem_read(DEVICE, len(device))).hex()


def state(rng, i):
    sound, channels = bytearray(rng.randbytes(64)), bytearray(rng.randbytes(256))
    sound[0xA] = [0, 1, 2, 14, 255][i % 5]
    u32(sound, 0x1C, CHANNEL)
    flags = [0, 1, 2, 3, 4, 8, 0x40, 0x41, 0x42, 0x43, 0x80, 0xC0, 0xFF]
    times = [0, 1, 2, 7, 15, 127, 128, 255]
    levels = [0, 1, 2, 7, 15, 16, 31, 127, 128, 255]
    for j in range(4):
        at = j * 64
        channels[at] = flags[(i + j * 3) % len(flags)]
        channels[at + 1] = j + 1 | ([0, 8][(i + j) % 2])
        channels[at + 2] = [0, 1, 63, 127, 128, 255][(i // 3 + j) % 6]
        channels[at + 3] = [255, 128, 127, 63, 1, 0][(i // 7 + j) % 6]
        for k in range(4, 8):
            channels[at + k] = times[(i // (k - 3) + j) % len(times)]
        channels[at + 9] = levels[(i // 5 + j) % len(levels)]
        channels[at + 0xA] = levels[(i // 11 + j) % len(levels)]
        channels[at + 0xB] = times[(i // 3 + j) % len(times)]
        channels[at + 0xC] = [0, 1, 127, 255][(i + j) % 4]
        channels[at + 0xD] = times[(i // 2 + j) % len(times)]
        channels[at + 0x19] = levels[(i // 13 + j) % len(levels)]
        channels[at + 0x1C] = [0x11, 0x22, 0x44, 0x88][j]
        channels[at + 0x1D] = [0, 1, 2, 3, 255][(i + j) % 5]
        channels[at + 0x1E] = [0, 1, 255][(i // 7 + j) % 3]
        channels[at + 0x1F] = [0, 8, 127, 255][(i // 3 + j) % 4]
    u32(channels, 2 * 64 + 0x24, WAVE)
    u32(channels, 2 * 64 + 0x28, WAVE if i % 2 else WAVE + 16)
    device = bytearray(rng.randbytes(64))
    device[0x29] = [0, 0x3F, 0x40, 0x7F, 0x80, 255][i % 6]
    rows = [span(SOUND, sound), span(CHANNEL, channels), span(WAVE, rng.randbytes(16)), span(SLOT, struct.pack("<I", SOUND))]
    return rows, device


def generate(rom):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact local A3VE source")
    for name, offset, length, digest in ROUTINES:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"source mismatch: {name}")
    if struct.unpack_from("<2I", rom, 0x5ED78) != (0x0805F571, 0x0805F4B9):
        raise ValueError("PSG source initializer bindings differ")
    rng, oracle = random.Random(0xA3E5F570), Oracle(rom)
    key_off, gain, synthetic, native = [], [], [], []
    lookup = rng.randbytes(256)
    for i in range(256):
        device = rng.randbytes(64)
        argument = i | ((i % 4) << 24)
        after, device_after = oracle.run(0x5F4B8, [], device, argument)
        key_off.append(dict(kind=argument, input=[], after=after, device=device.hex(), device_after=device_after, accesses=oracle.accesses))
    for i in range(512):
        channel = bytearray(rng.randbytes(64))
        channel[2], channel[3] = (i % 256, (255 - i) % 256) if i < 256 else ((i * 13) % 256, (i * 7) % 256)
        rows, device = [span(CHANNEL, channel)], rng.randbytes(64)
        after, device_after = oracle.run(0x5F508, rows, device, CHANNEL)
        gain.append(dict(input=rows, after=after, device=device.hex(), device_after=device_after, accesses=oracle.accesses))
    for count, source, cases in [(1024, lookup, synthetic), (512, rom[0xF61BC:0xF61BC + 256], native)]:
        for i in range(count):
            rows, device = state(rng, i)
            after, device_after = oracle.run(0x5F570, rows, device, resources=[span(0x080F61BC, source)])
            cases.append(dict(input=rows, after=after, device=device.hex(), device_after=device_after, accesses=oracle.accesses))
    return dict(schemaVersion=1, romSha256=ROM_SHA256,
                scope="All original key-off/gain/four-channel update instructions execute; no helper/BIOS interception. MMIO is synthetic mapped-storage capture only, not hardware or playback.",
                routines=[dict(name=n, offset=o, length=l, sha256=d) for n, o, l, d in ROUTINES],
                updateBindingOffset=0x5ED78, keyOffBindingOffset=0x5ED7C, syntheticLookup=lookup.hex(),
                key_off=key_off, gain=gain, synthetic=synthetic, native=native)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = generate(args.rom.read_bytes())
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps(dict(keyOff=len(result["key_off"]), gain=len(result["gain"]),
                         syntheticUpdates=len(result["synthetic"]), nativeUpdates=len(result["native"]),
                         sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))


if __name__ == "__main__":
    main()
