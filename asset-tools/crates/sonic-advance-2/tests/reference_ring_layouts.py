"""Fingerprint the pinned stage-authoring CSVs without exporting any positions.

Pass the root of a local SAT-R checkout, or a directory of flattened reference
filenames, then the destination JSON. No cartridge or Rust implementation is used.
"""
import csv
import hashlib
import json
import struct
import sys
from collections import defaultdict
from pathlib import Path


def source_bytes(root, path):
    source = root / path
    return (source if source.is_file() else root / path.replace("/", "_")).read_bytes()


def describe(root, name, level_ids, types=None, category=None):
    category = category or ("rings" if types is None else "interactables")
    path = "data/sa2/maps/" + name + "/entities/" + category + ".csv"
    raw = source_bytes(root,path)
    header, *rows = csv.reader(raw.decode().splitlines())
    assert header[:2] == ["Adv2", category.upper()]
    groups = defaultdict(list)
    for row in rows:
        if row and row[0]:
            region_x, region_y, tile_x, tile_y = map(int, row[:4])
            point = struct.pack("<ii",256*region_x + 8*tile_x,256*region_y + 8*tile_y)
            if types is not None:
                kind = int(row[4]) if row[4].isdecimal() else types[row[4]]
                point += bytes([kind]+[int(n)&255 for n in row[5:]])
            groups[region_y, region_x].append(point)
    ordered = [point for region in sorted(groups) for point in groups[region]]
    packed = b"".join(ordered)
    return {
        "source": path, "level_ids": level_ids, "count": len(ordered),
        "source_git_blob": hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest(),
        "positions_sha256": hashlib.sha256(packed).hexdigest(),
    }


if __name__ == "__main__":
    root, destination = map(Path, sys.argv[1:3])
    types=None
    if sys.argv[3:] == ["--interactables"]:
        import re
        definitions=source_bytes(root,"include/constants/sa2/interactables.h").decode()
        types={name:int(number) for name,number in re.findall(r"^#define IA__(\w+)\s+(\d+)",definitions,re.M)}
    category=None
    if sys.argv[3:] == ["--enemies"]:
        import re
        category="enemies"
        definitions=source_bytes(root,"include/constants/sa2/enemies.h").decode()
        types={name:int(number) for name,number in re.findall(r"^#define ENEMY__(\w+)\s+(\d+)",definitions,re.M)}
    records = []
    for zone in range(1, 8):
        start = 4 * (zone-1)
        for act, ids in [("1", [start]), ("2", [start+1, start+3]), ("boss", [start+2])]:
            records.append(describe(root, f"zone_{zone}/act_{act}", ids, types, category))
    for act, level in [("xx", 28), ("ta53", 29), ("unused", 30)]:
        records.append(describe(root, "zone_final/act_" + act, [level], types, category))
    destination.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
