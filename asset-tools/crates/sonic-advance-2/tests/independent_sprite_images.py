#!/usr/bin/env python3
"""Reproduce native sprite parity hashes from three local primary PNG files.

The files are SAT-R/sa2 commit 2dd3f17d graphics/sa2/obj_tiles/4bpp/anim_0000/
f000.png, f001.png and f004.png. Keep these reference images local. This
stdlib-only helper validates their exact Git blobs and emits metadata only.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib


EXPECTED_BLOBS = {
    "f000.png": "d2b9ba9880fe18dec34f7c2c63ca9c19ee4d5a80",
    "f001.png": "96b20b449c3ab6f4b0ac0e95448b116a3bd43e63",
    "f004.png": "b5ceb7a23a9b9094cc068367f2d0024b64decd8d",
}


def paeth(left, up, upper_left):
    prediction = left + up - upper_left
    distances = (abs(prediction - left), abs(prediction - up), abs(prediction - upper_left))
    return (left, up, upper_left)[distances.index(min(distances))]


def decode_indexed_png(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("invalid PNG signature")
    position = 8
    palette = None
    alpha = b""
    compressed = bytearray()
    header = None
    ended = False
    while position < len(data):
        if position + 12 > len(data):
            raise ValueError("truncated PNG chunk")
        length = int.from_bytes(data[position:position + 4], "big")
        kind = data[position + 4:position + 8]
        end = position + 8 + length
        if end + 4 > len(data):
            raise ValueError("PNG chunk exceeds input")
        chunk = data[position + 8:end]
        crc = int.from_bytes(data[end:end + 4], "big")
        if zlib.crc32(kind + chunk) & 0xFFFFFFFF != crc:
            raise ValueError("PNG chunk CRC mismatch")
        if kind == b"IHDR":
            if len(chunk) != 13 or header is not None:
                raise ValueError("invalid/repeated PNG header")
            header = struct.unpack(">IIBBBBB", chunk)
        elif kind == b"PLTE":
            palette = chunk
        elif kind == b"tRNS":
            alpha = chunk
        elif kind == b"IDAT":
            compressed.extend(chunk)
        elif kind == b"IEND":
            ended = True
            if end + 4 != len(data):
                raise ValueError("trailing bytes after PNG IEND")
            break
        position = end + 4
    if not ended or header is None or palette is None or len(palette) % 3:
        raise ValueError("incomplete PNG")
    width, height, depth, color, compression, filtering, interlace = header
    if not (0 < width <= 1024 and 0 < height <= 1024):
        raise ValueError("PNG dimensions exceed source-proof bounds")
    if (depth, color, compression, filtering, interlace) != (4, 3, 0, 0, 0):
        raise ValueError("this proof requires non-interlaced four-bit indexed PNG")
    row_bytes = (width + 1) // 2
    expected = (row_bytes + 1) * height
    inflater = zlib.decompressobj()
    scanlines = inflater.decompress(compressed, expected + 1)
    if len(scanlines) != expected or not inflater.eof or inflater.unconsumed_tail:
        raise ValueError("PNG decompressed size mismatch")
    previous = bytearray(row_bytes)
    rgba = bytearray()
    for y in range(height):
        start = y * (row_bytes + 1)
        filter_id = scanlines[start]
        row = bytearray(scanlines[start + 1:start + 1 + row_bytes])
        for x in range(row_bytes):
            left = row[x - 1] if x else 0
            up = previous[x]
            upper_left = previous[x - 1] if x else 0
            predictors = (0, left, up, (left + up) // 2, paeth(left, up, upper_left))
            if filter_id > 4:
                raise ValueError("unknown PNG scanline filter")
            row[x] = (row[x] + predictors[filter_id]) & 255
        for x in range(width):
            index = (row[x // 2] >> (4 if x % 2 == 0 else 0)) & 15
            if index * 3 + 3 > len(palette):
                raise ValueError("PNG palette index outside table")
            opacity = alpha[index] if index < len(alpha) else 255
            if opacity == 0:
                rgba.extend(bytes(4))
                continue
            if opacity != 255:
                raise ValueError("unexpected partial alpha in native sprite proof")
            # The source PNG export floors five-bit v*255/31. Restore the
            # original channel, then use the Rust converter's bit replication.
            for channel in palette[index * 3:index * 3 + 3]:
                value5 = (channel * 31 + 127) // 255
                if channel != value5 * 255 // 31:
                    raise ValueError("source PNG palette is not native five-bit floor expansion")
                rgba.append((value5 << 3) | (value5 >> 2))
            rgba.append(opacity)
        previous = row
    return width, height, rgba


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    reports = []
    for name, expected_blob in EXPECTED_BLOBS.items():
        path = args.reference_dir / name
        if path.stat().st_size > 1_048_576:
            raise ValueError("reference PNG exceeds one MiB")
        data = path.read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()
        if blob != expected_blob:
            raise ValueError(f"primary PNG Git blob mismatch: {name}")
        width, height, rgba = decode_indexed_png(data)
        reports.append({"frame": int(name[1:4]), "width": width, "height": height,
                        "pngGitBlobSha1": blob, "rgbaSha256": hashlib.sha256(rgba).hexdigest()})
    text = json.dumps({"referenceCommit": "2dd3f17d30cd3c156e508aac0017620ae485b9ba",
                       "paletteExpansion": "restore five-bit channels; bit replication; invisible RGB zero",
                       "frames": reports}, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    print(text, end="")


if __name__ == "__main__":
    main()
