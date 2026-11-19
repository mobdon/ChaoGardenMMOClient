"""Build the metadata-only engine source-span registry from oracle manifests.

This inventories evidence spans, including unported dependencies and reference
resources. Its count is not a count or percentage of completed game functions.
"""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROM_SHA256 = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"


def registry():
    spans = []

    def visit(value, fixture, path):
        if isinstance(value, dict):
            if all(key in value for key in ("offset", "length", "sha256")):
                spans.append({"fixture": fixture, "key": path, "name": value.get("name", path),
                              "offset": value["offset"], "length": value["length"], "sha256": value["sha256"]})
            for key, child in value.items():
                visit(child, fixture, path+"."+key if path else key)
        elif isinstance(value, list):
            for index, child in enumerate(value):
                visit(child, fixture, path+"["+str(index)+"]")

    for fixture in sorted((ROOT/"tests").glob("*vectors.json")):
        value = json.loads(fixture.read_text(encoding="utf8"))
        digest = value.get("romSha256", value.get("sourceSha256"))
        if digest != ROM_SHA256:
            raise ValueError("Oracle fixture has no matching source identity: "+fixture.name)
        visit(value, "tests/"+fixture.name, "")
    return {"schemaVersion": 1, "romSha256": ROM_SHA256, "wholeGamePorted": False,
            "scope": "Source evidence spans only; includes partial ports, boundaries and resources. See docs/PORT-STATUS.md for behavior coverage.",
            "spans": spans}


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--generate", action="store_true")
    p.add_argument("--rom")
    a = p.parse_args()
    actual = registry()
    target = ROOT/"engine-sources.json"
    if a.generate:
        target.write_text(json.dumps(actual, indent=2)+"\n", encoding="utf8")
    elif json.loads(target.read_text(encoding="utf8")) != actual:
        raise SystemExit("Source registry differs from oracle metadata; regenerate and review")
    if a.rom:
        with open(a.rom, "rb") as f:
            rom = f.read(0x800001)
        if len(rom) != 0x800000 or hashlib.sha256(rom).hexdigest() != ROM_SHA256:
            raise ValueError("Exact local A3VE rev0 required")
        for span in actual["spans"]:
            offset, length = span["offset"], span["length"]
            if offset < 0 or length <= 0 or offset+length > len(rom):
                raise ValueError("Source span outside cartridge: "+str(span))
            if hashlib.sha256(rom[offset:offset+length]).hexdigest() != span["sha256"]:
                raise ValueError("Source hash differs: "+str(span))
    print(json.dumps({"evidenceSpans": len(actual["spans"]), "sourceSha256": ROM_SHA256, "wholeGamePorted": False}))
