"""Extract the native Chao colour table from a locally decoded GameCube ChaoMain.rel.

    python -I scripts/extract-chao-look.py [path/to/ChaoMain.rel] [--out .local/imports/chao-look.json]

ChaoMain.rel comes from the person's own SA2B (GSNE8P) disc: the PRS-decoded
ChaoMain.prs, kept under ignored .local/ storage (default
.local/research/gsne8p-disc/ChaoMain.rel). Its SHA-1 must match the
roeming/SA2B decomp's GSNE8P config, so the section offsets below are exact.
The output is game-derived and stays under .local/; only this script is source.

Traced (ChaoMain .text offsets):
* AL_MaterialInit (0x17314) calls AL_MatSetChaoColor (fn_60_17BF4) only when
  ColorNum != 0. The setter stores table[ColorNum] (.rodata 0x1D60, u32) over the
  first material diffuse word of each node in the s16 list at .data 0x6E18
  (terminated by -1); nodes without an attach are skipped. It replaces the word.
* The word is a big-endian Ninja colour, 0xAARRGGBB.
"""
import hashlib
import json
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_SHA1 = 'e7e24b8b6f4305d95531c9d898d566ead79ddc8d'
RODATA, DATA = 4, 5
COLOR_TABLE = 0x1D60
COLOR_COUNT = 28  # .rodata object size 0x70
COLOR_NODES = 0x6E18
MAX_NODES = 40


def sections(rel):
    if len(rel) < 0x40:
        raise ValueError('file too small for a REL header')
    count, table = struct.unpack_from('>II', rel, 0x0C)
    if count > 64 or table + count * 8 > len(rel):
        raise ValueError('REL section table out of range')
    result = []
    for index in range(count):
        offset, size = struct.unpack_from('>II', rel, table + index * 8)
        result.append((offset & ~3, size))
    return result


def section_bytes(rel, table, index):
    if index >= len(table):
        raise ValueError(f'REL has no section {index}')
    offset, size = table[index]
    if offset == 0 or offset + size > len(rel):
        raise ValueError(f'REL section {index} is not stored in the file')
    return rel[offset:offset + size], offset


def main(argv):
    out = ROOT / '.local/imports/chao-look.json'
    if '--out' in argv:
        position = argv.index('--out')
        out = Path(argv[position + 1])
        argv = argv[:position] + argv[position + 2:]
    source = Path(argv[0]) if argv else ROOT / '.local/research/gsne8p-disc/ChaoMain.rel'
    rel = source.read_bytes()
    sha1 = hashlib.sha1(rel).hexdigest()
    if sha1 != EXPECTED_SHA1:
        raise SystemExit(f'{source}: SHA-1 {sha1} is not the GSNE8P ChaoMain.rel ({EXPECTED_SHA1})')
    table = sections(rel)
    rodata, rodata_offset = section_bytes(rel, table, RODATA)
    data, data_offset = section_bytes(rel, table, DATA)
    if COLOR_TABLE + COLOR_COUNT * 4 > len(rodata) or COLOR_NODES + MAX_NODES * 2 > len(data):
        raise SystemExit('colour tables are outside the REL sections')
    colors = []
    for index in range(COLOR_COUNT):
        word = struct.unpack_from('>I', rodata, COLOR_TABLE + index * 4)[0]
        colors.append([(word >> 16) & 0xFF, (word >> 8) & 0xFF, word & 0xFF, word >> 24])
    nodes = []
    for index in range(MAX_NODES):
        node = struct.unpack_from('>h', data, COLOR_NODES + index * 2)[0]
        if node == -1:
            break
        nodes.append(node)
    look = {
        'source': {'file': source.name, 'sha1': sha1, 'offset': rodata_offset + COLOR_TABLE,
                   'colorTable': {'section': '.rodata', 'offset': COLOR_TABLE},
                   'colorNodes': {'section': '.data', 'offset': COLOR_NODES, 'fileOffset': data_offset + COLOR_NODES}},
        'colorTable': colors,
        'colorTableOrder': 'index = ColorNum; [r, g, b, a] from the big-endian 0xAARRGGBB diffuse word',
        'colorNodes': nodes,
        'colorApplication': 'only when ColorNum != 0; replaces the first material diffuse word of each node',
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(look, indent=2) + '\n')
    print(json.dumps({'out': str(out), 'colors': len(colors), 'nodes': nodes}))


if __name__ == '__main__':
    main(sys.argv[1:])
