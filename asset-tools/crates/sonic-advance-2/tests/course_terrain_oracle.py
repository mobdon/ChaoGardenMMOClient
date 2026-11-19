"""Check retail foreground banks against the pinned SAT-R tree and render local probes.

Usage: python course_terrain_oracle.py ROM SAT_R_TREE_JSON SPAWN_POSITIONS_C
Only dimensions, source identities, coordinates and checksums go to Git.
"""
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
OUT = Path(__file__).resolve().parents[1] / "catalog/native-course-terrain.json"
NAMES = ("height_map.coll", "tile_rot.coll", "metatiles.tilemap2",
         "map_front.bin", "map_back.bin", "flags.coll")


def git_blob(data):
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()


def main(rom_file, tree_file, spawn_file):
    rom = Path(rom_file).read_bytes()
    assert hashlib.sha256(rom).hexdigest() == "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260"
    tree = json.loads(Path(tree_file).read_text())
    assert tree["sha"] == PIN and not tree["truncated"]
    files = {f["path"]: f for f in tree["tree"] if f["type"] == "blob"}
    spawn_c = Path(spawn_file).read_bytes()
    assert git_blob(spawn_c) == files["src/game/sa2/stage/spawn_positions.c"]["sha"]
    pairs = re.findall(r"\] = \{ (\d+), (\d+) \}", spawn_c.decode().split("gCheckpointPositions")[0])
    spawn_bytes = b"".join(struct.pack("<hh", int(x), int(y)) for x, y in pairs)
    assert len(pairs) == 34 and rom.index(spawn_bytes) == 0xd6374

    def word(at):
        return struct.unpack_from("<H", rom, at)[0]

    def ptr(at):
        return struct.unpack_from("<I", rom, at)[0] - 0x08000000

    rows = []
    banks = set()
    for level in range(31):
        if level >= 28:
            name = "zone_final/act_" + ("xx", "ta53", "unused")[level - 28]
        elif level % 4 == 3:
            name = "zone_1/act_dummy"
        else:
            name = f"zone_{level // 4 + 1}/act_" + ("1", "2", "boss")[level % 4]
        desc = ptr(0x0d62d8 + 4 * level)
        offsets = [ptr(desc + 4 * k) for k in range(6)]
        for filename, start in zip(NAMES, offsets):
            path = f"data/sa2/maps/{name}/tilemaps/fg/{filename}"
            source = files[path]
            assert git_blob(rom[start:start + source["size"]]) == source["sha"], path
            banks.add(path)
        head = ptr(0x0d5ce4 + 12 * level)
        mw, mh, width, height = struct.unpack_from("<HHII", rom, desc + 24)
        assert (mw * 96, mh * 96) == (width, height)
        frame_bytes, frames, delay, tile_ptr, static_size, pal = struct.unpack_from("<HBBIII", rom, head + 4)
        tiles = tile_ptr - 0x08000000
        colors = []
        for index in range(256):
            raw = word(pal - 0x08000000 + index * 2)
            rgb = [(raw >> shift) & 31 for shift in [0, 5, 10]]
            colors.append(bytes([(v << 3) | (v >> 2) for v in rgb] + [255]))

        def rgba(x, y, tick, clear=True):
            frame = tick // delay % frames if frames else 0
            if not (0 <= x < width and 0 <= y < height):
                return colors[0]
            cell = y // 96 * mw + x // 96
            for grid in offsets[3:5]:
                block = word(grid + 2 * cell)
                local = (y % 96 // 8) * 12 + x % 96 // 8
                entry = word(offsets[2] + 2 * (144 * block + local))
                a, b = x % 8, y % 8
                if entry & 1024:
                    a = 7 - a
                if entry & 2048:
                    b = 7 - b
                byte = (entry & 1023) * 32 + b * 4 + a // 2
                if frame and byte < frame_bytes:
                    byte += static_size + (frame - 1) * frame_bytes
                color = (rom[tiles + byte] >> (4 * (a % 2))) & 15
                if color:
                    return colors[(entry >> 12) * 16 + color]
            return colors[0] if clear else None

        def view(x, y, w, h, tick):
            return hashlib.sha256(b"".join(rgba(x + dx, y + dy, tick)
                for dy in range(h) for dx in range(w))).hexdigest()

        x, y = map(int, pairs[level])
        probes = []
        def probe(left, top, w, h, tick):
            probes.append([left, top, w, h, tick, view(left, top, w, h, tick)])
        probe(x - 20, y - 12, 41, 25, 0)
        probe(width - 20, height - 18, 28, 26, 0)
        found = False
        if frames:
            # Select a tile whose visible colors actually change. Compare
            # changing source tile indices first to avoid scanning empty cells.
            changed = {i for i in range(frame_bytes // 32)
                if rom[tiles+i*32:tiles+(i+1)*32] != rom[tiles+static_size+i*32:tiles+static_size+(i+1)*32]}
            seen = set()
            for cell in range(mw * mh):
                front, back = (word(grid + cell * 2) for grid in offsets[3:5])
                if (front, back) in seen:
                    continue
                seen.add((front, back))
                for k in range(144):
                    if all((word(offsets[2] + 2*(144*block+k)) & 1023) not in changed for block in (front, back)):
                        continue
                    left, top = cell % mw * 96 + k % 12 * 8, cell // mw * 96 + k // 12 * 8
                    if view(left, top, 8, 8, delay) == view(left, top, 8, 8, 0):
                        continue
                    for tick in (delay-1, delay, delay*(frames-1), delay*frames):
                        probe(left, top, 8, 8, tick)
                    found = True
                    break
                if found:
                    break
            if not found:
                # Some boss maps never expose changing tiles in either layer.
                probe(x - 20, y - 12, 41, 25, delay)
        scenes = []
        if level < 2:
            layers = []
            for idx, prefix in ((113, "data/sa2/tilemaps/stage_1_bg_0_copy"),
                                (3 * level + 2, "data/sa2/maps/zone_1/act_1/tilemaps/bg")):
                header = ptr(0xd5ce4 + idx * 4)
                assert (word(header), word(header + 2)) == (32, 30)
                layout = ptr(header + 24)
                assert git_blob(rom[layout:layout+1920]) == files[prefix+"/tilemap.tilemap2"]["sha"]
                layers.append((ptr(header + 8), layout))

            def backdrop(layer, u, v):
                tileset, layout = layers[layer]
                u, v = u % 256, v % 256
                assert v < 240
                attr = word(layout + 2*(v//8 * 32 + u//8))
                px, py = u % 8, v % 8
                if attr & 1024: px ^= 7
                if attr & 2048: py ^= 7
                n = (rom[tileset + (attr & 1023)*32 + 4*py + px//2] >> (4*(px % 2))) & 15
                return colors[16*(attr//4096) + n] if n else None

            for cx, cy, tick in [(0, 560, 0), (0, 560, 1), (2467, 555, 18), (3720, 1580, 271)]:
                buf = bytearray()
                for sy in range(160):
                    for sx in range(240):
                        nearx, neary = (cx//64,cy//512) if tick else (0,0)
                        farx, fary = (cx//128,cy//256) if tick and 72 <= sy < 159 else (0,0)
                        buf.extend(rgba(cx+sx,cy+sy,tick,False) or backdrop(0,sx+nearx,sy+neary)
                                   or backdrop(1,sx+farx,sy+fary) or colors[0])
                scenes.append((cx,cy,tick,hashlib.sha256(buf).hexdigest()))
        rows.append(dict(scenes=scenes,level=level, source=name, spawn=[x, y], dimensions=[width, height], animated_probe=found, probes=probes))
    output = dict(reference_commit=PIN, reference_bank_count=len(banks), stages=rows)
    OUT.write_text(json.dumps(output, separators=(",", ":"), indent=None).replace('},{"scenes"', '},\n{"scenes"') + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(*sys.argv[1:])
