"""Run the pinned C rectangle predicate across two player poses around a ring."""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

if __name__ == "__main__":
    refs, zig, local = map(Path, sys.argv[1:])
    if ".local" not in local.resolve().parts:
        raise ValueError("Keep generated C and executables under .local")
    local.mkdir(parents=True, exist_ok=True)
    names = {
        "include/rect.h": ["RECT_WIDTH", "RECT_HEIGHT", "RECT_LEFT", "RECT_RIGHT", "RECT_TOP", "RECT_BOTTOM"],
        "include/game/shared/stage/rings_manager.h": ["RECT_TOUCHING_RING"],
    }
    declarations = []
    fingerprints = {}
    for source, macros in names.items():
        path = refs / source
        if not path.is_file():
            path = refs / source.replace("/", "_")
        raw = path.read_bytes()
        fingerprints[source] = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        lines = raw.decode().splitlines()
        for macro in macros:
            i = next(i for i,line in enumerate(lines) if re.match(r"^#define " + macro + r"\(",line))
            while True:
                declarations.append(lines[i])
                if not lines[i].endswith("\\"):
                    break
                i += 1
    harness = """
#include <stdio.h>
#define TILE_WIDTH 8
typedef struct { signed char left,top,right,bottom; } Rect8;
int main(void) {
    for (int pose=0;pose<2;pose++) {
        Rect8 rect={-6,pose?-9:-14,6,pose?9:14};
        for(int dy=-40;dy<=40;dy++) for(int dx=-24;dx<=24;dx++)
            putchar(RECT_TOUCHING_RING(96+dx,96+dy,96,96,&rect) ? '1' : '0');
    }
}
"""
    c = local / "overlap.c"
    c.write_text("\n".join(declarations) + "\n" + harness)
    exe = local / "overlap.exe"
    subprocess.run([str(zig), "cc", str(c), "-o", str(exe)], check=True)
    result = subprocess.check_output([str(exe)])
    print(json.dumps({"git_blobs": fingerprints, "cases": len(result), "sha256": hashlib.sha256(result).hexdigest()}))
