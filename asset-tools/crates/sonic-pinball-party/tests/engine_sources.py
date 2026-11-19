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

    def visit(value, fixture, path, parent_space="rom"):
        if isinstance(value, dict):
            space = value.get("space", parent_space)
            if all(key in value for key in ("offset", "length", "sha256")):
                if space not in ("rom", "garden"):
                    raise ValueError("Unknown source address space: "+str(space))
                item = {"fixture": fixture, "key": path, "name": value.get("name", path),
                        "offset": value["offset"], "length": value["length"], "sha256": value["sha256"]}
                if space != "rom":
                    item["space"] = space
                spans.append(item)
            for key, child in value.items():
                visit(child, fixture, path+"."+key if path else key, space)
        elif isinstance(value, list):
            for index, child in enumerate(value):
                visit(child, fixture, path+"["+str(index)+"]", parent_space)

    # The oracle driver is the reviewed publication manifest. Unregistered
    # scratch/in-progress fixtures must not silently change a release registry.
    from check_source_port import fixture_specs
    names = [spec[3] for spec in fixture_specs(None, None, None)]
    if len(names) != len(set(names)):
        raise ValueError("Duplicate registered oracle fixture")
    for name in sorted(names):
        fixture = ROOT/"tests"/name
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
        import reference_parity
        garden, consumed = reference_parity.decode(rom[reference_parity.GARDEN_OFFSET:], reference_parity.GARDEN_LENGTH)
        if consumed != reference_parity.GARDEN_STORED or hashlib.sha256(garden).hexdigest() != reference_parity.GARDEN_SHA256:
            raise ValueError("Embedded Garden source identity differs")
        for span in actual["spans"]:
            offset, length = span["offset"], span["length"]
            source = garden if span.get("space", "rom") == "garden" else rom
            if offset < 0 or length <= 0 or offset+length > len(source):
                raise ValueError("Source span outside its address space: "+str(span))
            if hashlib.sha256(source[offset:offset+length]).hexdigest() != span["sha256"]:
                raise ValueError("Source hash differs: "+str(span))
    print(json.dumps({"evidenceSpans": len(actual["spans"]), "sourceSha256": ROM_SHA256, "wholeGamePorted": False}))
