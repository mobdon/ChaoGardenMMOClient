"""Check every completed engine oracle against local native instructions.

No fixture or native file is written. See each oracle's declared interception
boundaries; passing this suite is not a complete-game acceptance result.
"""
import argparse
import importlib
import json
from pathlib import Path


def check(module, function, arguments, fixture):
    actual = getattr(importlib.import_module(module), function)(*arguments)
    # JSON object keys (including address-keyed native read maps) are strings.
    # Normalize through the fixture encoding before structural comparison.
    actual = json.loads(json.dumps(actual))
    expected = json.loads(Path(__file__).with_name(fixture).read_text(encoding="utf8"))
    if actual != expected:
        raise ValueError("Native oracle differs: "+fixture)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", required=True, type=Path)
    p.add_argument("--bios-reference-tool", type=Path)
    a = p.parse_args()
    with a.rom.open("rb") as f:
        rom = f.read(0x800001)
    import reference_parity
    reference_parity.validate_rom(rom)
    fixtures = [
        ("control_reference", "generate", [a.rom], "control-vectors.json"),
        ("startup_reference", "generate", [a.rom], "startup-vectors.json"),
        ("math_reference", "generate", [a.rom, a.bios_reference_tool], "math-vectors.json"),
        ("ball_reference", "generate", [rom], "ball-vectors.json"),
        ("flipper_reference", "generate", [rom, False], "flipper-vectors.json"),
        ("flipper_reference", "generate", [rom, True], "flipper-native-vectors.json"),
        ("frame_reference", "generate", [rom, False], "frame-vectors.json"),
        ("frame_reference", "generate", [rom, True], "frame-native-leaf-vectors.json"),
        ("collision_reference", "reference", [a.rom], "collision-vectors.json"),
        ("animation_reference", "generate", [a.rom], "animation-vectors.json"),
        ("graphics_frame_reference", "generate", [a.rom], "graphics_frame-vectors.json"),
        ("oam_reference", "generate", [a.rom], "oam-vectors.json"),
        ("oam_render_reference", "generate", [a.rom], "oam_render-vectors.json"),
        ("oam_tree_reference", "generate", [a.rom], "oam_tree-vectors.json"),
        ("palette_reference", "generate", [rom], "palette-vectors.json"),
        ("sound_reference", "generate", [a.rom], "sound-vectors.json"),
        ("mplay_reference", "generate", [a.rom], "mplay-vectors.json"),
        ("game_graphics_reference", "generate", [a.rom], "game-graphics-vectors.json"),
        ("lifecycle_reference", "generate", [rom], "lifecycle-vectors.json"),
        ("lifecycle_tables_reference", "generate", [rom], "lifecycle_tables-vectors.json"),
        ("interpolation_reference", "generate", [a.rom], "interpolation-vectors.json"),
        ("nudge_reference", "generate", [rom, False], "nudge-vectors.json"),
        ("nudge_reference", "generate", [rom, True], "nudge-native-vectors.json"),
    ]
    for module, function, arguments, fixture in fixtures:
        check(module, function, arguments, fixture)
        print("Verified "+fixture, flush=True)
    import engine_sources
    expected = json.loads((Path(__file__).resolve().parent.parent/"engine-sources.json").read_text(encoding="utf8"))
    if engine_sources.registry() != expected:
        raise ValueError("Engine source registry differs")
    print(json.dumps({"verifiedOracleFixtures": len(fixtures), "primaryNumericalCrosscheck": bool(a.bios_reference_tool), "wholeGamePorted": False}))
