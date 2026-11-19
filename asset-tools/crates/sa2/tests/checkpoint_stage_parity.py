"""Check shared checkpoint contact paths offline, including imported engine targets."""
import hashlib
import os
import struct
from pathlib import Path

root = Path(os.environ["SA2_GAME_PATH"]) / "resource/gd_PC"


def load(path):
    data = path.read_bytes()
    word = lambda off: struct.unpack_from(">I", data, off)[0]
    section = word(16) + 8
    start = word(section) & ~1
    imports = {}
    for module, cursor in struct.iter_unpack(">II", data[word(40):word(40)+word(44)]):
        target, off = None, 0
        while True:
            inc, kind, sec, addend = struct.unpack_from(">HBBI", data, cursor)
            cursor += 8
            if kind == 203:
                break
            if kind == 202:
                target, off = sec, 0
                continue
            off += inc
            if target == 1 and module == 0:
                imports[off] = (kind, sec, addend)
    return data[start:start+word(section+4)], imports


city_path = root/"stg13D.rel"
assert hashlib.sha256(city_path.read_bytes()).hexdigest() == "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b"
city, reference = load(city_path)
matched = []
for path in sorted(root.glob("stg*D.rel")):
    text, imports = load(path)
    segment = city[0x446d8:0x44738]
    start = text.find(segment)
    if start == -1:
        continue
    assert text.find(segment,start+1) == -1, path
    shift = start-0x446d8
    # The local spin-speed call lies between these spans and is excluded.
    assert text[0x4473c+shift:0x447a8+shift] == city[0x4473c:0x447a8], path
    for a,b in [(0x43cac,0x43cc0),(0x43d84,0x43dbc),(0x446d8,0x44738),(0x4473c,0x447a8)]:
        assert text[a+shift:b+shift] == city[a:b], path
        expected={off-a:v for off,v in reference.items() if a<=off<b}
        actual={off-shift-a:v for off,v in imports.items() if a+shift<=off<b+shift}
        assert actual == expected, path
    matched.append(path.stem)
assert len(matched)==50, matched
print("50 stage modules match the contact gate, player-state handoff instructions and external references.")
