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
* fn_60_52480(Jewel, Color, Multi, NonTex, obj), called from al_draw_B for every body
  node, sets the draw override. When JewelNum == 0 and ColorNum > 0, the lit material
  colour becomes gxColorTable[ColorNum - 1] (.data 0x38E7D0, GX RGBA order) instead of
  the chunk diffuse (GXSetChanMatColor at 0x5110C; fn_60_50DDC is skipped), so for a
  coloured Chao this table, not colorTable, is the visible colour.
* JewelNum > 0 binds al_body texture JewelNum + 17 to TEXMAP1 with a white material
  colour (.rodata 0x993C); MultiNum != 0 binds al_body texture 34 (chao_rare00) to TEXMAP2.
* al_draw_B never calls fn_60_52480 for the eyes (18, 21), the mouth (27) or nodes 37/39.
* Palettes: ALPaletteInit loads the 39 .GVP files named at .data 0x43CE8 into gPalette
  slots in that order (48 entries each, held as RGBA8, 192 bytes). AL_PaletteSetColorRatio
  (0x1F1FC) picks a slot base per type through the jumptable at .data 0x43DD0: child (2) 0,
  hero normal (6) 9, hero swim (9) 15, hero fly (12) 21, hero run (15) 27, hero power (18)
  33; every other type keeps its palette unblended. A GVP is 'GVPL', a 16-byte header with
  format 1 (RGB565) and 48 big-endian u16 entries.

    python -I scripts/extract-chao-look.py [ChaoMain.rel] [--out FILE] [--textures DIR] [--game DIR]

--textures is the ArchiveTool al_body folder (default .local/extracted/textures/al_body)
and --game the SA2 install (default from .local/game-inventory.json); both are read only.
"""
import hashlib
import json
import shutil
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_SHA1 = 'e7e24b8b6f4305d95531c9d898d566ead79ddc8d'
RODATA, DATA = 4, 5
COLOR_TABLE = 0x1D60
COLOR_COUNT = 28  # .rodata object size 0x70
COLOR_NODES = 0x6E18
GX_COLOR_TABLE = 0x38E7D0
GX_COLOR_COUNT = 13  # ColorNum 1..13; the .data object (0x38) ends with a zero word
JEWEL_WHITE = 0x993C
MAX_NODES = 40
PALETTE_NAMES = 0x43CE8
PALETTE_COUNT = 39
TYPE_PALETTE_BASE = {2: 0, 6: 9, 9: 15, 12: 21, 15: 27, 18: 33}
LOOK_TEXTURES = range(18, 35)  # JewelNum 1..16 -> 18..33, chao_rare00 34
LOOK_NODES_EXCLUDED = [18, 21, 27, 37, 39]


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


def option(argv, name, default):
    if name in argv:
        position = argv.index(name)
        value = argv[position + 1]
        del argv[position:position + 2]
        return Path(value)
    return default


def rgb565(value):
    r, g, b = (value >> 11) & 31, (value >> 5) & 63, value & 31
    return [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]


def palette_names(rel, table):
    """Read the GVP names through the REL's own relocations to .data 0x43CE8."""
    imports = struct.unpack_from('>II', rel, 0x28)
    module = struct.unpack_from('>I', rel, 0)[0]
    targets = {}
    for entry in range(imports[1] // 8):
        owner, offset = struct.unpack_from('>II', rel, imports[0] + entry * 8)
        if owner != module:
            continue
        section = position = 0
        while offset + 8 <= len(rel):
            delta, kind, target, addend = struct.unpack_from('>HBBI', rel, offset)
            offset += 8
            position += delta
            if kind == 203:
                break
            if kind == 202:
                section, position = target, 0
            elif kind == 1 and section == DATA:
                targets[position] = (target, addend)
    names = []
    for index in range(PALETTE_COUNT):
        target = targets.get(PALETTE_NAMES + index * 4)
        if target is None:
            raise SystemExit(f'no relocation for palette name {index}')
        data, _ = section_bytes(rel, table, target[0])
        end = data.find(b'\0', target[1])
        if end < 0 or end - target[1] > 32:
            raise SystemExit(f'palette name {index} is not a short string')
        names.append(data[target[1]:end].decode('ascii'))
    return names


def read_gvp(path):
    raw = path.read_bytes()
    if len(raw) < 16 or raw[:4] != b'GVPL':
        raise SystemExit(f'{path}: not a GVPL palette')
    fmt, count = struct.unpack_from('>H', raw, 8)[0], struct.unpack_from('>H', raw, 14)[0]
    if fmt != 1 or 16 + count * 2 > len(raw):
        raise SystemExit(f'{path}: unexpected palette format {fmt} or count {count}')
    return [rgb565(struct.unpack_from('>H', raw, 16 + i * 2)[0]) for i in range(count)]


def main(argv):
    argv = list(argv)
    out = ROOT / '.local/imports/chao-look.json'
    textures = option(argv, '--textures', ROOT / '.local/extracted/textures/al_body')
    inventory = ROOT / '.local/game-inventory.json'
    game = option(argv, '--game', Path(json.loads(inventory.read_text())['gamePath']) if inventory.exists() else None)
    out = option(argv, '--out', out)
    source = Path(argv[0]) if argv else ROOT / '.local/research/gsne8p-disc/ChaoMain.rel'
    rel = source.read_bytes()
    sha1 = hashlib.sha1(rel).hexdigest()
    if sha1 != EXPECTED_SHA1:
        raise SystemExit(f'{source}: SHA-1 {sha1} is not the GSNE8P ChaoMain.rel ({EXPECTED_SHA1})')
    table = sections(rel)
    rodata, rodata_offset = section_bytes(rel, table, RODATA)
    data, data_offset = section_bytes(rel, table, DATA)
    if (COLOR_TABLE + COLOR_COUNT * 4 > len(rodata) or COLOR_NODES + MAX_NODES * 2 > len(data)
            or GX_COLOR_TABLE + GX_COLOR_COUNT * 4 > len(data) or JEWEL_WHITE + 4 > len(rodata)):
        raise SystemExit('colour tables are outside the REL sections')
    colors = []
    for index in range(COLOR_COUNT):
        word = struct.unpack_from('>I', rodata, COLOR_TABLE + index * 4)[0]
        colors.append([(word >> 16) & 0xFF, (word >> 8) & 0xFF, word & 0xFF, word >> 24])
    gx_colors = [list(data[GX_COLOR_TABLE + i * 4:GX_COLOR_TABLE + i * 4 + 4]) for i in range(GX_COLOR_COUNT)]
    jewel_color = list(rodata[JEWEL_WHITE:JEWEL_WHITE + 4])
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
        'gxColorTable': gx_colors,
        'gxColorTableOrder': 'index = ColorNum - 1; [r, g, b, a] GX colour; the material colour used when JewelNum == 0',
        'jewel': {'materialColor': jewel_color, 'alBodyTextureIndex': 'JewelNum + 17', 'texmap': 1},
        'shiny': {'alBodyTextureIndex': 34, 'texmap': 2},
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    rows = [row.split(',') for row in (textures / 'index.txt').read_text().splitlines() if ',' in row]
    body_files = {}
    for index in LOOK_TEXTURES:
        name = rows[index][1]
        target = out.parent / f'chao-look-{Path(name).stem.lower()}.png'
        shutil.copyfile(textures / name, target)
        body_files[str(index)] = target.name
    look['bodyTextureFiles'] = body_files
    look['lookNodesExcluded'] = LOOK_NODES_EXCLUDED
    names = palette_names(rel, table)
    look['palettes'] = {
        'slots': names,
        'typeBase': {str(t): b for t, b in TYPE_PALETTE_BASE.items()},
        'note': 'slot base + 0..5 = z,n,s,f,r,p for hero adults; child base 0: NC00, NC01, HCZ..HCP (2..7), DC (8)',
        'colors': ({name: read_gvp(game / 'resource/gd_PC' / name.lower()) for name in names} if game else None),
        'colorFormat': 'GVP RGB565 expanded to [r, g, b]',
    }
    out.write_text(json.dumps(look, indent=2) + '\n')
    print(json.dumps({'out': str(out), 'colors': len(colors), 'nodes': nodes}))


if __name__ == '__main__':
    main(sys.argv[1:])
