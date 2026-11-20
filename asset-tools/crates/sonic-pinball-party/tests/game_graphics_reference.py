"""Independent native ball OBJ decoding; publishes only resource/frame hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"


def generate(path):
    with open(path, "rb") as f:
        rom = f.read(0x800001)
    if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
        raise ValueError("Exact local A3VE rev0 required")
    palette = struct.unpack_from("<11H", rom, 0x459DAC)
    frames = []
    for frame in range(4):
        rgba = bytearray(16*16*4)
        for tile in range(4):
            for py in range(8):
                for px in range(8):
                    packed = rom[0x4599AC+frame*128+tile*32+py*4+px//2]
                    index = (packed >> ((px % 2)*4)) & 15
                    if index == 0:
                        continue
                    color = palette[index]
                    rgb = [((color >> shift & 31) << 3) | ((color >> shift & 31) >> 2) for shift in (0, 5, 10)]
                    target = ((tile//2*8+py)*16+tile % 2*8+px)*4
                    rgba[target:target+4] = bytes(rgb+[255])
        frames.append(hashlib.sha256(rgba).hexdigest())
    return {"schemaVersion": 1, "romSha256": ROM_SHA256,
            "scope": "Four original ball OBJ pixel frames; diagnostic composition does not establish native camera/layer/OAM timing",
            "resources": [{"name": name, "offset": offset, "length": length, "sha256": hashlib.sha256(rom[offset:offset+length]).hexdigest()}
                          for name, offset, length in [("ballTiles", 0x4599AC, 512), ("ballPalette", 0x459DAC, 22), ("ballOam", 0x6DDC70, 8)]],
            "frameSha256": frames}


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--rom", required=True)
    p.add_argument("--generate", action="store_true")
    a = p.parse_args()
    value = generate(a.rom)
    target = Path(__file__).with_name("game-graphics-vectors.json")
    if a.generate:
        target.write_text(json.dumps(value, indent=2)+"\n", encoding="utf8")
    elif json.loads(target.read_text(encoding="utf8")) != value:
        raise SystemExit("Native ball frame fingerprints differ")
    print(json.dumps({"frames": 4, "sourceSha256": ROM_SHA256}))
