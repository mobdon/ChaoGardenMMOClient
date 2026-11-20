"""Compare `kart-native-layout` with CartD's placement calls run in main.dol.
Usage: python kart_layout_dol.py CLI EXE DATA_DLL PRIVATE_DIR   (SA2_MAIN_DOL set)

For every occurrence after the first, this runs the GSNE8P routines CartD's
imports resolve to, in CartD's order (+0x7300..+0x7348): njPushMatrixEx,
njUnitMatrix(0), njRotateY(0, previous yaw), njCalcVector(0, d, d) on the
previous row's displacement, njPopMatrixEx, then njAddVector(previous
origin, d, origin). Every origin must match the Rust result bit for bit.
Needs the shared harness tests/dol_ppc.py next to this file.
"""
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402

cli, exe, dll, work = map(Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=True)
for old in work.glob("*.json"):
    old.unlink()  # the CLI never overwrites an existing report


def rows(path):
    """CartCourseModels displacement (+8) and turn (+0x14) from the PE export."""
    d = path.read_bytes()
    pe = struct.unpack_from("<I", d, 0x3C)[0]
    nsec, opt = struct.unpack_from("<HxxxxxxxxxxxxH", d, pe + 6)
    secs = [struct.unpack_from("<8xIIII", d, pe + 24 + opt + 40 * i) for i in range(nsec)]
    def off(rva):
        for size, va, raw, ptr in secs:
            if va <= rva < va + max(size, raw):
                return rva - va + ptr
        raise KeyError(hex(rva))
    exp = off(struct.unpack_from("<I", d, pe + 24 + 96)[0])
    # Export directory: +24 name count, +28 functions, +32 names, +36 ordinals.
    n, funcs, names, ords = struct.unpack_from("<IIII", d, exp + 24)
    for i in range(n):
        name_at = off(struct.unpack_from("<I", d, off(names) + 4 * i)[0])
        if d[name_at:d.index(b"\0", name_at)] == b"CartCourseModels":
            ordinal = struct.unpack_from("<H", d, off(ords) + 2 * i)[0]
            table = off(struct.unpack_from("<I", d, off(funcs) + 4 * ordinal)[0])
            return [(struct.unpack_from("<3f", d, table + 108 * k + 8),
                     struct.unpack_from("<i", d, table + 108 * k + 20)[0]) for k in range(85)]
    raise SystemExit("CartCourseModels export missing")


row = rows(dll)
m = Dol(os.environ["SA2_MAIN_DOL"]).machine()
VEC, PREV, OUT = 0x81710000, 0x81710010, 0x81710020
bits = lambda v: struct.unpack(">I", struct.pack(">f", v))[0]
for name in ("101", "280", "beginner", "standard", "expert"):
    out = work / f"layout-{name}.json"
    subprocess.run([str(cli), "kart-native-layout", str(exe), str(out), str(dll), name], check=True)
    ours = json.loads(out.read_text())
    assert ours[0]["position"] == [0.0, 0.0, 0.0] and ours[0]["yaw"] == 0
    for prev, cur in zip(ours, ours[1:]):
        disp, turn = row[prev["pieceId"]]
        for k in range(3):
            m.put(VEC + 4 * k, "f", disp[k])
            m.put(PREV + 4 * k, "f", prev["position"][k])
        m.call(0x80116098)                                  # njPushMatrixEx
        m.call(0x801153E4, r3=0)                            # njUnitMatrix
        m.call(0x80115828, r3=0, r4=prev["yaw"] & 0xFFFFFFFF)  # njRotateY
        m.call(0x801154EC, r3=0, r4=VEC, r5=VEC)            # njCalcVector
        m.call(0x801160E4)                                  # njPopMatrixEx
        m.call(0x80027D1C, r3=PREV, r4=VEC, r5=OUT)         # njAddVector
        native = [m.get(OUT + 4 * k, "I") for k in range(3)]
        assert native == [bits(v) for v in cur["position"]], (name, cur)
        assert cur["yaw"] == (prev["yaw"] + turn + 2**31) % 2**32 - 2**31
    print(name, len(ours), "origins match")
