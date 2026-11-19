"""Execute the verified A2NE TransformSprite routine as a local test oracle.

Requires local Unicorn 2.1.3. This CPU execution is test-only; the Rust game
runtime uses independently implemented arithmetic and OBJ composition.
ROM, source, matrix vectors and optional rendered images remain under .local.
"""

import argparse
import hashlib
import json
import pathlib
import struct

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_INTR
from unicorn.arm_const import (
    UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R3, UC_ARM_REG_SP,
    UC_ARM_REG_LR, UC_ARM_REG_PC,
)


PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES = {
    "asm_non_matching_engine_TransformSprite.inc": "67bc6853f5e1aabea739df29903e0628ac2ed1b7",
    "src_sprite.c": "396cb0291f8ee4ad5744755f88cf5ee9608a7914",
    "include_sprite.h": "476ddddb2a6362f1e50c2304fe3e2851d6d43bbf",
}
SCALES = [-32768, -512, -384, -256, -192, -128, -1, 1, 128, 192, 256, 384, 512, 32767]
SIZES = [[(8, 8), (16, 16), (32, 32), (64, 64)],
         [(16, 8), (32, 8), (32, 16), (64, 32)],
         [(8, 16), (8, 32), (16, 32), (32, 64)]]


def next_seed(value):
    value ^= value << 13 & 0xffffffff
    value ^= value >> 17
    return (value ^ value << 5) & 0xffffffff


def signed16(value):
    return ((value + 32768) & 65535) - 32768


class NativeOracle:
    def __init__(self, rom):
        self.rom = rom
        self.cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.cpu.mem_map(0x08000000, len(rom))
        self.cpu.mem_write(0x08000000, rom)
        self.cpu.mem_map(0x02000000, 0x40000)
        self.cpu.mem_map(0x03000000, 0x8000)
        self.cpu.hook_add(UC_HOOK_INTR, self.division)
        self.oam = struct.unpack_from("<I", rom, 0x4a14)[0]
        self.tables = [self.u32(0xf40d4 + i * 4) - 0x08000000 for i in range(6)]

    def u32(self, offset):
        return struct.unpack_from("<I", self.rom, offset)[0]

    def division(self, cpu, interrupt, _):
        pc = cpu.reg_read(UC_ARM_REG_PC)
        if interrupt != 2 or bytes(cpu.mem_read(pc - 2, 2)) != b"\x06\xdf":
            raise ValueError("Unexpected native interrupt")
        def signed(register):
            value = cpu.reg_read(register)
            return value if value < 0x80000000 else value - 0x100000000
        numerator, denominator = signed(UC_ARM_REG_R0), signed(UC_ARM_REG_R1)
        if not denominator:
            raise ValueError("Zero native division outside oracle domain")
        quotient = abs(numerator) // abs(denominator)
        if (numerator < 0) != (denominator < 0):
            quotient = -quotient
        cpu.reg_write(UC_ARM_REG_R0, quotient & 0xffffffff)
        cpu.reg_write(UC_ARM_REG_R1, (numerator - quotient * denominator) & 0xffffffff)
        cpu.reg_write(UC_ARM_REG_R3, abs(quotient))

    def dimensions(self, animation, frame):
        base = self.u32(self.tables[1] + animation * 4) - 0x08000000
        return self.rom[base + frame * 12:base + frame * 12 + 12]

    def transform(self, dimensions, rotation, scale_x, scale_y, x=0, y=0):
        cpu = self.cpu
        cpu.mem_write(0x0200000c, struct.pack("<II", 0x02000100, 0))
        cpu.mem_write(0x02000100, dimensions)
        cpu.mem_write(0x02000080, struct.pack("<Hhhhh", rotation, scale_x, scale_y, x, y))
        cpu.reg_write(UC_ARM_REG_R0, 0x02000000)
        cpu.reg_write(UC_ARM_REG_R1, 0x02000080)
        cpu.reg_write(UC_ARM_REG_SP, 0x03007000)
        cpu.reg_write(UC_ARM_REG_LR, 0x02001001)
        cpu.emu_start(0x08004861, 0x02001000, count=10000)
        if cpu.reg_read(UC_ARM_REG_PC) != 0x02001000:
            raise ValueError("Native transform exceeded instruction budget")
        matrix = [struct.unpack("<h", cpu.mem_read(self.oam + i * 8, 2))[0] for i in range(4)]
        return matrix + list(struct.unpack("<hh", cpu.mem_read(0x02000016, 4)))

    def show_frames(self, variant, count):
        scripts = self.u32(self.tables[0] + 9 * 4) - 0x08000000
        cursor = self.u32(scripts + variant * 4) - 0x08000000
        tiles = bytearray(32768)
        palette = [0] * 256
        frames = []
        for _ in range(4096):
            opcode = struct.unpack_from("<i", self.rom, cursor)[0]
            arg = lambda i: self.u32(cursor + i * 4)
            if opcode >= 0:
                frame = arg(1)
                frames.append((frame, bytes(tiles), palette.copy()))
                cursor += 8
                if len(frames) == count:
                    return frames
            elif opcode == -1:
                eight = arg(1) >> 31
                stride = 64 if eight else 32
                start = self.tables[5 if eight else 4] + (arg(1) & 0x7fffffff) * stride
                size = arg(2) * stride
                tiles[:size] = self.rom[start:start + size]
                cursor += 12
            elif opcode == -2:
                packed = arg(2)
                count_colors, insert = packed & 65535, packed >> 16
                start = self.tables[3] + arg(1) * 32
                palette[insert:insert + count_colors] = struct.unpack_from(f"<{count_colors}H", self.rom, start)
                cursor += 12
            elif opcode == -3:
                cursor -= struct.unpack_from("<i", self.rom, cursor + 4)[0] * 4
            elif opcode in (-5, -7, -11, -12):
                cursor += 8
            elif opcode in (-6, -8):
                cursor += 12
            else:
                raise ValueError(f"Unsupported walking oracle opcode {opcode}")
        raise ValueError("Walking script exceeded oracle budget")

    def compose(self, frame, tiles, palette, rotation, scale_x, scale_y, double_size):
        dimensions = self.dimensions(9, frame)
        _, first, count, width, height, _, _ = struct.unpack("<BBHHHhh", dimensions)
        pa, pb, pc, pd, base_x, base_y = self.transform(dimensions, rotation, scale_x, scale_y)
        if double_size:
            base_x -= width // 2
            base_y -= height // 2
        oam = self.u32(self.tables[2] + 9 * 4) - 0x08000000
        parts = []
        for part in range(count):
            a0, a1, a2 = struct.unpack_from("<HHH", self.rom, oam + (first + part) * 6)
            if a0 & 0x1c00 or a1 & 0x3e00:
                raise ValueError("Walking oracle entered unported OAM mode")
            w, h = SIZES[a0 >> 14][a1 >> 14]
            double = double_size or bool(a0 & 0x200)
            bw, bh = (w * 2, h * 2) if double else (w, h)
            x, y = a1 & 511, a0 & 255
            if x >= 256: x -= 512
            if y >= 128: y -= 256
            parts.append((base_x + x, base_y + y, w, h, bw, bh, a0, a2))
        left, top = min(p[0] for p in parts), min(p[1] for p in parts)
        right, bottom = max(p[0] + p[4] for p in parts), max(p[1] + p[5] for p in parts)
        out_w, out_h = right - left, bottom - top
        rgba, ranks = bytearray(out_w * out_h * 4), [(4, 128)] * (out_w * out_h)
        for part, (x, y, w, h, bw, bh, a0, a2) in enumerate(parts):
            eight = bool(a0 & 0x2000)
            for dy in range(bh):
                for dx in range(bw):
                    sx = ((pa * (dx - bw // 2) + pb * (dy - bh // 2)) >> 8) + w // 2
                    sy = ((pc * (dx - bw // 2) + pd * (dy - bh // 2)) >> 8) + h // 2
                    if not (0 <= sx < w and 0 <= sy < h): continue
                    tile = (a2 & 1023) + (sy // 8) * (w // 8) + sx // 8
                    within = (sy % 8) * 8 + sx % 8
                    packed = tiles[tile * (64 if eight else 32) + (within if eight else within // 2)]
                    index = packed if eight else packed >> (within % 2 * 4) & 15
                    if not index: continue
                    destination = (y - top + dy) * out_w + x - left + dx
                    rank = ((a2 >> 10) & 3, part)
                    if rank >= ranks[destination]: continue
                    ranks[destination] = rank
                    color = palette[index + (0 if eight else (a2 >> 12) * 16)]
                    channels = [(color >> shift) & 31 for shift in (0, 5, 10)]
                    rgba[destination * 4:destination * 4 + 4] = bytes([(v << 3) | (v >> 2) for v in channels] + [255])
        return out_w, out_h, -left, -top, bytes(rgba)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", type=pathlib.Path, required=True)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if ".local" not in args.output_dir.resolve().parts:
        parser.error("Native vectors and images must remain under .local")
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Wrong A2NE source ROM")
    for name, expected in SOURCES.items():
        data = (args.reference_dir / name).read_bytes()
        if hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest() != expected:
            raise ValueError(f"Wrong pinned source: {name}")
    oracle, seed, result = NativeOracle(rom), 0xa2aff1e0, bytearray()
    for row in range(4096):
        values = []
        for _ in range(6):
            seed = next_seed(seed)
            values.append(seed)
        animation, frame = [0, 9, 10, 11][values[0] % 4], values[1] % 4
        transform = oracle.transform(oracle.dimensions(animation, frame),
            (row % 1024) | (values[0] & 0xfc00), SCALES[values[4] % len(SCALES)],
            SCALES[values[5] % len(SCALES)], signed16(values[2]), signed16(values[3]))
        result.extend(struct.pack("<6i", *transform))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "transforms.bin").write_bytes(result)
    images = []
    for variant in range(6):
        for show, (frame, tiles, palette) in enumerate(oracle.show_frames(variant, 6)):
            for rotation in (0, 64, 128, 896, 960):
                for scale_x in (-256, 256):
                    double_size = show % 2 == 1
                    w, h, x, y, rgba = oracle.compose(frame, tiles, palette, rotation, scale_x, 256, double_size)
                    images.append(dict(variant=variant, show=show, frame=frame, rotation=rotation,
                        scaleX=scale_x, scaleY=256, doubleSize=double_size, width=w, height=h,
                        originX=x, originY=y, rgbaSha256=hashlib.sha256(rgba).hexdigest()))
                    (args.output_dir / f"walk-{variant}-{show}-{rotation}-{scale_x}.rgba").write_bytes(rgba)
    report = dict(schemaVersion=1, referenceCommit=PIN, sourceGitBlobs=SOURCES,
        method="Actual verified ROM TransformSprite Thumb execution (test-only Unicorn 2.1.3, bounded BIOS integer-Div shim); independent native OBJ pixel sampler",
        routineOffset=0x4860, routineBytes=0x25c, routineSha256=hashlib.sha256(rom[0x4860:0x4abc]).hexdigest(),
        seed=0xa2aff1e0, cases=4096, rowWords=6, outputBytes=len(result), outputSha256=hashlib.sha256(result).hexdigest(),
        images=images)
    args.summary.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "images"}) + f" images={len(images)}")


if __name__ == "__main__":
    main()
