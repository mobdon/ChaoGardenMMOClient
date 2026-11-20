"""Render owner-local true-geometry emerald sprites with the existing Blender pipeline.

blender --background --factory-startup --python android/tools/render-widget-emeralds.py -- \
  --assets C:/original/client/.local/imports --output .local/imports

Imports standard GLB through Blender, not a native game-format reader. No meshes,
native texture pixels, material alpha or original inputs are rewritten. The
existing brand renderer restores the source camera-normal environment mapping;
its optional PBR styling is explicitly restored to the imported GLB values.
Only metadata may be committed. Original GLBs and rendered PNGs remain local.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import zlib

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
COLORS = ["cyan", "blue", "yellow", "red", "green", "purple", "white"]
EXPECTED = {
    "cyan": "85bf07dfbca806bd74cc3cd6827052234119e14da8e1785e4a927d6d4df20f37",
    "blue": "24f521669881c44aa0c6d729f2748a6883e8e5f677d06cdbf5fd87828f9868ea",
    "yellow": "7d22e93164931afbbffee0701c341b801ca5d61b213ef7721e41b54187ab74cb",
    "red": "d6c05784c2c7d53bb22e4109a35ded4b1de9881627157205e4b5c28ed3aa17f5",
    "green": "d29c74548f7c60f1dfa4a644c665a4560750ded499d9daf9d2a78ee1a8362542",
    "purple": "d121a00b9b875b1425f469fe55e9c5087b5db3f53b06dadf87764c1faba5575c",
    "white": "125ee2041bb69fd508c4ac8591d6378d8de0af0e233e1ec46ab7161eb5ef6bf8",
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalize_png_metadata(path):
    """Retain exact encoded pixels/color profile; omit volatile render receipts.

    Blender writes Date/RenderTime text to PNGs even when visible stamping is
    disabled. This standard PNG container pass does not decode/re-encode an
    image or touch IDAT bytes. Provenance belongs in the separate JSON receipt.
    """
    source = path.read_bytes()
    signature = b"\x89PNG\r\n\x1a\n"
    if not source.startswith(signature) or len(source) > 256 * 1024:
        raise RuntimeError("Unexpected bounded sprite PNG")
    output = bytearray(signature)
    cursor = len(signature)
    while cursor < len(source):
        if cursor + 12 > len(source):
            raise RuntimeError("Truncated PNG container")
        length = struct.unpack_from(">I", source, cursor)[0]
        end = cursor + length + 12
        chunk_type = source[cursor + 4:cursor + 8]
        if end > len(source) or zlib.crc32(source[cursor + 4:end - 4]) != struct.unpack_from(">I", source, end - 4)[0]:
            raise RuntimeError("Invalid PNG chunk")
        if chunk_type[0] < 97 or chunk_type in [b"sRGB", b"gAMA", b"cHRM", b"pHYs", b"tRNS"]:
            output.extend(source[cursor:end])
        cursor = end
        if chunk_type == b"IEND":
            break
    if cursor != len(source):
        raise RuntimeError("Trailing PNG container bytes")
    path.write_bytes(output)


def geometry_receipt(objects):
    """Audit the imported mesh, without changing vertices, normals or indices."""
    rows = []
    for obj in objects:
        if obj.type == "MESH":
            rows.append({
                "vertices": [list(vertex.co) for vertex in obj.data.vertices],
                "polygons": [list(polygon.vertices) for polygon in obj.data.polygons],
                "cornerNormals": [list(normal.vector) for normal in obj.data.corner_normals],
            })
    return hashlib.sha256(json.dumps(rows, separators=(",", ":")).encode()).hexdigest()


def material_values(objects):
    result = []
    for material in {mat for obj in objects if obj.type == "MESH" for mat in obj.data.materials if mat}:
        shader = next(node for node in material.node_tree.nodes if node.type == "BSDF_PRINCIPLED")
        source = material.get("sa2Source", {})
        native_alpha = list(source.get("diffuse", [1, 1, 1, 1]))[3]
        alpha_links = [(link.from_node.name, link.from_socket.name) for link in shader.inputs["Alpha"].links]
        result.append((shader, {name: shader.inputs[name].default_value
                               for name in ["Alpha", "Roughness", "Metallic"]}, native_alpha, alpha_links))
    return result


def render_one(helper, color, output):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.use_denoising = True
    scene.cycles.seed = 0
    scene.render.film_transparent = True
    scene.render.resolution_x = scene.render.resolution_y = 64
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0
    scene.view_settings.gamma = 1
    world = bpy.data.worlds.new("Original emerald soft light")
    scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (.45, .5, .65, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = .5
    wrapper, objects = helper.import_asset("emerald-" + color)
    before_geometry = geometry_receipt(objects)
    original_materials = material_values(objects)
    helper.native_emerald_projection(objects)
    for shader, values, native_alpha, alpha_links in original_materials:
        for name, value in values.items():
            shader.inputs[name].default_value = value
    low, high = helper.bounds(objects)
    center = (low + high) / 2
    extent = max(high - low)
    if extent <= 0:
        raise RuntimeError("Empty original emerald bounds")
    scale = 1 / extent
    wrapper.scale = (scale, scale, scale)
    wrapper.location = -center * scale
    camera_data = bpy.data.cameras.new("Uniform original emerald camera")
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = 1.25
    camera = bpy.data.objects.new("Uniform original emerald camera", camera_data)
    scene.collection.objects.link(camera)
    camera.location = (1.1, -2, .7)
    camera.rotation_euler = (-camera.location).to_track_quat("-Z", "Y").to_euler()
    scene.camera = camera
    helper.light("Soft source key", (1, -1.5, 2), (0, 0, 0), 80, 2)
    helper.light("Soft source fill", (-1.5, -.5, .5), (0, 0, 0), 25, 2)
    scene.render.filepath = str(output)
    bpy.ops.render.render(write_still=True)
    normalize_png_metadata(output)
    after_geometry = geometry_receipt(objects)
    if before_geometry != after_geometry:
        raise RuntimeError("Imported original emerald geometry changed")
    for shader, values, native_alpha, alpha_links in original_materials:
        for name, value in values.items():
            if shader.inputs[name].default_value != value:
                raise RuntimeError("Imported original material value changed")
        if alpha_links != [(link.from_node.name, link.from_socket.name) for link in shader.inputs["Alpha"].links]:
            raise RuntimeError("Imported original alpha wiring changed")
    image = bpy.data.images.load(str(output), check_existing=False)
    rgba = list(image.pixels[:])
    alpha = rgba[3::4]
    if min(alpha) != 0 or max(alpha) <= 0:
        raise RuntimeError("Sprite lacks transparent background or visible gem")
    xs, ys = [], []
    for pixel, value in enumerate(alpha):
        if value > 1 / 255:
            xs.append(pixel % 64)
            ys.append(63 - pixel // 64)
    bbox = [min(xs), min(ys), max(xs) + 1, max(ys) + 1]
    bpy.data.images.remove(image)
    return {
        "color": color,
        "filename": output.name,
        "sha256": sha(output),
        "width": 64,
        "height": 64,
        "sourceGLB": "emerald-" + color + ".glb",
        "sourceGLBSha256": EXPECTED[color],
        "importedMeshCount": sum(obj.type == "MESH" for obj in objects),
        "importedVertices": sum(len(obj.data.vertices) for obj in objects if obj.type == "MESH"),
        "geometryReceiptSha256": before_geometry,
        "geometryUnchanged": before_geometry == after_geometry,
        "uniformScale": scale,
        "alphaBounds": bbox,
        "sourceMaterialAlpha": [native_alpha for _, _, native_alpha, _ in original_materials],
        "sourceAlphaWiringUnchanged": True,
        "sourceAlphaLinkedInShader": [bool(links) for _, _, _, links in original_materials],
        "uniqueVertexPositions": len({tuple(vertex.co) for obj in objects if obj.type == "MESH" for vertex in obj.data.vertices}),
        "alphaRange": [min(alpha), max(alpha)],
        "renderer": "Blender " + bpy.app.version_string + "; Cycles48; transparent orthographic1.25; Standard; seed0",
        "environmentProjection": "Original camera-normal.xy*0.5+0.5 using unchanged embedded native texture",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / ".local/imports")
    parser.add_argument("--metadata", type=Path, default=ROOT / "android/docs/WIDGET-EMERALDS.json")
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
    assets = args.assets.resolve(strict=True)
    output = args.output.resolve()
    if ".local" not in output.parts:
        raise RuntimeError("Native sprites must remain inside ignored .local")
    targets = [output / ("widget-emerald-" + color + ".png") for color in COLORS]
    if args.metadata.exists() or any(path.exists() for path in targets):
        raise RuntimeError("Refusing to overwrite an existing render or receipt")
    for color in COLORS:
        if sha(assets / ("emerald-" + color + ".glb")) != EXPECTED[color]:
            raise RuntimeError("Original GLB fingerprint mismatch: " + color)
    helper_path = ROOT / "scripts/render-brand-assets.py"
    spec = importlib.util.spec_from_file_location("original_brand_renderer", helper_path)
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    helper.ASSETS = assets
    output.mkdir(parents=True, exist_ok=True)
    records = [render_one(helper, color, target) for color, target in zip(COLORS, targets)]
    for color in COLORS:
        if sha(assets / ("emerald-" + color + ".glb")) != EXPECTED[color]:
            raise RuntimeError("Original GLB changed during rendering: " + color)
    receipt = {
        "schemaVersion": 1,
        "kind": "original-geometry-widget-emerald-sprites",
        "source": "Existing verified SA2 e0207 cinematic GLBs; unchanged imported geometry, original materials and textures",
        "presentation": "New static transparent sprites from native geometry; not recovered native UI art",
        "renderTool": "android/tools/render-widget-emeralds.py",
        "renderToolSha256": sha(Path(__file__)),
        "projectionHelper": "scripts/render-brand-assets.py",
        "projectionHelperSha256": sha(helper_path),
        "renderPolicy": "Uniform wrapper scale only; no mesh edits, texture recolor, shape masking or material-alpha override",
        "pngPolicy": "Original encoded IDAT/color-profile chunks retained; volatile Blender Date/RenderTime metadata omitted for reproducible file hashes",
        "sprites": records,
    }
    args.metadata.parent.mkdir(parents=True, exist_ok=True)
    args.metadata.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("WIDGET_EMERALD_SPRITES=" + json.dumps(records))


if __name__ == "__main__":
    main()
