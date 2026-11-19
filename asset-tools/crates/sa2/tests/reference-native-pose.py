"""Independent owner-local SA3D 1.2.2 pose oracle; never a production parser.

Run with the prepared Blender Python 3.13 interpreter and --reference-root set
to the original checkout containing ignored SAIO tools/native extractions. This
script delegates native file parsing, interpolation and hierarchy evaluation to
the original SA3D assembly. It does not implement those algorithms in Python.
Only a new output inside this worktree's ignored .local folder is permitted.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys

# Reference packages are inputs too; prevent import caches beside their files.
sys.dont_write_bytecode = True


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def output_path(value):
    worktree = Path(__file__).resolve().parents[4]
    path = Path(value).absolute()
    if ".." in path.parts or not path.is_relative_to(worktree / ".local"):
        raise ValueError("oracle output must be inside this worktree's .local")
    for parent in [path, *path.parents]:
        if parent.is_symlink() or (hasattr(parent, "is_junction") and parent.is_junction()):
            raise ValueError("oracle output links/junctions refused")
    if path.exists():
        raise FileExistsError(path)
    return path


def matrix_columns(matrix):
    # System.Numerics stores row-vector transforms. Transposition changes only
    # matrix representation; coordinates, orientation and scale are untouched.
    return [[float(getattr(matrix, f"M{column + 1}{row + 1}"))
             for column in range(4)] for row in range(4)]


def nodes_reference(model):
    nodes = list(model.GetTreeNodes())
    indices = {str(node.Label): index for index, node in enumerate(nodes)}
    if len(indices) != len(nodes):
        raise ValueError("source node labels are not unique")
    return [{
        "index": index,
        "label": str(node.Label),
        "parentIndex": None if node.Parent is None else indices[str(node.Parent.Label)],
        "flags": int(node.Attributes),
        "localMatrix": matrix_columns(node.LocalMatrix),
        "worldMatrix": matrix_columns(node.GetWorldMatrix()),
    } for index, node in enumerate(nodes)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=output_path)
    args = parser.parse_args()
    reference = args.reference_root.resolve(strict=True)
    addon = reference / ".local/tools/saio-2.2.2"
    sys.path.insert(0, str(reference / ".local/tools/saio-python"))
    # Process-only roll-forward, matching the existing local conversion tools.
    os.environ.setdefault("DOTNET_ROLL_FORWARD", "Major")
    import pythonnet
    pythonnet.load("coreclr", runtime_config=str(addon / "DLL/SAIO.NET.runtimeconfig.json"))
    import clr
    assembly_path = addon / "DLL/SA3D.Modeling.dll"
    clr.AddReference(str(assembly_path))
    from SA3D.Modeling.File import ModelFile, AnimationFile
    from SA3D.Modeling.ObjectData import Node
    from SA3D.Modeling.ObjectData.Enums import NodeAttributes, RotationUpdateMode
    from System.Numerics import Vector3

    model_folder = ".local/extracted/pc-chao/Chao/models/AL_RootObject"
    motion_folder = ".local/extracted/pc-chao/Chao/models/al_motion_table"
    names = [("child", "al_ncz"), ("neutral-chaos", "al_nchaos"),
             ("hero-chaos", "al_hchaos"), ("dark-chaos", "al_dchaos"),
             ("tails", "al_tails"), ("knuckles", "al_knuckles"), ("amy", "al_amy")]
    sources = {}

    def source(relative):
        path = reference / relative
        digest = sha256(path)
        sources[relative] = digest
        return path, digest

    records = []
    informational_version = None
    for identifier, filename in names:
        relative = f"{model_folder}/{filename}.sa2mdl"
        path, digest = source(relative)
        original = ModelFile.ReadFromFile(str(path)).Model
        if informational_version is None:
            assembly = original.GetType().Assembly
            informational_version = next(str(attribute.ConstructorArguments[0].Value)
                for attribute in assembly.GetCustomAttributesData()
                if str(attribute.AttributeType.Name) == "AssemblyInformationalVersionAttribute")
            if informational_version != "1.2.2+f23c73df68c0c69a96f28746c41b6fc32643340a":
                raise ValueError(f"unverified SA3D oracle version: {informational_version}")
        rest = nodes_reference(original)
        if len(rest) != 40:
            raise ValueError(f"expected original 40-node Chao: {identifier}")
        motions = []
        for name in ["alm_tati", "alm_aruki_ncz"]:
            motion_relative = f"{motion_folder}/{name}.saanim"
            motion_path, motion_digest = source(motion_relative)
            motion = AnimationFile.ReadFromFile(str(motion_path)).Animation
            frame_count = int(motion.GetFrameCount())
            if str(motion.InterpolationMode) != "Linear":
                raise ValueError("this oracle slice verifies linear native transform motion")
            frames = sorted(set([-1.0, 0.0, 0.5, 1.0, 3.25, 8.5,
                                 frame_count - 1.5, float(frame_count - 1), float(frame_count + 1)]))
            samples = []
            for requested in frames:
                copied = original.DeepSimpleCopy()
                animated_nodes = list(copied.GetAnimTreeNodes())
                if len(animated_nodes) != int(motion.NodeCount):
                    raise ValueError("native animation-node mapping does not match source motion")
                evaluated = max(0.0, requested)
                for entry in motion.Keyframes:
                    frame = entry.Value.GetFrameAt(evaluated)
                    # This public SA3D API retains absent-channel rest values.
                    animated_nodes[int(entry.Key)].UpdateTransforms(
                        frame.Position, frame.EulerRotation, frame.Scale)
                samples.append({"requestedFrame": requested, "evaluatedFrame": evaluated,
                                "nodes": nodes_reference(copied)})
            motions.append({"name": name, "source": motion_relative,
                            "sha256": motion_digest, "frameCount": frame_count,
                            "interpolation": str(motion.InterpolationMode),
                            "shortRotation": bool(motion.ShortRot), "samples": samples})
        records.append({"id": identifier, "source": relative, "sha256": digest,
                        "rest": rest, "motions": motions})

    # Noncommuting synthetic transforms explicitly distinguish default rotation
    # order from the legacy 1.2.2 RotateZYX branch, without reproducing its math.
    synthetic = []
    import math
    for flag in [0, 0x20]:
        node = Node()
        node.Label = "synthetic-root"
        node.SetAllNodeAttributes(NodeAttributes(flag), RotationUpdateMode.Keep)
        node.Position = Vector3(2.0, -3.0, 5.0)
        node.Scale = Vector3(1.2, 0.8, 1.1)
        bams = [3233, 7864, -4693]
        node.EulerRotation = Vector3(*(value * math.tau / 65536 for value in bams))
        synthetic.append({"flags": flag, "position": [2.0, -3.0, 5.0],
                          "scale": [1.2, 0.8, 1.1], "rotationBams": bams,
                          "localMatrix": matrix_columns(node.LocalMatrix)})
    for relative, digest in sources.items():
        if sha256(reference / relative) != digest:
            raise ValueError(f"reference input changed: {relative}")
    report = {
        "schemaVersion": 1, "oracle": "SAIO 2.2.2 bundled SA3D.Modeling public API",
        "assemblyVersion": informational_version, "assemblySha256": sha256(assembly_path),
        "scriptSha256": sha256(Path(__file__)), "pythonVersion": sys.version.split()[0],
        "coordinateSpace": "original native units; column-vector matrix representation",
        "framePolicy": "negative requests clamp to zero; SA3D GetFrameAt per-channel endpoint clamping; no looping or inferred FPS",
        "sourceInputsUnchanged": True, "models": records, "syntheticRotationOrders": synthetic,
        "limitations": ["No native compiled gameplay execution", "No Chao weighted-mesh skinning or morph evaluation",
                        "No cache/envelope/quaternion animation variants verified", "No blending, FPS or native action-state machine inferred"],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, separators=(",", ":"), allow_nan=False)
        output.write("\n")
    print(json.dumps({"models": len(records), "nativePoses": sum(1 + sum(len(motion["samples"]) for motion in model["motions"]) for model in records),
                      "assemblyVersion": informational_version, "output": args.output.name}))


if __name__ == "__main__":
    main()
