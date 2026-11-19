"""Execute original A3VE player/track lifecycle instructions on synthetic RAM.

Only the dynamic PSG oscillator callback is intercepted and recorded. Native
MPlayStart/Stop, trackStop and the reachable u8 SoundMode branch execute in full.
This tests state transitions, not sequence decoding, mixing or audible output.
Fixtures contain synthetic RAM/header scalars; no native instructions or audio.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROUTINES = {"open": (0x5F0F8, 0x78), "clear64": (0x5E130, 0x18), "clear64Wrapper": (0x5EDA4, 0x14),
            "start": (0x5F170, 0xE4), "stop": (0x5F254, 0x40),
            "trackStop": (0x5E610, 0x44), "soundMode": (0x5EF54, 0x98)}
PLAYER, TRACKS, CHANNELS, INFO, HEADER = 0x02001000, 0x02002000, 0x02004000, 0x02008000, 0x02009000
SIGNATURE = 0x68736D53


def put32(buf, offset, value):
    struct.pack_into("<I", buf, offset, value)


def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_PC, UC_ARM_REG_SP, UC_ARM_REG_LR
    with open(path, "rb") as f:
        rom = f.read(0x800001)
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("Exact local A3VE rev0 required")
    uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
    for base, size in [(0x08000000, 0x800000), (0x02000000, 0x40000), (0x03000000, 0x8000)]:
        uc.mem_map(base, size)
    uc.mem_write(0x08000000, rom)
    uc.mem_write(0x03007FF0, struct.pack("<I", INFO))
    stops = []

    def hook(cpu, address, size, data):
        if address == 0x03007E00:
            stops.append(cpu.reg_read(UC_ARM_REG_R0))
            cpu.reg_write(UC_ARM_REG_PC, cpu.reg_read(UC_ARM_REG_LR))

    uc.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(0xA3E5F170)
    cases = []
    specs = ["start"] * 256 + ["stop"] * 96 + ["trackStop"] * 96 + ["open"] * 128
    boundary_start = len(specs)
    specs += ["start", "stop", "trackStop"] * 12
    storage = 8
    uc.mem_write(0x020387CC, struct.pack("<I", 0x0805E131))

    def snapshot():
        return {"player": bytes(uc.mem_read(PLAYER, 0x40)).hex(),
                "tracks": [bytes(uc.mem_read(TRACKS+i*0x50, 0x50)).hex() for i in range(storage)],
                "channels": [{"address": CHANNELS+i*0x40, "bytes": bytes(uc.mem_read(CHANNELS+i*0x40, 0x40)).hex()} for i in range(16)],
                "soundInfo": bytes(uc.mem_read(INFO, 0x30)).hex()}

    def fingerprint(value):
        packed = bytes.fromhex(value["player"])
        packed += b"".join(bytes.fromhex(t) for t in value["tracks"])
        packed += b"".join(struct.pack("<I", c["address"])+bytes.fromhex(c["bytes"]) for c in value["channels"])
        packed += bytes.fromhex(value["soundInfo"])
        return hashlib.sha256(packed).hexdigest()

    for index, routine in enumerate(specs):
        boundary = index >= boundary_start
        storage = 16 if routine == "open" or boundary else 8
        player = bytearray(rng.randbytes(0x40))
        info = bytearray(rng.randbytes(0x30))
        capacity = [9, 12, 16][(index-boundary_start)//12] if boundary else index % 9
        player[8] = capacity
        player[9] = rng.choice([0, 1, 127, 255])
        player[0xB] = rng.choice([0, 1, 255])
        if boundary:
            player[0xB] = 0
        put32(player, 0, rng.choice([0, HEADER+0x100]))
        put32(player, 4, rng.choice([0, 1, 0x80000000, 0xFFFFFFFF, 0x10000]))
        put32(player, 0x2C, TRACKS)
        put32(player, 0x34, SIGNATURE if index % 13 else SIGNATURE+1)
        put32(info, 0, SIGNATURE if index % 7 else 0)
        put32(info, 0x20, rng.choice([0, 0x0805E3A9, 0x08000001]))
        put32(info, 0x2C, 0x03007E01)
        uc.mem_write(PLAYER, bytes(player))
        uc.mem_write(INFO, bytes(info))
        for ti in range(storage):
            track = bytearray(rng.randbytes(0x50))
            track[0] = rng.choice([0, 0x40, 0x80, 0xC0, 0xFF])
            put32(track, 0x20, rng.choice([0, CHANNELS+(ti % 8)*2*0x40, CHANNELS]))
            uc.mem_write(TRACKS+ti*0x50, bytes(track))
        for ci in range(16):
            channel = bytearray(rng.randbytes(0x40))
            channel[0] = rng.choice([0, 1, 0x80, 0xFF])
            channel[1] = rng.randrange(256)
            put32(channel, 0x34, CHANNELS+(ci+1)*0x40 if ci % 2 == 0 else 0)
            uc.mem_write(CHANNELS+ci*0x40, bytes(channel))
        h = {"address": HEADER, "count": rng.choice([0, 1, 2, 7, 8, 9, 255]),
             "priority": rng.choice([0, 1, 127, 255]), "reverb": rng.choice([0, 1, 127, 128, 255]),
             "voiceGroup": 0x08012000, "tracks": [0x08013000+i*0x100 for i in range(storage)]}
        if boundary:
            h["count"] = 255
        raw_header = struct.pack("<4BI", h["count"], 0, h["priority"], h["reverb"], h["voiceGroup"])
        raw_header += struct.pack("<"+"I"*storage, *h["tracks"])
        uc.mem_write(HEADER, raw_header)
        ti = index % storage
        before = snapshot()
        stops.clear()
        uc.reg_write(UC_ARM_REG_SP, 0x03007D00)
        uc.reg_write(UC_ARM_REG_LR, 0x03007F01)
        uc.reg_write(UC_ARM_REG_R0, PLAYER)
        count = [0, 1, 2, 8, 16, 17, 255, 256, 0xFFFFFFFF][index % 9]
        uc.reg_write(UC_ARM_REG_R1, TRACKS if routine == "open" else TRACKS+ti*0x50 if routine == "trackStop" else HEADER)
        if routine == "open":
            from unicorn.arm_const import UC_ARM_REG_R2
            uc.reg_write(UC_ARM_REG_R2, count)
        uc.emu_start(0x08000000+ROUTINES[routine][0]+1, 0x03007F00, count=100000)
        if uc.reg_read(UC_ARM_REG_PC) != 0x03007F00:
            raise RuntimeError("Native lifecycle instruction budget exceeded")
        cases.append({"routine": routine, "index": ti, "header": h, "playerAddress": PLAYER,
                      "trackAddress": TRACKS, "count": count, "before": before,
                      "afterSha256": fingerprint(snapshot()), "psgStops": stops.copy()})
    return {"schemaVersion": 1, "romSha256": ROM_SHA256,
            "boundary": "PSG callback intercepted; sequencer/mixer/playback not executed",
            "routines": [{"name": name, "offset": off, "length": length,
                          "sha256": hashlib.sha256(rom[off:off+length]).hexdigest()} for name, (off, length) in ROUTINES.items()],
            "cases": cases}


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--rom", required=True)
    p.add_argument("--generate", action="store_true")
    a = p.parse_args()
    actual = generate(a.rom)
    fixture = Path(__file__).with_name("mplay-vectors.json")
    if a.generate:
        fixture.write_text(json.dumps(actual, separators=(",", ":"))+"\n", encoding="utf8")
    elif json.loads(fixture.read_text(encoding="utf8")) != actual:
        raise SystemExit("Native lifecycle vectors differ")
    print(json.dumps({"cases": len(actual["cases"]), "sourceSha256": ROM_SHA256, "boundary": actual["boundary"]}))
