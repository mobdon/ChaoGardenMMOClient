"""Compare the mission-ranks JSON with seven private SA Tools split INIs."""
import argparse
import configparser
import json
import re
from pathlib import Path

def compare(output, reference, enums):
    enum = enums.read_text().split("enum LevelIDs", 1)[1].split("};", 1)[0]
    ids = {}
    value = -1
    for name, explicit in re.findall(r"LevelIDs_(\w+)(?:\s*=\s*(\d+))?", enum):
        value = int(explicit) if explicit else value + 1
        ids[name] = value
    actual = json.loads(output.read_text())
    expected = []
    for table in [*(f"Mission {i}" for i in range(1, 6)), "Route 101", "Route 280"]:
        ini = configparser.ConfigParser(interpolation=None)
        ini.read(reference / (table + ".ini"), encoding="utf-8-sig")
        for section in ini.sections():
            route = table.startswith("Route")
            fields = ini[section]
            values = [fields[k + "Rank"] for k in "DCBA"]
            timed = ":" in values[0]
            thresholds = [sum(int(x)*factor for x,factor in zip(v.split(":"),(60,1))) if timed else int(v) for v in values]
            expected.append((table if route else None, int(section)+1 if route else int(table[-1]),
                             ids[fields["Level"] if route else section], "seconds" if timed else "storedScore", thresholds))
    records = [(r["course"],r["mission"],r["level"],r["thresholds"]["unit"],r["thresholds"]["dThroughA"]) for r in actual]
    if len(expected) != 164 or records != expected:
        raise RuntimeError("mission rank rows differ from the independent split")
    print(f"Matched {len(records)} rank rows and {4*len(records)} thresholds.")
if __name__ == "__main__":
    parser=argparse.ArgumentParser(__doc__)
    for name in ("output","reference","enums"):
        parser.add_argument(name,type=Path)
    args=parser.parse_args()
    compare(args.output,args.reference,args.enums)
