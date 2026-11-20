#!/usr/bin/env python3
"""Independent stdlib-only source oracle; emits hashes/counts, never asset bytes.

Set A2NE_ROM to the local verified cartridge and run this script. The default
output is catalog/parity.json. No Rust implementation or generated Rust output
is read. The source ranges come from the public metadata catalog.
"""

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import struct


CRATE = Path(__file__).resolve().parents[1]
ROM_LENGTH = 16_777_216
ROM_SHA1 = "7bcd6a07af7c894746fa28073fe0c0e34408022d"
ROM_SHA256 = "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
GARDEN_SHA1 = "2c7da8e1c9af85986eecf4effae39197cdac9abf"
GARDEN_SHA256 = "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca"
GARDEN_OFFSET = 0xAED70
GARDEN_LENGTH = 191_340
GARDEN_COMPRESSED_LENGTH = 155_273


def digest(data, algorithm="sha256"):
    return hashlib.new(algorithm, data).hexdigest()


def lz77(data, limit=4 * 1024 * 1024):
    """BIOS type-10 byte decoder, independently expressed with a flag mask."""
    if len(data) < 4 or data[0] != 0x10:
        raise ValueError("missing type-10 LZ77 header")
    length = int.from_bytes(data[1:4], "little")
    if length > limit:
        raise ValueError("LZ77 output exceeds independent limit")
    output = bytearray()
    cursor = 4
    flags = 0
    mask = 0
    while len(output) < length:
        if mask == 0:
            if cursor >= len(data):
                raise ValueError("truncated LZ77 flags")
            flags = data[cursor]
            cursor += 1
            mask = 128
        if flags & mask:
            if cursor + 2 > len(data):
                raise ValueError("truncated LZ77 back-reference")
            token = int.from_bytes(data[cursor:cursor + 2], "big")
            cursor += 2
            distance = (token & 0xFFF) + 1
            count = (token >> 12) + 3
            if distance > len(output) or len(output) + count > length:
                raise ValueError("invalid LZ77 back-reference")
            for _ in range(count):
                output.append(output[-distance])
        else:
            if cursor >= len(data):
                raise ValueError("truncated LZ77 literal")
            output.append(data[cursor])
            cursor += 1
        mask >>= 1
    return bytes(output), cursor


def decode(raw, kind, source=None, base=0):
    if kind == "opaque":
        return None
    if kind == "lz77":
        result, _consumed = lz77(raw)
        units = len(result)
        output_kind = "lz77-bytes"
    elif kind == "palette-bgr555":
        if len(raw) % 2 or len(raw) > 512:
            raise ValueError("invalid palette size")
        result = bytearray()
        for (word,) in struct.iter_unpack("<H", raw):
            for shift in (0, 5, 10):
                channel = (word >> shift) & 31
                result.append(channel * 8 + channel // 4)
            result.append(255)
        result = bytes(result)
        units = len(raw) // 2
        output_kind = "rgba8-opaque"
    elif kind in ("tiles-4bpp", "tiles-8bpp"):
        stride = 32 if kind == "tiles-4bpp" else 64
        if len(raw) % stride or len(raw) // stride > 16_384:
            raise ValueError("invalid tile-bank size")
        if stride == 32:
            result = bytes(nibble for byte in raw for nibble in (byte % 16, byte // 16))
        else:
            result = raw
        units = len(raw) // stride
        output_kind = "tile-major-indices8"
    elif kind == "motion-8":
        if len(raw) % 8 or len(raw) // 8 > 4096:
            raise ValueError("invalid motion-table size")
        rows = [
            {"firstWord": int.from_bytes(raw[i:i + 2], "little"), "fields": list(raw[i + 2:i + 8])}
            for i in range(0, len(raw), 8)
        ]
        result = json.dumps(rows, separators=(",", ":")).encode("ascii")
        units = len(rows)
        output_kind = "motion-fields-json"
    elif kind == "text-map":
        if len(raw) % 2:
            raise ValueError("invalid text-map word length")
        rows = [
            {"raw": word, "tileIndex": word & 1023, "horizontalFlip": bool(word & 1024),
             "verticalFlip": bool(word & 2048), "paletteBank": word >> 12}
            for (word,) in struct.iter_unpack("<H", raw)
        ]
        result = json.dumps(rows, separators=(",", ":")).encode("ascii")
        units = len(rows)
        output_kind = "text-map-words-json"
    elif kind == "pointer-table":
        if source is None or len(raw) % 4 or len(raw) // 4 > 16_384:
            raise ValueError("invalid pointer-table source or row length")
        rows = []
        for (pointer,) in struct.iter_unpack("<I", raw):
            offset = pointer - base if pointer else None
            if offset is not None and not 0 <= offset < len(source):
                raise ValueError("pointer-table entry outside source")
            rows.append({"pointer": pointer, "offset": offset})
        result = json.dumps(rows, separators=(",", ":")).encode("ascii")
        units = len(rows)
        output_kind = "pointer-offsets-json"
    elif kind == "m4a-song-table":
        if source is None or len(raw) % 8:
            raise ValueError("invalid song-table source or row length")
        rows = []

        def pointer_offset(pointer):
            offset = pointer - base
            if offset < 0 or offset >= len(source):
                raise ValueError("song-table pointer outside source")
            return offset

        for pointer, tail1, tail2 in struct.iter_unpack("<IHH", raw):
            offset = pointer_offset(pointer)
            if offset + 4 > len(source):
                raise ValueError("truncated song control bytes")
            header = None
            if source[offset:offset + 4] != bytes(4):
                if offset + 8 > len(source):
                    raise ValueError("truncated song header")
                tracks, blocks, priority, reverb, voicegroup = struct.unpack_from("<BBBBI", source, offset)
                if not 1 <= tracks <= 16:
                    raise ValueError("invalid song track count")
                pointer_offset(voicegroup)
                if offset + 8 + tracks * 4 > len(source):
                    raise ValueError("truncated song tracks")
                track_pointers = list(struct.unpack_from(f"<{tracks}I", source, offset + 8))
                for track in track_pointers:
                    pointer_offset(track)
                header = {"trackCount": tracks, "blockCount": blocks, "priority": priority,
                          "reverb": reverb, "voicegroupPointer": voicegroup,
                          "trackPointers": track_pointers}
            rows.append({"pointer": pointer, "opaqueTailWords": [tail1, tail2], "header": header})
        result = json.dumps(rows, separators=(",", ":")).encode("ascii")
        units = len(rows)
        output_kind = "m4a-song-table-json"
    else:
        raise ValueError(f"unsupported format, no silent parity skip: {kind}")
    return {"format": output_kind, "bytes": len(result), "units": units, "sha256": digest(result)}


def resource_ranges():
    resources = [
        {"id": "rom.header", "source": "rom", "offset": 0, "length": 0xC0, "format": "opaque"},
        {"id": "rom.tiny-garden.lz77", "source": "rom", "offset": GARDEN_OFFSET,
         "length": GARDEN_COMPRESSED_LENGTH, "format": "lz77"},
    ]
    with (CRATE / "catalog/garden-ranges.tsv").open(encoding="utf-8", newline="") as stream:
        for row in csv.DictReader(stream, delimiter="\t"):
            resources.append({"id": f"garden.range.{row['id']}", "source": row["source"],
                              "offset": int(row["offset"], 16), "length": int(row["length"], 16),
                              "format": "opaque"})
    with (CRATE / "catalog/resources.json").open(encoding="utf-8") as stream:
        resources.extend(json.load(stream))
    return resources


def rendered_maps(resources, sources):
    """Direct packed-nibble pixel loop, independent of the Rust tile reader."""
    by_id = {item["id"]: item for item in resources}

    def raw(identifier):
        item = by_id[identifier]
        start = item["offset"]
        end = start + item["length"]
        source = sources[item["source"]]
        if not 0 <= start < end <= len(source):
            raise ValueError(f"invalid render range: {identifier}")
        return source[start:end]

    reports = []
    for group, columns, rows in (
        ("rom.bg.title", 32, 64),
        ("rom.bg.zone-1-act-1", 32, 30),
        ("rom.ui.enter-name", 30, 20),
    ):
        tiles = raw(group + ".tiles")
        palette = raw(group + ".palette")
        tilemap = raw(group + ".map")
        if len(tilemap) != columns * rows * 2 or len(tiles) % 32 or len(palette) % 2:
            raise ValueError(f"invalid render geometry: {group}")
        width, height = columns * 8, rows * 8
        pixels = bytearray(width * height * 4)
        for y in range(height):
            for x in range(width):
                entry_offset = ((y // 8) * columns + x // 8) * 2
                word = int.from_bytes(tilemap[entry_offset:entry_offset + 2], "little")
                source_x = (x % 8) ^ (7 if word & 0x400 else 0)
                source_y = (y % 8) ^ (7 if word & 0x800 else 0)
                nibble = source_y * 8 + source_x
                packed_offset = (word & 1023) * 32 + nibble // 2
                if packed_offset >= len(tiles):
                    raise ValueError(f"render tile index outside bank: {group}")
                index = (tiles[packed_offset] >> ((nibble % 2) * 4)) & 15
                palette_offset = ((word >> 12) * 16 + index) * 2
                if palette_offset + 2 > len(palette):
                    raise ValueError(f"render color outside palette: {group}")
                color = int.from_bytes(palette[palette_offset:palette_offset + 2], "little")
                pixel_offset = (y * width + x) * 4
                for channel, shift in enumerate((0, 5, 10)):
                    value = (color >> shift) & 31
                    pixels[pixel_offset + channel] = value * 8 + value // 4
                pixels[pixel_offset + 3] = 0 if index == 0 else 255
        blob_header = b"blob " + str(len(tilemap)).encode("ascii") + b"\0"
        reports.append({"groupId": group, "widthPixels": width, "heightPixels": height,
                        "rgbaSha256": digest(pixels),
                        "mapGitBlobSha1": digest(blob_header + tilemap, "sha1")})
    return reports


def oracle(path):
    if path.stat().st_size != ROM_LENGTH:
        raise ValueError("A2NE ROM must be exactly 16 MiB")
    rom = path.read_bytes()
    if digest(rom, "sha1") != ROM_SHA1 or digest(rom) != ROM_SHA256:
        raise ValueError("A2NE ROM fingerprint mismatch")
    if rom[0xAC:0xB0] != b"A2NE" or rom[0xBC] != 0 or rom[0xB2] != 0x96:
        raise ValueError("A2NE ROM header mismatch")
    if (-sum(rom[0xA0:0xBD]) - 0x19) & 255 != rom[0xBD]:
        raise ValueError("A2NE ROM header complement mismatch")
    garden, consumed = lz77(rom[GARDEN_OFFSET:], 0x40000)
    if len(garden) != GARDEN_LENGTH or consumed != GARDEN_COMPRESSED_LENGTH:
        raise ValueError("garden stream length mismatch")
    if digest(garden, "sha1") != GARDEN_SHA1 or digest(garden) != GARDEN_SHA256:
        raise ValueError("garden fingerprint mismatch")
    sources = {"rom": rom, "garden": garden}
    reports = []
    seen = set()
    resources = resource_ranges()
    for item in resources:
        if item["id"] in seen:
            raise ValueError(f"duplicate resource: {item['id']}")
        seen.add(item["id"])
        source = sources[item["source"]]
        offset, length = item["offset"], item["length"]
        if not isinstance(offset, int) or not isinstance(length, int):
            raise ValueError("non-integer resource range")
        if offset < 0 or length <= 0 or offset + length > len(source):
            raise ValueError(f"out-of-bounds resource: {item['id']}")
        raw = source[offset:offset + length]
        report = {key: item[key] for key in ("id", "source", "offset", "length", "format")}
        report["sha256"] = digest(raw)
        base = 0x08000000 if item["source"] == "rom" else 0x02000000
        try:
            decoded = decode(raw, item["format"], source, base)
        except ValueError as error:
            raise ValueError(f"{item['id']} ({item['source']} {offset:#x}+{length:#x}, {item['format']}): {error}") from error
        if decoded is not None:
            report["decoded"] = decoded
        reports.append(report)
    return {
        "schemaVersion": 1,
        "oracle": "independent_parity.py/1; Python stdlib, no Rust output inputs",
        "romSha1": ROM_SHA1,
        "romSha256": ROM_SHA256,
        "gardenSha1": GARDEN_SHA1,
        "gardenSha256": GARDEN_SHA256,
        "gardenCompressedBytes": consumed,
        "resources": reports,
        "renders": rendered_maps(resources, sources),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=CRATE / "catalog/parity.json")
    args = parser.parse_args()
    source = os.environ.get("A2NE_ROM")
    if not source:
        parser.error("set A2NE_ROM to the verified local raw .gba cartridge")
    result = oracle(Path(source))
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Verified {len(result['resources'])} resource ranges; wrote metadata-only oracle to {args.output}")


if __name__ == "__main__":
    main()
