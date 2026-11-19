"""A3VE note allocation and PCM-rate original-instruction reference.

All state, command, key-split, bank and sample-header bytes are synthetic.
Controller cases intercept unlink/parameter/volume/rate dependencies precisely;
native compositions execute those original leaves and only intercept the
synthetic PSG-rate callback. The ARM UMULL helper executes natively in Unicorn.
Native pitch tables are read locally and never emitted. No mixer, oscillator,
MMIO, playback or device timing is implemented or inferred by this oracle.
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
    ("note", 0x5E684, 0x200, "d6674254ab734e962dcde6274c080175cc66fd8bba6f855d7d8dee3cf0a10ca1"),
    ("pitch", 0x5E914, 0x64, "d0a5917a15e89ff2b3a224d270d63c04e263b3d09c88af825804377d9eacde9c"),
    ("multiply", 0x5DCF8, 0x10, "1addb8be644cbaa9421a91f6cd6d91407907e9c92320f56ae49804b98fc29a81"),
    ("keys", 0xF5FE8, 180, "52e19ba195ffad7aa82ac71bf26543d912f3e3a763e40c755ce0e71b674ef9e7"),
    ("scales", 0xF609C, 48, "6b73747d3fb29635eeda13481d08a06e5371958ee746967aa892fa5390509404"),
    ("psg_pitch", 0x5F410, 0xA8, "54c17aaded94bc0d4fd4d86ce69df0e0dc15679d7b3046643be4ff45632dc6e1"),
    ("psg_keys", 0xF60E4, 132, "2ab14116232b5768777f67dd371a380bd88fdc97858aa050306b132d05faa03e"),
    ("psg_scales", 0xF6168, 24, "34e0ae7c2f912add79c2e61ed8347ff085000bc39f0173883ccbaff150205ee5"),
    ("noise", 0xF6180, 60, "197f0679262884d48dd459dcfe690c0a41e8a73cf76cb62838119fc9cb0e7ab6"),
    ("waits", 0xF61CC, 49, "952d81c712713fc630d9d622d5f79eec8189f2a4cf05a78442857b635f0d23f8"),
    ("dispatch", 0xF5F58, 144, "07070902ece8f71e1302f45bc09ae93555c715dc7c578a7eda7901a981f6358f"),
    ("unlink", 0x5E148, 0x20, "c3d23903d0b87a7b19fcd84aa0234c674e3014a9a2835853727defeaad3422dc"),
    ("unlink_wrapper", 0x5ED90, 0x14, "2f962ba32754a3208bbbd994a2427ed0c4e9f8b281f4bc0c5464d6b130cf3f30"),
    ("clear_lfo", 0x5E8C4, 0x1C, "4317009f4748028f27c2ac3672106a2c22d1eb90ad97d417fbe8f76d31477982"),
    ("parameters", 0x5F35C, 0xB4, "00879cc3e04cd6caf909a6f1763a16ae1ceff8b9f40b83afea83b283b3dc35f5"),
    ("volume", 0x5E654, 0x30, "04988bf8e911a12b477d9aafa24342b68e83960a06796e555869cc67b24bd4d2"),
]
PLAYER, TRACK, SOUND, PSG = 0x02001000, 0x02002000, 0x02004000, 0x02005000
BANK, KEYMAP, COMMAND, SAMPLE = 0x02006000, 0x02007000, 0x02008000, 0x02009000
STOP, SLOT, CALLBACK = 0x03007F00, 0x03007FF0, 0x0200A001


def u32(buf, off, value):
    struct.pack_into("<I", buf, off, value)


def span(address, data):
    return dict(address=address, bytes=bytes(data).hex())


class Oracle:
    def __init__(self, rom):
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for address, length in [(0x02000000, 0x40000), (0x03000000, 0x8000), (0x08000000, 0x800000)]:
            self.uc.mem_map(address, length)
        self.uc.mem_write(0x08000000, rom)
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, uc, address, _size, _user):
        if address not in self.intercept:
            return
        target, operands = self.intercept[address]
        registers = {0: UC_ARM_REG_R0, 1: UC_ARM_REG_R1, 2: UC_ARM_REG_R2,
                     4: UC_ARM_REG_R4, 5: UC_ARM_REG_R5}
        call = dict(target=target, r0=None, r1=None, r2=None, r4=None, r5=None)
        for index in operands:
            call[f"r{index}"] = uc.reg_read(registers[index])
        self.calls.append(call)
        occurrence = self.occurrences.get(target, 0) + 1
        self.occurrences[target] = occurrence
        result = 0
        for action in self.actions:
            if action["target"] == target and action["occurrence"] == occurrence:
                for at, width, value in action.get("writes", []):
                    uc.mem_write(at, value.to_bytes(4, "little")[:width])
                result = action.get("return", result)
        uc.reg_write(UC_ARM_REG_R0, result)
        uc.reg_write(UC_ARM_REG_PC, uc.reg_read(UC_ARM_REG_LR))

    def run(self, entry, spans, args, mode, actions=None, resources=None):
        self.uc.mem_write(0x02000000, bytes(0x40000))
        self.uc.mem_write(0x03000000, bytes(0x8000))
        for row in (resources or []) + spans:
            self.uc.mem_write(row["address"], bytes.fromhex(row["bytes"]))
        self.calls, self.occurrences, self.actions = [], {}, actions or []
        self.intercept = {CALLBACK & ~1: (CALLBACK, [0, 1, 2])}
        if mode == "controller":
            for target, operands in [(0x0805ED90, [0]), (0x0805F35C, [0, 1]),
                                     (0x0805E654, [4, 5]), (0x0805E914, [0, 1, 2])]:
                self.intercept[target] = (target, operands)
        for register, value in zip([UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2], args):
            self.uc.reg_write(register, value)
        self.uc.reg_write(UC_ARM_REG_SP, 0x03007D00)
        self.uc.reg_write(UC_ARM_REG_LR, STOP | 1)
        self.uc.emu_start(0x08000000 + entry | 1, STOP, count=200000)
        if self.uc.reg_read(UC_ARM_REG_PC) != STOP:
            raise RuntimeError(f"bounded original {entry:x} did not return")
        after = [span(row["address"], self.uc.mem_read(row["address"], len(row["bytes"]) // 2)) for row in spans]
        return after, self.uc.reg_read(UC_ARM_REG_R0)


def state(rng, i, actual_psg=False):
    p, tracks = bytearray(rng.randbytes(64)), bytearray(rng.randbytes(160))
    current = TRACK + 80 * (i % 2)
    at = current - TRACK
    p[9] = [0, 1, 20, 127, 255][i % 5]
    sound = bytearray(rng.randbytes(0x350))
    sound[6] = [0, 1, 2, 4, 8, 12][(i // 5) % 6]
    u32(sound, 0x1C, PSG if i % 11 else 0)
    u32(sound, 0x30, 0x0805F411 if actual_psg else CALLBACK)
    psg = bytearray(rng.randbytes(7 * 64))
    # All supplied channel links are coherent, including dormant channels.
    addresses = [SOUND + 0x50 + 64 * j for j in range(12)] + [PSG + 64 * j for j in range(7)]
    records = [bytearray(sound[0x50 + 64 * j:0x90 + 64 * j]) for j in range(12)]
    records += [bytearray(psg[64 * j:64 * (j + 1)]) for j in range(7)]
    groups = [[], []]
    for j, record in enumerate(records):
        record[0] = [0, 1, 2, 4, 8, 0x40, 0x80, 0xC0, 0xFF][(i + j * 3) % 9]
        record[1] = j % 8
        u32(record, 0x24, SAMPLE + 8 * (j % 16))
        record[0x13] = [0, 1, 20, 127, 255][(i // 3 + j) % 5]
        if i % 13 == 0:
            record[0], record[0x13] = 1, 255
        if i % 17 == 0:
            record[0], record[0x13] = 0x40, (j % 2) * 127
        owner = (i + j) % 2
        groups[owner].append(j)
    for owner, group in enumerate(groups):
        u32(tracks, owner * 80 + 0x20, addresses[group[0]])
        for index, j in enumerate(group):
            u32(records[j], 0x2C, TRACK + owner * 80)
            u32(records[j], 0x30, addresses[group[index - 1]] if index else 0)
            u32(records[j], 0x34, addresses[group[index + 1]] if index + 1 < len(group) else 0)
    for j, record in enumerate(records[:12]):
        sound[0x50 + 64 * j:0x90 + 64 * j] = record
    for j, record in enumerate(records[12:]):
        psg[64 * j:64 * (j + 1)] = record
    root = [0, 0x40, 0x80, 0xC0][(i // 7) % 4]
    kind = (i // 3) % 8
    tracks[at] = [0, 0x80, 0x8F, 0xFF][i % 4]
    tracks[at + 5], tracks[at + 6] = i % 16, (i * 3) % 128
    tracks[at + 0x1B] = [0, 1, 255][i % 3]
    tracks[at + 0x18] = i % 4
    tracks[at + 0x1D] = [0, 1, 20, 127, 255][(i // 5) % 5]
    u32(tracks, at + 0x40, COMMAND)
    pan = [0, 8, 0x10, 0x30, 0x70, 0x80, 0xBF, 0xC0, 0xFF][i % 9]
    tracks[at + 0x24:at + 0x28] = bytes([root | kind, (i * 7) % 256, i % 256, pan])
    u32(tracks, at + 0x28, BANK if root else SAMPLE)
    u32(tracks, at + 0x2C, KEYMAP if root else 0x11223344)
    bank = bytearray(rng.randbytes(16 * 12))
    for j in range(16):
        bank[j * 12:j * 12 + 4] = bytes([kind | (0x40 if i % 19 == 0 else 0), (i + j * 17) % 256, j, pan])
        u32(bank, j * 12 + 4, SAMPLE + 8 * j)
    keymap = bytes((i + j * 7) % 16 for j in range(16))
    key = i % 16 if root else [0, 1, 60, 127][i % 4]
    command = [[0x80, 0, 0, 0], [key, 0x80, 0, 0], [key, i % 128, 0x80, 0],
               [key, i % 128, [0, 1, 127][i % 3], 0x80]][i % 4]
    samples = bytearray(rng.randbytes(16 * 8))
    for j in range(16):
        u32(samples, 8 * j + 4, [0, 1, 0xFFFFFFFF, 16777216, rng.getrandbits(32)][(i + j) % 5])
    rows = [span(PLAYER, p), span(TRACK, tracks), span(SOUND, sound), span(PSG, psg),
            span(BANK, bank), span(KEYMAP, keymap), span(COMMAND, command), span(SAMPLE, samples),
            span(SLOT, struct.pack("<I", SOUND)), span(0x020387C8, struct.pack("<I", 0x0805E149))]
    return rows, current


def generate(rom):
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("expected exact local A3VE source")
    for name, offset, length, digest in ROUTINES:
        if hashlib.sha256(rom[offset:offset + length]).hexdigest() != digest:
            raise ValueError(f"source mismatch: {name}")
    if struct.unpack_from("<I", rom, 0x5EEA0)[0] != 0x0805E685:
        raise ValueError("note-handler initializer binding mismatch")
    if struct.unpack_from("<I", rom, 0x5ED80)[0] != 0x0805F411:
        raise ValueError("PSG-rate initializer binding mismatch")
    rng, oracle = random.Random(0xA3E5E684), Oracle(rom)
    controller, native, ticks = [], [], []
    synthetic_waits = bytes((i * 7 + 1) % 256 for i in range(49))
    native_resources = [span(0x080F61CC, rom[0xF61CC:0xF61CC + 49]),
                        span(0x080F5FE8, rom[0xF5FE8:0xF5FE8 + 180]), span(0x080F609C, rom[0xF609C:0xF609C + 48]),
                        span(0x080F60E4, rom[0xF60E4:0xF60E4 + 132]), span(0x080F6168, rom[0xF6168:0xF6168 + 24]),
                        span(0x080F6180, rom[0xF6180:0xF6180 + 60])]
    for mode, cases in [("controller", controller), ("native", native)]:
        for i in range(512):
            rows, track = state(rng, i, mode == "native" and i % 2 == 0)
            actions = []
            if mode == "controller":
                rows.append(span(0x080F61CC, synthetic_waits))
                if i % 23 == 0:
                    actions.append(dict(target=0x0805F35C, occurrence=1, writes=[[track + 8, 1, 0x80], [SLOT, 4, 0]]))
                for target in [CALLBACK, 0x0805E914]:
                    actions.append(dict(target=target, occurrence=1, **{"return": 0x89ABCDEF}))
            else:
                actions.append(dict(target=CALLBACK, occurrence=1, **{"return": 0x76543210}))
            after, _ = oracle.run(0x5E684, rows, [i % 49, PLAYER, track], mode, actions,
                                  native_resources if mode == "native" else None)
            cases.append(dict(duration=i % 49, track=track, input=rows, after=after, actions=actions, calls=oracle.calls))
    for i in range(128):
        rows, track = state(rng, i, i % 2 == 0)
        memory = {row["address"]: bytearray.fromhex(row["bytes"]) for row in rows}
        p, tracks, sound = memory[PLAYER], memory[TRACK], memory[SOUND]
        p[8] = 2
        u32(p, 4, 1)
        u32(p, 0xC, 0xFFFFFFFF)
        struct.pack_into("<4H", p, 0x20, [0, 150, 300][i % 3], [0, 149][i % 2], 0, 0)
        u32(p, 0x2C, TRACK)
        u32(p, 0x34, 0x68736D53)
        u32(p, 0x38, 0)
        tracks[0], tracks[80] = 0, 0
        tracks[track - TRACK] = 0x8F
        tracks[track - TRACK + 1] = 0
        u32(sound, 0x34, 0x0200B000)
        u32(sound, 0x38, 0x0805E685)
        key = i % 16
        memory[COMMAND] = bytearray([0xCF + i % 49, key, (i * 3) % 128, i % 128, 0xB0, 0xCE, key, 0xB0, 0xB1])
        rows = [span(address, data) for address, data in memory.items()]
        actions = [dict(target=CALLBACK, occurrence=j + 1, **{"return": 0x76543210 + j}) for j in range(8)]
        resources = native_resources + [span(0x0200B000, rom[0xF5F58:0xF5F58 + 144])]
        after, _ = oracle.run(0x5E3A8, rows, [PLAYER, 0, 0], "native", actions, resources)
        ticks.append(dict(input=rows, after=after, actions=actions, calls=oracle.calls))
    synthetic_keys = bytes((i % 12) | ((i // 12) % 16) << 4 for i in range(180))
    synthetic_scales = struct.pack("<12I", *[rng.getrandbits(32) for _ in range(12)])
    synthetic_resources = [span(0x080F5FE8, synthetic_keys), span(0x080F609C, synthetic_scales)]
    synthetic_psg_keys = bytes((i % 12) | ((i // 12) % 16) << 4 for i in range(132))
    synthetic_psg_scales = struct.pack("<12h", *[rng.randrange(-32768, 32768) for _ in range(12)])
    synthetic_noise = rng.randbytes(60)
    synthetic_resources += [span(0x080F60E4, synthetic_psg_keys), span(0x080F6168, synthetic_psg_scales), span(0x080F6180, synthetic_noise)]
    pitch_synthetic, pitch_native, multiply = [], [], []
    for mode, count, resources, cases in [("synthetic", 2048, synthetic_resources, pitch_synthetic),
                                          ("native", 2048, native_resources, pitch_native)]:
        for i in range(count):
            key = (i % 256) | ([0, 0x100, 0xFFFF0000, 0x12340000][(i // 256) % 4])
            fine = [0, 1, 63, 64, 127, 128, 254, 255][i // 256 % 8] | ((i % 4) << 16)
            frequency = [0, 1, 0xFFFFFFFF, rng.getrandbits(32)][i % 4]
            rows = [span(SAMPLE, struct.pack("<2I", 0x12345678, frequency))]
            _, result = oracle.run(0x5E914, rows, [SAMPLE, key, fine], "pitch", resources=resources)
            if oracle.calls:
                raise RuntimeError("pitch unexpectedly crossed a callback boundary")
            cases.append(dict(key=key, fine=fine, frequency=frequency, result=result))
    psg_synthetic, psg_native = [], []
    for resources, cases in [(synthetic_resources, psg_synthetic), (native_resources, psg_native)]:
        for i in range(2048):
            kind = 1 + i % 7 | ((i % 3) << 16)
            key = i % 256 | ((i % 5) << 16)
            fine = [0, 1, 63, 64, 127, 128, 254, 255][i // 256 % 8] | ((i % 3) << 24)
            _, result = oracle.run(0x5F410, [], [kind, key, fine], "pitch", resources=resources)
            if oracle.calls:
                raise RuntimeError("PSG rate unexpectedly crossed a callback boundary")
            cases.append(dict(kind=kind, key=key, fine=fine, result=result))
    for i in range(256):
        a, b = rng.getrandbits(32), rng.getrandbits(32)
        _, result = oracle.run(0x5DCF8, [], [a, b, 0], "multiply")
        multiply.append(dict(first=a, second=b, result=result))
    return dict(schemaVersion=1, romSha256=ROM_SHA256,
                scope="Synthetic state/banks/sample headers;512 dependency-intercepted notes;512 complete native notes and128 ticks execute PCM and known PSG pitch, intercepting only synthetic unknown PSG callbacks;4352 unhooked synthetic math calls and4096 local-table pitch calls; no playback/device execution",
                routines=[dict(name=n, offset=o, length=l, sha256=d) for n, o, l, d in ROUTINES],
                initializerBindingOffset=0x5EEA0, psgBindingOffset=0x5ED80,
                syntheticKeys=synthetic_keys.hex(), syntheticScales=synthetic_scales.hex(),
                syntheticPsgKeys=synthetic_psg_keys.hex(), syntheticPsgScales=synthetic_psg_scales.hex(), syntheticNoise=synthetic_noise.hex(),
                controller=controller, native=native, ticks=ticks, pitch_synthetic=pitch_synthetic, pitch_native=pitch_native,
                psg_synthetic=psg_synthetic, psg_native=psg_native, multiply=multiply)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = generate(args.rom.read_bytes())
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(json.dumps(dict(controller=len(result["controller"]), native=len(result["native"]),
                         ticks=len(result["ticks"]),
                         pitchSynthetic=len(result["pitch_synthetic"]), pitchNative=len(result["pitch_native"]),
                         psgSynthetic=len(result["psg_synthetic"]), psgNative=len(result["psg_native"]),
                         multiply=len(result["multiply"]), sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))


if __name__ == "__main__":
    main()
