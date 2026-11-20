#!/usr/bin/env python3
"""Independent, standard-library A3VE manifest oracle; emits metadata only.

This script does not import the Rust implementation or a third-party decoder.
ROM, decoded programs, saves and images are never written. Report files must
live below an existing ignored .local directory, outside a tracked checkout.
"""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct
import sys
import unittest


ROM_LENGTH = 8 * 1024 * 1024
ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
ROM_SHA1 = "92b5c19cb4dee8fdfc15a8abdc699822d5373042"
GARDEN_OFFSET = 0xD0970
GARDEN_STORED = 152552
GARDEN_LENGTH = 188716
GARDEN_SHA256 = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
MAX_DECODED = 256 * 1024
MAX_CATALOG = 16 * 1024 * 1024


class ParityError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise ParityError(message)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def integer(value, field):
    require(isinstance(value, int) and not isinstance(value, bool), f"{field}: integer required")
    require(value >= 0, f"{field}: negative integer")
    return value


def span(data, offset, length):
    integer(offset, "offset")
    integer(length, "length")
    require(offset <= len(data) and length <= len(data) - offset, "span outside source")
    return data[offset:offset + length]


def decode(data, maximum=MAX_DECODED):
    """Strict BIOS 0x10/0x30 decoding; returns (output, exact bytes consumed).

    The 24-bit declared size is checked before allocation. Each token is
    checked against both the input end and the declared output end. LZ match
    bytes are appended individually to support overlapping references.
    """
    require(len(data) >= 4, "truncated compression header")
    tag = data[0]
    require(tag in (0x10, 0x30), "unsupported compression tag")
    size = int.from_bytes(data[1:4], "little")
    require(size <= maximum, "decoded output exceeds bound")
    output = bytearray()
    cursor = 4

    def take(count):
        nonlocal cursor
        require(count <= len(data) - cursor, "truncated compression token")
        result = data[cursor:cursor + count]
        cursor += count
        return result

    if tag == 0x10:
        while len(output) < size:
            flags = take(1)[0]
            for shift in range(7, -1, -1):
                if len(output) == size:
                    break
                if flags & (1 << shift):
                    high, low = take(2)
                    count = (high >> 4) + 3
                    distance = ((high & 15) << 8 | low) + 1
                    require(distance <= len(output), "back reference precedes output")
                    require(count <= size - len(output), "match crosses output end")
                    for _ in range(count):
                        output.append(output[-distance])
                else:
                    output.extend(take(1))
    else:
        while len(output) < size:
            command = take(1)[0]
            count = (command & 127) + (3 if command & 128 else 1)
            require(count <= size - len(output), "RLE token crosses output end")
            if command & 128:
                output.extend(take(1) * count)
            else:
                output.extend(take(count))
    return bytes(output), cursor


def bounded_read(path, maximum):
    require(path.stat().st_size <= maximum, "input file exceeds size bound")
    with path.open("rb") as handle:
        data = handle.read(maximum + 1)
    require(len(data) <= maximum, "input file grew beyond size bound")
    return data


def validate_rom(rom):
    require(len(rom) == ROM_LENGTH, "A3VE ROM length mismatch")
    require(rom[0xA0:0xAC] == b"SONICPINBALL", "A3VE title mismatch")
    require(rom[0xAC:0xB0] == b"A3VE", "A3VE game code mismatch")
    require(rom[0xB2] == 0x96 and rom[0xBC] == 0, "A3VE fixed byte/revision mismatch")
    complement = (-sum(rom[0xA0:0xBD]) - 0x19) & 255
    require(rom[0xBD] == complement, "A3VE header complement mismatch")
    require(sha256(rom) == ROM_SHA256, "A3VE ROM SHA-256 mismatch")
    require(hashlib.sha1(rom).hexdigest() == ROM_SHA1, "A3VE ROM SHA-1 mismatch")


def crc_table_words():
    result = []
    for value in range(256):
        word = value
        for _ in range(8):
            word = (word >> 1) ^ (0x8408 if word & 1 else 0)
        result.append(word)
    return result


def validate_crc_tables(rom, garden):
    expected = crc_table_words()
    for space, data, offset in [("rom", rom, 0xF5D58), ("garden", garden, 0x1F4E0)]:
        actual = list(struct.unpack("<256H", span(data, offset, 512)))
        require(actual == expected, f"{space}: reflected 0x8408 checksum table mismatch")
    return {"tables": 2, "wordsPerTable": 256, "polynomial": "0x8408"}


def graphics_hashes(garden):
    """Independent byte-level tile-index and palette-conversion fingerprints."""
    result = {}
    for offset, label in [(0x1614C, "minigameObj4bpp"), (0x17214, "minigameBg4bpp")]:
        decoded, _ = decode(garden[offset:])
        require(len(decoded) % 32 == 0, "4bpp stream is not whole 8x8 tiles")
        indices = bytes(nibble for byte in decoded for nibble in (byte & 15, byte >> 4))
        result[label] = {
            "space": "garden", "offset": offset, "tileCount": len(decoded) // 32,
            "decodedIndexLength": len(indices), "sha256": sha256(indices),
        }
    for offset, length, label in [
        (0x1606C, 0xE0, "minigameObjPalette"),
        (0x17074, 0x1A0, "minigameBgPalette"),
        (0x22B68, 0x200, "gardenPalette22b68"),
        (0x23E50, 0x200, "gardenPalette23e50"),
    ]:
        values = struct.unpack(f"<{length // 2}H", span(garden, offset, length))
        rgba = bytearray()
        for value in values:
            channels = [(value >> shift) & 31 for shift in (0, 5, 10)]
            rgba.extend((channel << 3) | (channel >> 2) for channel in channels)
            rgba.append(255)
        result[label] = {
            "space": "garden", "offset": offset, "storedLength": length,
            "rgbaLength": len(rgba), "sha256": sha256(rgba),
        }
    return result


def rom_pointer(rom, position, target_length):
    address = int.from_bytes(span(rom, position, 4), "little")
    require(address >= 0x08000000, "pointer below ROM address space")
    offset = address - 0x08000000
    span(rom, offset, target_length)
    return offset


def compact_hash(value):
    return sha256(json.dumps(value, separators=(",", ":")).encode("ascii"))


def audio_metadata(rom):
    """Fingerprint bounded header/track starts; never execute sequence opcodes.

    Canonical rows are [id, header_offset, track_count, voicegroup_or_null,
    [track_offsets]], serialized as compact ASCII JSON without a trailing line.
    """
    table_offset = 0xF9F74
    span(rom, table_offset, 200 * 8)
    rows = []
    for song_id in range(200):
        header_offset = rom_pointer(rom, table_offset + song_id * 8, 8)
        require(header_offset % 4 == 0, "unaligned song header")
        header = span(rom, header_offset, 8)
        track_count = header[0]
        require(track_count <= 32, "song track count exceeds bound")
        require(header[1] == 0, "block-based song header unsupported")
        span(rom, header_offset, 8 + 4 * track_count)
        if track_count:
            voicegroup = rom_pointer(rom, header_offset + 4, 12)
            require(voicegroup % 4 == 0, "unaligned voicegroup")
        else:
            require(not any(header), "nonzero empty song header")
            voicegroup = None
        tracks = [rom_pointer(rom, header_offset + 8 + 4 * index, 1) for index in range(track_count)]
        rows.append([song_id, header_offset, track_count, voicegroup, tracks])
    return {
        "entries": len(rows), "uniqueHeaders": len({row[1] for row in rows}),
        "nonemptyEntries": sum(row[2] != 0 for row in rows),
        "trackPointers": sum(row[2] for row in rows),
        "canonicalRowsSha256": compact_hash(rows),
    }


def table_metadata(rom):
    """Read the A3VE descriptor tree without assigning pinball-stage names.

    Canonical descriptor rows are [id,[columns],[primary_tiles],
    [primary_palettes],[inferred_allocation_lengths]]. The supplemental hash
    uses [[seven_lz_offsets],[fourteen_palette_offsets]].
    """
    descriptor_offset = 0xAC78C
    span(rom, descriptor_offset, 23 * 28)
    rows = []
    for table_id in range(23):
        columns = [rom_pointer(rom, descriptor_offset + table_id * 28 + column * 4, 1) for column in range(7)]
        if table_id < 8:
            tiles = [rom_pointer(rom, columns[0] + variant * 4, 1) for variant in range(4)]
            palettes = [rom_pointer(rom, columns[2] + variant * 4, 512) for variant in range(4)]
            ends = palettes
        else:
            tiles = [columns[0]]
            palettes = [columns[2]]
            span(rom, palettes[0], 512)
            ends = [columns[1]]
        span(rom, columns[4], 512)
        lengths = []
        for start, end in zip(tiles, ends):
            length = end - start
            require(0 < length <= 128 * 1024 and length % 32 == 0, "invalid inferred tile allocation")
            span(rom, start, length)
            lengths.append(length)
        rows.append([table_id, columns, tiles, palettes, lengths])
    supplemental_lz = [rom_pointer(rom, 0xACA10 + index * 4, 4) for index in range(7)]
    for offset in supplemental_lz:
        require(rom[offset] == 0x10, "supplemental stream is not LZ77")
        decode(rom[offset:])
    supplemental_palettes = [rom_pointer(rom, 0xACA2C + index * 4, 512) for index in range(14)]
    return {
        "descriptors": len(rows), "primaryVariants": sum(len(row[2]) for row in rows),
        "canonicalRowsSha256": compact_hash(rows),
        "supplementalLzPointers": len(supplemental_lz),
        "supplementalPalettePointers": len(supplemental_palettes),
        "supplementalSha256": compact_hash([supplemental_lz, supplemental_palettes]),
    }


def main_board_render(rom):
    """Independent per-pixel 32x64 text-background render; emits only a hash."""
    tiles = span(rom, 0x31C5FC, 0xD000)
    palette = struct.unpack("<256H", span(rom, 0x3295FC, 512))
    tilemap, consumed = decode(rom[0x3345FC:])
    require(len(tilemap) == 4096, "main board tilemap length mismatch")
    width, height = 256, 512
    rgba = bytearray(width * height * 4)
    max_tile = 0
    for y in range(height):
        for x in range(width):
            tile_x, tile_y = x // 8, y // 8
            # 32-wide map: two vertical 32x32 screen blocks.
            cell = tile_y // 32 * 1024 + tile_y % 32 * 32 + tile_x
            entry = int.from_bytes(tilemap[cell * 2:cell * 2 + 2], "little")
            tile = entry & 1023
            max_tile = max(max_tile, tile)
            pixel_x = 7 - x % 8 if entry & 0x400 else x % 8
            pixel_y = 7 - y % 8 if entry & 0x800 else y % 8
            packed = span(tiles, tile * 32 + pixel_y * 4 + pixel_x // 2, 1)[0]
            index = (packed >> (4 * (pixel_x % 2))) & 15
            if index == 0:
                continue
            value = palette[(entry >> 12) * 16 + index]
            color = [(((value >> shift) & 31) << 3) | (((value >> shift) & 31) >> 2) for shift in (0, 5, 10)]
            destination = (y * width + x) * 4
            rgba[destination:destination + 4] = bytes(color + [255])
    fingerprint = sha256(rgba)
    require(fingerprint == "947f0e412a386426ab6e363c3c3c225e9e18534c201bcd1a62d60bbd1515c09c", "independent Sonic board render mismatch")
    return {
        "width": width, "height": height, "mapConsumed": consumed,
        "mapDecodedLength": len(tilemap), "highestTileIndex": max_tile,
        "rgbaSha256": fingerprint,
    }


def validate_catalog(rom, garden, catalog):
    require(isinstance(catalog, dict), "catalog must be an object")
    require(catalog.get("schemaVersion") == 1, "unsupported catalog schema")
    source = catalog.get("source", {})
    require(isinstance(source, dict), "catalog source must be an object")
    require(source.get("gameCode") == "A3VE" and source.get("revision") == 0, "catalog identity mismatch")
    require(source.get("length") == ROM_LENGTH, "catalog source length mismatch")
    require(source.get("sha256") == ROM_SHA256 and source.get("sha1") == ROM_SHA1, "catalog source hash mismatch")
    assets = catalog.get("assets")
    require(isinstance(assets, list) and len(assets) <= 16384, "invalid asset array")
    counts = Counter()
    categories = Counter()
    pointers = 0
    decode_budget = 512 * 1024 * 1024
    identifiers = set()
    for asset in assets:
        require(isinstance(asset, dict), "invalid asset record")
        identity = asset.get("id")
        require(isinstance(identity, str) and identity not in identifiers, "missing/duplicate asset id")
        identifiers.add(identity)
        space = asset.get("space")
        require(space in ("rom", "garden"), f"{identity}: invalid address space")
        data = rom if space == "rom" else garden
        offset = integer(asset.get("offset"), "offset")
        stored_length = integer(asset.get("storedLength"), "storedLength")
        require(stored_length > 0, f"{identity}: empty source span")
        stored = span(data, offset, stored_length)
        require(sha256(stored) == asset.get("storedSha256"), f"{identity}: stored SHA-256 mismatch")
        encoding = asset.get("encoding")
        declared_length = integer(asset.get("decodedLength"), "decodedLength")
        require(declared_length <= decode_budget, "aggregate decoding budget exceeded")
        decode_budget -= declared_length
        if encoding == "raw":
            decoded = stored
        else:
            tags = {"gba-lz77-10": 0x10, "gba-rle-30": 0x30}
            require(encoding in tags and stored[0] == tags[encoding], f"{identity}: encoding mismatch")
            decoded, consumed = decode(stored)
            require(consumed == stored_length, f"{identity}: compressed span includes padding or trailing bytes")
        require(len(decoded) == asset.get("decodedLength"), f"{identity}: decoded length mismatch")
        require(sha256(decoded) == asset.get("decodedSha256"), f"{identity}: decoded SHA-256 mismatch")
        refs = asset.get("pointerReferences")
        require(isinstance(refs, list) and len(refs) <= len(data) // 4, f"{identity}: invalid references")
        require(len(refs) == len(set(refs)), f"{identity}: duplicate references")
        base = 0x08000000 if space == "rom" else 0x02000000
        for reference in refs:
            integer(reference, "pointer reference")
            require(reference % 4 == 0, f"{identity}: unaligned reference")
            word = int.from_bytes(span(data, reference, 4), "little")
            require(word == base + offset, f"{identity}: pointer target mismatch")
        pointers += len(refs)
        counts[(space, encoding)] += 1
        categories[asset.get("category", "unknown")] += 1
    return {
        "assets": len(assets), "pointerReferences": pointers,
        "bySpaceAndEncoding": {f"{space}/{encoding}": count for (space, encoding), count in sorted(counts.items())},
        "byCategory": dict(sorted(categories.items())),
    }


def check_report_destination(destination, rom_path, catalog_path):
    resolved = destination.resolve()
    require(resolved.suffix.lower() == ".json", "report extension must be .json")
    require(resolved not in (rom_path.resolve(), catalog_path.resolve()), "report would overwrite an input")
    require(resolved.parent.is_dir(), "report parent directory must already exist")
    for parent in (resolved.parent, *resolved.parent.parents):
        if parent.name == ".local" and parent.is_dir():
            return resolved
        require(not (parent / ".git").exists(), "report must be in an ignored .local folder, outside tracked checkout paths")
    raise ParityError("report must be below an existing .local directory")


class OracleDecoderTests(unittest.TestCase):
    """Run with unittest discovery; fixtures are entirely synthetic."""

    def test_literal_lz_consumes_exact_span(self):
        decoded, consumed = decode(bytes([0x10, 3, 0, 0, 0, 7, 8, 9, 0xFF]))
        self.assertEqual((decoded, consumed), (bytes([7, 8, 9]), 8))

    def test_overlapping_lz_reference(self):
        self.assertEqual(decode(bytes([0x10, 6, 0, 0, 0x40, 0x61, 0x20, 0])), (b"aaaaaa", 8))

    def test_rle_literal_and_run(self):
        self.assertEqual(decode(bytes([0x30, 5, 0, 0, 1, 7, 8, 0x80, 9])), (bytes([7, 8, 9, 9, 9]), 9))

    def test_truncated_headers_and_tokens(self):
        for data in [b"", bytes([0x10, 0]), bytes([0x10, 1, 0, 0]),
                     bytes([0x10, 1, 0, 0, 0]), bytes([0x30, 1, 0, 0, 0]),
                     bytes([0x30, 3, 0, 0, 0x80])]:
            with self.subTest(data=data), self.assertRaises(ParityError):
                decode(data)

    def test_invalid_reference_and_token_overrun(self):
        for data in [bytes([0x10, 3, 0, 0, 0x80, 0, 0]),
                     bytes([0x10, 2, 0, 0, 0x40, 7, 0, 0]),
                     bytes([0x30, 1, 0, 0, 0x80, 7]),
                     bytes([0x30, 1, 0, 0, 1, 7, 8])]:
            with self.subTest(data=data), self.assertRaises(ParityError):
                decode(data)

    def test_output_bound_and_unknown_type(self):
        for data, maximum in [(bytes([0x10, 3, 0, 0]), 2), (bytes([0x20, 0, 0, 0]), 1)]:
            with self.subTest(data=data), self.assertRaises(ParityError):
                decode(data, maximum)

    def test_span_rejects_negative_and_outside_ranges(self):
        for offset, length in [(-1, 1), (1, -1), (4, 1), (2, 3), (True, 1)]:
            with self.subTest(offset=offset, length=length), self.assertRaises(ParityError):
                span(bytes([0, 1, 2]), offset, length)

    def test_checksum_table_reference_entries(self):
        table = crc_table_words()
        self.assertEqual((len(table), table[0], table[1], table[255]), (256, 0, 0x1189, 0x0F78))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report_path = check_report_destination(args.report, args.rom, args.catalog) if args.report else None
    rom = bounded_read(args.rom, ROM_LENGTH)
    validate_rom(rom)
    stored = span(rom, GARDEN_OFFSET, GARDEN_STORED)
    garden, consumed = decode(stored, GARDEN_LENGTH)
    require(consumed == GARDEN_STORED and len(garden) == GARDEN_LENGTH, "garden compressed/decoded length mismatch")
    require(sha256(garden) == GARDEN_SHA256, "garden SHA-256 mismatch")
    catalog = json.loads(bounded_read(args.catalog, MAX_CATALOG))
    report = {
        "schemaVersion": 1, "oracle": "python-standard-library-independent",
        "romSha256": ROM_SHA256, "gardenSha256": GARDEN_SHA256,
        "catalog": validate_catalog(rom, garden, catalog),
        "checksumTables": validate_crc_tables(rom, garden),
        "graphics": graphics_hashes(garden),
        "audio": audio_metadata(rom),
        "tables": table_metadata(rom),
        "mainBoard": main_board_render(rom),
    }
    serialized = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if report_path:
        report_path.write_text(serialized, encoding="utf-8")
    print(serialized, end="")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"parity failed: {error}", file=sys.stderr)
        sys.exit(1)
