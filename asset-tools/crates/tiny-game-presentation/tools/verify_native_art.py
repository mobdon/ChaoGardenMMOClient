#!/usr/bin/env python3
"""Verify local exported pixels against original Thumb loaders and OAM helpers.

No native bytes are embedded or downloaded. Requires Unicorn2.1.4 and Pillow.
BIOS copy/LZ calls are explicit functional host shims, not hardware parity.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys

BASE = 0x02000000
IWRAM = 0x03000000
VRAM = 0x06000000
PAL = 0x05000000
OAM = 0x07000000
END = BASE + 0x3F000
IDENTITIES = {
    "A2NE": "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca",
    "ASOE": "989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0",
    "A3VE": "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb",
}
SIZES = (
    ((8, 8), (16, 16), (32, 32), (64, 64)),
    ((16, 8), (32, 8), (32, 16), (64, 32)),
    ((8, 16), (8, 32), (16, 32), (32, 64)),
)


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Native:
    def __init__(self, game, source):
        import unicorn as u
        from unicorn import arm_const as r

        if u.__version__ != "2.1.4":
            raise ValueError("expected Unicorn2.1.4")
        self.regs, self.source, self.game = r, source, game
        self.cpu = u.Uc(u.UC_ARCH_ARM, u.UC_MODE_THUMB)
        for start, length in [(BASE, 0x40000), (IWRAM, 0x8000),
                              (0x04000000, 0x1000), (PAL, 0x1000),
                              (VRAM, 0x20000), (OAM, 0x1000)]:
            self.cpu.mem_map(start, length)
        self.cpu.mem_write(BASE, source)
        copies, fast, lz = {
            "A2NE": ([0x11958], [0x11954], [0x11968, 0x1196C]),
            "A2NE-Bounce": ([0x11958], [0x11954], [0x11968, 0x1196C]),
            "ASOE": ([0xDB08], [], [0xDB18]),
            "A3VE": ([0x11AE4], [0x11AE0], [0x11AF4]),
        }[game]
        self.calls = []

        def hook(cpu, pc, size, _):
            offset = pc - BASE
            src, dst, control = self.r(0), self.r(1), self.r(2)
            if offset in copies + fast:
                width = 4 if offset in fast or control & (1 << 26) else 2
                count = control & 0x1FFFFF
                if offset in fast:
                    count = (count + 7) & ~7
                if count * width > 0x20000:
                    raise ValueError("copy exceeds bounded video/RAM span")
                if control & (1 << 24):
                    data = bytes(cpu.mem_read(src, width)) * count
                else:
                    data = bytes(cpu.mem_read(src, count * width))
            else:
                header = struct.unpack("<I", cpu.mem_read(src, 4))[0]
                length, cursor, data = header >> 8, src + 4, bytearray()
                if length > 0x10000 or header & 255 not in (1, 0x10):
                    raise ValueError("unexpected source LZ stream")
                while len(data) < length:
                    flags = cpu.mem_read(cursor, 1)[0]
                    cursor += 1
                    for bit in range(7, -1, -1):
                        if len(data) >= length:
                            break
                        if flags & (1 << bit):
                            a, b = cpu.mem_read(cursor, 2)
                            cursor += 2
                            distance = ((a & 15) << 8 | b) + 1
                            for _ in range((a >> 4) + 3):
                                data.append(data[-distance])
                        else:
                            data.append(cpu.mem_read(cursor, 1)[0])
                            cursor += 1
                data = bytes(data[:length])
            cpu.mem_write(dst, data)
            self.calls.append({"callback": hex(offset), "source": hex(src),
                               "destination": hex(dst), "length": len(data),
                               "sha256": sha(data)})
            cpu.reg_write(r.UC_ARM_REG_PC, cpu.reg_read(r.UC_ARM_REG_LR))

        for offset in copies + fast + lz:
            self.cpu.hook_add(u.UC_HOOK_CODE, hook, begin=BASE + offset,
                              end=BASE + offset)
        language = {"A2NE": 0x3330, "A2NE-Bounce":0x3330, "ASOE": 0x2760, "A3VE": 0x4250}[game]
        self.cpu.mem_write(IWRAM + language + 0x10, b"\1")
        start, stop = {"A2NE": (0xA944, 0xAB20), "A2NE-Bounce":(0xD294,0xD3A4), "ASOE": (0xB55C, 0xB70E),
                       "A3VE": (0xD294, 0xD458)}[game]
        self.run(start, stop=BASE + stop)
        # Retained normal appearance0, from each original Garden palette update.
        if game in ("ASOE", "A3VE"):
            at = {"ASOE": 0x1C700, "A3VE": 0x24050}[game]
            self.cpu.mem_write(PAL + 0x200, source[at:at + 32])

    def r(self, index):
        return self.cpu.reg_read(getattr(self.regs, f"UC_ARM_REG_R{index}"))

    def run(self, offset, args=(), stop=END):
        self.cpu.reg_write(self.regs.UC_ARM_REG_SP, IWRAM + 0x7D00)
        self.cpu.reg_write(self.regs.UC_ARM_REG_LR, END | 1)
        for index, value in enumerate(args):
            self.cpu.reg_write(getattr(self.regs, f"UC_ARM_REG_R{index}"), value)
        self.cpu.emu_start(BASE + offset | 1, stop, count=500000)
        pc = self.cpu.reg_read(self.regs.UC_ARM_REG_PC)
        if pc != stop:
            raise AssertionError(f"incomplete original callback {offset:#x}: {pc:#x}")

    def read(self, address, size):
        return bytes(self.cpu.mem_read(address, size))

    def word(self, address):
        return struct.unpack("<H", self.read(address, 2))[0]

    def layout(self):
        return {key: sha(self.read(at, length)) for key, at, length in [
            ("bgTilesSha256", VRAM, 0x8000),
            ("bgExtendedTilesSha256", VRAM+0x8000, 0x4000),
            ("bgMapsSha256", VRAM + 0xC000, 0x2000),
            ("objTilesSha256", VRAM + 0x10000, 0x8000),
            ("bgPaletteSha256", PAL, 512),
            ("objPaletteSha256", PAL + 0x200, 512),
        ]}

    def color(self, bank, index, obj=False):
        word = self.word(PAL + (0x200 if obj else 0) + (bank * 16 + index) * 2)
        return bytes(((v << 3) | (v >> 2)) for v in
                     (word & 31, (word >> 5) & 31, (word >> 10) & 31))

    def tile_pixel(self, tile, x, y, bank=0, obj=False, depth=4):
        start = VRAM + (0x10000 if obj else 0) + tile * (64 if depth == 8 else 32)
        address = start + y * (8 if depth == 8 else 4) + (x if depth == 8 else x // 2)
        packed = self.read(address, 1)[0]
        index = packed if depth == 8 else packed >> ((x % 2) * 4) & 15
        return self.color(0 if depth == 8 else bank, index, obj) + bytes((255 if index else 0,))

    def rectangle(self, tile, bank, width, height, obj=False):
        data = bytearray()
        for y in range(height):
            for x in range(width):
                at = tile + (y // 8) * (width // 8) + x // 8
                data += self.tile_pixel(at, x % 8, y % 8, bank, obj)
        return bytes(data)

    def background(self, layers, height=160, opaque=True):
        output = bytearray((self.color(0, 0) + b"\xff") * (240 * height)) if opaque else bytearray(240*height*4)
        for offset, depth in layers:
            for y in range(height):
                for x in range(240):
                    word = self.word(VRAM + offset + (y // 8) * 64 + (x // 8) * 2)
                    sx, sy = x % 8, y % 8
                    if word & 0x400:
                        sx = 7 - sx
                    if word & 0x800:
                        sy = 7 - sy
                    pixel = self.tile_pixel(word & 1023, sx, sy, word >> 12, depth=depth)
                    if pixel[3]:
                        at = (y * 240 + x) * 4
                        output[at:at + 4] = pixel
        return bytes(output)

    def oam_sprite(self, entries, width, height, origin):
        out = bytearray(width * height * 4)
        for attr0, attr1, attr2 in reversed(entries):
            w, h = SIZES[attr0 >> 14][attr1 >> 14]
            x, y = (attr1 & 511) - origin[0], (attr0 & 255) - origin[1]
            for dy in range(h):
                for dx in range(w):
                    sx = w - 1 - dx if attr1 & 0x1000 else dx
                    sy = h - 1 - dy if attr1 & 0x2000 else dy
                    tile = (attr2 & 1023) + (sy // 8) * (w // 8) + sx // 8
                    pixel = self.tile_pixel(tile, sx % 8, sy % 8, attr2 >> 12, obj=True)
                    if pixel[3]:
                        if not (0 <= x + dx < width and 0 <= y + dy < height):
                            raise AssertionError("export clips native opaque pixel")
                        at = ((y + dy) * width + x + dx) * 4
                        out[at:at + 4] = pixel
        return bytes(out)

    def descriptor(self, entry):
        source = entry["source"]["native"]
        task = bytearray(36)
        struct.pack_into("<h", task, 0xA, 100)
        struct.pack_into("<h", task, 0xE, 100)
        struct.pack_into("<I", task, 0x10, BASE + source["descriptorOffset"])
        struct.pack_into("<H", task, 0x14, source["tile"])
        task[0x16] = source.get("style",0)
        task[0x17] = source.get("transform",0)
        task[0x18] = source["packedPaletteLayer"]
        task_base,oam_base=(0x4400,0x4e70) if self.game=="A2NE-Bounce" else (0x5320,0x5d90)
        self.cpu.mem_write(IWRAM + task_base, bytes(task))
        self.cpu.mem_write(IWRAM + 0x1088, b"\1\0" + bytes(6))
        self.run(0x1088)
        rows = []
        at = source["descriptorOffset"]
        while True:
            rows.append(struct.unpack("<3H", self.read(IWRAM + oam_base + 8 * len(rows), 6)))
            if not self.source[at + 7]:
                break
            at += 8
        anchor = source["anchorOffset"]
        if source.get("composition",{}).get("requiresNativeAffine",False):
            if len(rows)!=1:raise AssertionError("multi-piece affine subject is unsupported")
            attr0,attr1,attr2=rows[0]
            part=source["composition"]["parts"][0]
            if (attr0>>8,attr1>>8,attr2>>8) != (part["attr0High"],part["attr1High"],part["attr2High"]|(part["tile"]>>8)):
                raise AssertionError("native affine source OAM differs")
            width,height=SIZES[attr0>>14][attr1>>14]
            if [width,height]!=part["textureSize"] or ((attr1>>9)&31)!=part["matrixIndex"]:
                raise AssertionError("native affine source dimensions/matrix index differ")
            return self.rectangle(attr2&1023,attr2>>12,width,height,obj=True)
        return self.oam_sprite(rows, entry["width"], entry["height"],
                               [100 + anchor[0], 100 + anchor[1]])


def main():
    parser = argparse.ArgumentParser()
    for game in IDENTITIES:
        parser.add_argument("--" + game.lower(), type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--unicorn-dir", type=Path)
    args = parser.parse_args()
    if args.unicorn_dir:
        sys.path.insert(0, str(args.unicorn_dir.resolve()))
    from PIL import Image

    catalog = json.loads(args.catalog.read_text())
    native = {}
    for game, expected in IDENTITIES.items():
        source = getattr(args, game.lower()).read_bytes()
        if sha(source) != expected:
            raise ValueError("unsupported complete source: " + game)
        native[game] = Native(game, source)
    native["A2NE-Bounce"] = Native("A2NE-Bounce", args.a2ne.read_bytes())
    for layout in catalog["sourceResources"]:
        if layout["id"] != "final-native-layout":
            continue
        actual = native[layout["game"]].layout()
        for key, value in actual.items():
            if key in layout and layout[key] != value:
                raise AssertionError(f"native loader {layout['game']} {key}: {value} != {layout[key]}")
    for entry in catalog["assets"]:
        path = args.catalog.parent / entry["filename"]
        if sha(path.read_bytes()) != entry["sha256"]:
            raise AssertionError("changed PNG: " + entry["filename"])
        image = Image.open(path).convert("RGBA")
        data = image.tobytes()
        if image.size != (entry["width"], entry["height"]) or sha(data) != entry["rgbaSha256"]:
            raise AssertionError("PNG roundtrip changed native RGBA")
        src = entry["source"]["native"]
        n = native[entry["source"]["game"]]
        name = entry["filename"]
        if name=="bounce-world.png":
            expected=n.background([(0xd000,8)],height=320)
        elif name=="bounce-hud.png":
            expected=n.background([(0xc800,4),(0xc000,4),(0xb800,4)],opaque=False)
        elif name in ("memory-board.png", "rps-background.png", "ball-background.png"):
            layers = {"memory-board.png": [(0xD800, 4), (0xD000, 4), (0xC800, 4)],
                      "rps-background.png": [(0xD800, 4), (0xD000, 4)],
                      "ball-background.png": [(0xD800, 8), (0xD000, 4)]}[name]
            expected = n.background(layers)
        elif "descriptorOffset" in src:
            expected = n.descriptor(entry)
        elif name.startswith("rps-actor-frame-"):
            n.run(0x8190, (0,))
            n.run(0x8204, (0,))
            n.run(0x8284, (src["frameTile"],))
            n.run(0x82FC, (108,))
            n.run(0x8324, (80, 47))
            rows = [struct.unpack("<3H", n.read(OAM + index * 8, 6)) for index in range(5)]
            expected = n.oam_sprite(rows, entry["width"], entry["height"], (80, 39))
        elif "topLeftEntry" in src:
            word = src["topLeftEntry"]
            expected = n.rectangle(word & 1023, word >> 12, 16, 16)
        elif "baseTile" in src:
            template = n.source[src["templateOffset"]:src["templateOffset"] + 16]
            out = bytearray()
            for y in range(32):
                for x in range(32):
                    pixel = n.tile_pixel(src["baseTile"] + template[(y // 8) * 4 + x // 8], x % 8, y % 8)
                    out += pixel[:3] + b"\xff"
            expected = bytes(out)
        else:
            expected = n.rectangle(src["tile"], src["paletteBank"], entry["width"], entry["height"], obj=True)
        if expected != data:
            differences = sum(a != b for a, b in zip(expected, data))
            raise AssertionError(f"native pixels {name}: {differences} byte differences")
    print(json.dumps({"result": "pass", "assets": len(catalog["assets"]),
                      "affineSourceTextures":sum(bool(e["source"]["native"].get("composition",{}).get("requiresNativeAffine")) for e in catalog["assets"]),
                      "catalogSha256": sha(args.catalog.read_bytes()),
                      "nativeLoaderLayouts": {game: n.layout() for game, n in native.items()},
                      "copyLzCalls": {game: len(n.calls) for game, n in native.items()},
                      "scope": "Original Thumb video loaders/OAM producers and independent pixel decoding; explicit BIOS copy/LZ shims, no hardware color/timing/affine claim"}, indent=2))


if __name__ == "__main__":
    main()
