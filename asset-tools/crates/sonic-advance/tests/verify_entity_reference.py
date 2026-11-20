"""Check every placement digest against a local SAT-R/sa2 reference checkout."""
import argparse
import csv
import hashlib
import re
import struct
from pathlib import Path


def verify(reference):
    evidence = Path(__file__).with_name("entity-evidence.tsv")
    classes = {
        "interactables": ("interactables", "IA"),
        "enemies": ("enemies", "ENEMY"),
        "itemboxes": ("items", "ITEM"),
    }
    with evidence.open() as source:
        for row in csv.DictReader(source, delimiter="\t"):
            category = row["class"]
            header, prefix = classes[category]
            constants = (reference / f"include/constants/sa1/{header}.h").read_text()
            kinds = dict(re.findall(r"^#define\s+" + prefix + r"__(\w+)\s+(\d+)", constants, re.M))
            stage = int(row["stage"])
            if stage < 14:
                zone = f"zone_{stage // 2 + 1}/act_{stage % 2 + 1}"
            else:
                zone = f"chao_hunt/vs_zone_{chr(97 + stage - 14)}"
            path = reference / f"data/sa1/maps/{zone}/entities/{category}.csv"
            # Reference checkouts may materialize Git's LF blobs as Windows CRLF.
            raw = path.read_bytes()
            def blob_id(data):
                return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            if row["csv_blob"] not in (blob_id(raw), blob_id(raw.replace(b"\r\n", b"\n"))):
                raise ValueError(f"unexpected reference blob: {path}")
            rows = csv.reader(raw.decode().splitlines())
            heading = next(rows)
            if heading[2:4] != [row["width"], row["height"]]:
                raise ValueError(f"unexpected region dimensions: {path}")
            digest = hashlib.sha256()
            count = 0
            for record in rows:
                if not record:
                    continue
                rx, ry, x, y = map(int, record[:4])
                kind = int(kinds.get(record[4], record[4]))
                digest.update(struct.pack("<iiB", 256 * rx + 8 * x, 256 * ry + 8 * y, kind))
                digest.update(bytes(int(field) & 255 for field in record[5:] if field))
                count += 1
            if digest.hexdigest() != row["ordered_world_records_sha256"] or count != int(row["count"]):
                raise ValueError(f"placement mismatch: {path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    verify(parser.parse_args().reference)
    print("Verified 54 native entity placement manifests.")
