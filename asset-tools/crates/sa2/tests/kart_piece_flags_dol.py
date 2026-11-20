"""Check the surface flags `kart-piece-collision` reports against main.dol.
Usage: python kart_piece_flags_dol.py PRIVATE_DIR DATA_DLL   (SA2_MAIN_DOL set)

Run after kart_piece_collision_ppc.py has written pieces-*.json to
PRIVATE_DIR. For every tasked occurrence, this calls the dynamic land
register (GSNE8P 0x800232A4, PC import 0x8002305c) with CartD's flags
0x20000001, the occurrence's posed rotation and its attachment centre read
from Data_DLL, and compares the stored flags. Needs tests/dol_ppc.py.
"""
import json
import os
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol, SDA_BASE  # noqa: E402

work, dll = map(Path, sys.argv[1:])
image = dll.read_bytes()
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
LIST, TASK, TWP, OBJ, ATTACH = 0x81720000, 0x81721000, 0x81721100, 0x81721200, 0x81721300
m.put(SDA_BASE - 0x79E0, "I", LIST)
m.put(TASK + 0x34, "I", TWP)
total = 0
for report in sorted(p for p in work.glob("pieces-*.json") if not p.name.endswith(".manifest.json")):
    for piece in json.loads(report.read_text())["pieces"]:
        center = struct.unpack_from("<3f", image, piece["attachment"] + 0x18)
        m.put(SDA_BASE - 0x79E4, "h", 0)
        m.put(OBJ + 4, "I", ATTACH)
        for k in range(3):
            m.put(OBJ + 0x14 + 4 * k, "i", piece["rotation"][k])
            m.put(ATTACH + 0x18 + 4 * k, "f", center[k])
        m.call(0x800232A4, r3=0x20000001, r4=TASK, r5=OBJ)
        assert m.get(LIST, "I") == piece["surfaceFlags"], (report.name, piece)
        total += 1
print("flags match for", total, "tasked occurrences")
