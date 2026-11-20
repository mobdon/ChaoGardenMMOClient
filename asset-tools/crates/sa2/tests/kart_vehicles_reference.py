"""Compare the kart-vehicles JSON to all 15 SA Tools 786 kartphysics INIs."""
import hashlib
import json
from pathlib import Path
import struct
import sys

report, splits, config = map(Path, sys.argv[1:])
import configparser
ini = configparser.ConfigParser(interpolation=None, strict=False)
ini.read_string("[module]\n" + config.read_text(encoding="utf-8-sig"))
sections = [s for s in ini.values() if s.get("type") == "kartphysics"]
rows = json.loads(report.read_text())
assert len(rows) == len(sections) == 15
fields = ["Acceleration","BrakePower","Deceleration","SpeedCap","Weight","Unk1",
          "DriftHandling","DriftSpeedThreshold","Unk2","TopSpeed"]
for i,(row,section) in enumerate(zip(rows,sections)):
    name = Path(section["filename"]).stem
    assert (row["slot"],row["name"],row["source_rva"]) == (i,name,int(section["address"],16))
    ref = dict(line.split("=",1) for line in (splits/(name+".ini")).read_text(encoding="utf-8-sig").splitlines() if line.strip())
    words = b""
    for key in fields:
        field = key.replace("Unk","Unknown") if key.startswith("Unk") else key
        actual = row["physics"][field[0].lower()+field[1:]]
        expected = struct.pack("<f",float(ref[key]))
        assert struct.pack("<f",actual) == expected,(name,key)
        words += expected
    assert hashlib.sha256(words).hexdigest() == row["sha256"]
print("Matched 15 named profiles, 150 float fields and 15 raw row hashes.")
