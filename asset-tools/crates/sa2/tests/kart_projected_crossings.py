"""Compare projected CartD crossings against PC CartCourseModels in double precision.
Inputs: private crossing-directory JSON and Steam Data_DLL.dll. No game launch.
"""
import json
import math
import struct
import sys
from pathlib import Path

import pefile

report = json.loads(Path(sys.argv[1]).read_text())
pe = pefile.PE(sys.argv[2])
export, = [e for e in pe.DIRECTORY_ENTRY_EXPORT.symbols if e.name == b"CartCourseModels"]
models = pe.get_data(export.address, 85 * 108)
for course in report["courses"]:
    x = y = z = 0.
    yaw = 0
    maximum = 0.
    cursor = 0
    for occ, pid in enumerate(course["piece_ids"]):
        pose = course["projected_occurrence_poses"][occ]
        assert pose["yaw_bams"] == yaw
        maximum = max(maximum, math.dist(pose["translation"], (x, y, z)))
        angle = math.radians((yaw % 65536) * 360 / 65536)
        sine, cosine = math.sin(angle), math.cos(angle)
        def world(lx, lz):
            return (x + lx * cosine + lz * sine, z - lx * sine + lz * cosine)
        for quad in report["piece_local_xz_quads"][pid]:
            actual = course["projected_segment_xz"][cursor]
            assert len(actual) == 4
            maximum = max(maximum, *(math.dist(world(*p), q) for p, q in zip(quad, actual)))
            cursor += 1
        dx, dy, dz, turn = struct.unpack_from("<3fi", models, 108*pid + 8)
        x, z = world(dx, dz)
        y += dy
        yaw = (yaw + turn + 2**31) % 2**32 - 2**31
    assert cursor == len(course["projected_segment_xz"]) == course["segment_count"]
    print(course["course"], cursor, maximum)
    assert maximum < 0.025
