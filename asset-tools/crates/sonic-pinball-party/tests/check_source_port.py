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


def fixture_specs(rom_path, rom, bios_reference_tool):
    """Only reviewed, registered source-port fixtures belong in publication."""
    return [
        ("control_reference", "generate", [rom_path], "control-vectors.json"),
        ("startup_reference", "generate", [rom_path], "startup-vectors.json"),
        ("math_reference", "generate", [rom_path, bios_reference_tool], "math-vectors.json"),
        ("ball_reference", "generate", [rom], "ball-vectors.json"),
        ("flipper_reference", "generate", [rom, False], "flipper-vectors.json"),
        ("flipper_reference", "generate", [rom, True], "flipper-native-vectors.json"),
        ("frame_reference", "generate", [rom, False], "frame-vectors.json"),
        ("frame_reference", "generate", [rom, True], "frame-native-leaf-vectors.json"),
        ("collision_reference", "reference", [rom_path], "collision-vectors.json"),
        ("animation_reference", "generate", [rom_path], "animation-vectors.json"),
        ("graphics_frame_reference", "generate", [rom_path], "graphics_frame-vectors.json"),
        ("display_reference", "generate", [rom_path], "display-vectors.json"),
        ("oam_reference", "generate", [rom_path], "oam-vectors.json"),
        ("oam_render_reference", "generate", [rom_path], "oam_render-vectors.json"),
        ("oam_tree_reference", "generate", [rom_path], "oam_tree-vectors.json"),
        ("palette_reference", "generate", [rom], "palette-vectors.json"),
        ("sound_reference", "generate", [rom_path], "sound-vectors.json"),
        ("sound_dma_reference", "generate", [rom_path], "sound_dma-vectors.json"),
        ("mplay_reference", "generate", [rom_path], "mplay-vectors.json"),
        ("game_graphics_reference", "generate", [rom_path], "game-graphics-vectors.json"),
        ("lifecycle_reference", "generate", [rom], "lifecycle-vectors.json"),
        ("lifecycle_tables_reference", "generate", [rom], "lifecycle_tables-vectors.json"),
        ("interpolation_reference", "generate", [rom_path], "interpolation-vectors.json"),
        ("intro_reference", "generate", [rom_path], "intro-vectors.json"),
        ("intro_callbacks_reference", "generate", [rom_path], "intro-callbacks-vectors.json"),
        ("table_events_reference", "make", [rom], "table_events-vectors.json"),
        ("table_actions_reference", "make", [rom], "table_actions-vectors.json"),
        ("table_presentation_reference", "make", [rom], "table_presentation-vectors.json"),
        ("table_presentation_frame_reference", "make", [rom], "table_presentation_frame-vectors.json"),
        ("table_frame_reference", "make", [rom], "table_frame-vectors.json"),
        ("table_bootstrap_reference", "make", [rom], "table_bootstrap-vectors.json"),
        ("table_initialize_reference", "make", [rom], "table_initialize-vectors.json"),
        ("table_collision_setup_reference", "make", [rom], "table_collision_setup-vectors.json"),
        ("sequence_reference", "generate", [rom_path], "sequence-vectors.json"),
        ("sequence_extended_reference", "generate", [rom], "sequence-extended-vectors.json"),
        ("nudge_reference", "generate", [rom, False], "nudge-vectors.json"),
        ("nudge_reference", "generate", [rom, True], "nudge-native-vectors.json"),
    ]


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--rom", required=True, type=Path)
    p.add_argument("--bios-reference-tool", type=Path)
    a = p.parse_args()
    with a.rom.open("rb") as f:
        rom = f.read(0x800001)
    import reference_parity
    reference_parity.validate_rom(rom)
    fixtures = fixture_specs(a.rom, rom, a.bios_reference_tool)
    for module, function, arguments, fixture in fixtures:
        check(module, function, arguments, fixture)
        print("Verified "+fixture, flush=True)
    import engine_sources
    expected = json.loads((Path(__file__).resolve().parent.parent/"engine-sources.json").read_text(encoding="utf8"))
    if engine_sources.registry() != expected:
        raise ValueError("Engine source registry differs")
    print(json.dumps({"verifiedOracleFixtures": len(fixtures), "primaryNumericalCrosscheck": bool(a.bios_reference_tool), "wholeGamePorted": False}))
