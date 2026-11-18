"""Run with portable Blender --background --factory-startup --python this-file.

Requires the locally prepared official SAIO 2.2.2 extension and native extraction.
Writes game-derived artifacts only under ignored .local/ storage.
"""
import importlib.util
import json
import os
from pathlib import Path
import sys
import zipfile
import struct
import zlib
import time
import re

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
ADDON = ROOT / '.local/tools/saio-2.2.2'
DEPS = ROOT / '.local/tools/saio-python'
DEPS.mkdir(parents=True, exist_ok=True)
for wheel in (ADDON / 'wheels').glob('*.whl'):
    if 'win_amd64' in wheel.name or 'none-any' in wheel.name:
        with zipfile.ZipFile(wheel) as archive:
            archive.extractall(DEPS)
sys.path.insert(0, str(DEPS))
spec = importlib.util.spec_from_file_location(
    'sonic_adventure_io', ADDON / '__init__.py',
    submodule_search_locations=[str(ADDON)],
)
addon = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = addon
spec.loader.exec_module(addon)
addon.register()

EXTRACTED = ROOT / '.local/extracted'
OUTPUT = ROOT / '.local/imports'
OUTPUT.mkdir(parents=True, exist_ok=True)
SPECIAL_NPCS = {'tails', 'knuckles', 'amy'}
SPECIAL_ONLY = '--special-npcs-only' in sys.argv
CHAO_NAMES = {'chao', 'neutral-chaos', 'hero-chaos', 'dark-chaos'} | SPECIAL_NPCS
CHAO_ONLY = SPECIAL_ONLY or '--chao-only' in sys.argv
reports = []
if CHAO_ONLY and (OUTPUT / 'conversion-report.json').exists():
    replaced_names = SPECIAL_NPCS if SPECIAL_ONLY else CHAO_NAMES
    reports = [entry for entry in json.loads((OUTPUT / 'conversion-report.json').read_text())
               if entry['name'] not in replaced_names]
GAME_PATH = Path(os.environ.get('SA2_GAME_PATH') or json.loads((ROOT / '.local/game-inventory.json').read_text())['gamePath'])


def replace_asset(temporary, target):
    # Windows readers/virus scanners can briefly hold a served GLB. Keep the
    # previous complete file available and retry the atomic replace.
    for attempt in range(40):
        try:
            os.replace(temporary, target)
            return
        except PermissionError:
            if attempt == 39:
                raise
            time.sleep(0.05)


def write_png_rows(target, width, height, rows):
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    data = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
    data += chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b'')
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix('.tmp.png')
    temporary.write_bytes(data)
    replace_asset(temporary, target)


def normal_eye_cutout(source):
    # The original normal-eye sprite has a white matte. Remove that matte
    # rather than exposing the entire white eyeball shell as a googly eye.
    image = bpy.data.images.load(str(source), check_existing=True)
    image.colorspace_settings.name = 'Non-Color'
    pixels = list(image.pixels)
    width, height = image.size
    rows = []
    for y in reversed(range(height)):
        row = bytearray([0])
        for x in range(width):
            p = (y * width + x) * 4
            alpha = max(0, min(1, 1 - pixels[p]))
            rgb = [max(0, min(1, (pixels[p + channel] - (1 - alpha)) / alpha)) if alpha > 0.001 else 0 for channel in range(3)]
            row.extend([round(value * 255) for value in rgb] + [round(alpha * 255)])
        rows.append(bytes(row))
    target = EXTRACTED / 'textures/paletted/al_eye01-cutout.png'
    write_png_rows(target, width, height, rows)
    return target


def export_original_icons():
    folder = EXTRACTED / 'textures/al_icon'
    if not folder.exists():
        return
    (OUTPUT / 'emotion-flame.png').write_bytes((folder / 'torch_blue00.png').read_bytes())
    (OUTPUT / 'emotion-sphere.png').write_bytes((folder / 'al_icon01.png').read_bytes())
    frames = []
    for index in range(16):
        image = bpy.data.images.load(str(folder / f'torch_blue{index:02}.png'), check_existing=True)
        image.colorspace_settings.name = 'Non-Color'
        frames.append((image.size[:], list(image.pixels)))
    width, height = frames[0][0]
    rows = []
    for y in reversed(range(height)):
        row = bytearray([0])
        for _, pixels in frames:
            start = y * width * 4
            row.extend(round(max(0, min(1, value)) * 255) for value in pixels[start:start + width * 4])
        rows.append(bytes(row))
    write_png_rows(OUTPUT / 'emotion-flame-atlas.png', width * 16, height, rows)
    (OUTPUT / 'chao-flame.png').write_bytes((OUTPUT / 'emotion-flame-atlas.png').read_bytes())
    (OUTPUT / 'emotion-icon-report.json').write_text(json.dumps({'source': 'al_icon.prs', 'flameFrames': 16, 'frameWidth': width, 'frameHeight': height, 'atlasColumns': 16}, indent=2))


def child_palette_texture(source, bank):
    """Decode the game's original 3x16 RGB565 GVP palette, not a tint.

    SA Tools ChaoPalettes.cs assigns child02/03/04 to banks 0/1/2 of
    al_nc00.GVP. ArchiveTool's fallback CI4 decode stores index*16 in RGB.
    """
    palette = (GAME_PATH / 'resource/gd_PC/al_nc00.GVP').read_bytes()
    if palette[:4] != b'GVPL' or struct.unpack_from('>H', palette, 8)[0] != 1:
        raise RuntimeError('Expected original RGB565 Neutral child palette')
    image = bpy.data.images.load(str(source), check_existing=True)
    image.colorspace_settings.name = 'Non-Color'
    pixels = list(image.pixels)
    width, height = image.size
    rows = []
    for y in reversed(range(height)):
        row = bytearray([0])
        for x in range(width):
            p = (y * width + x) * 4
            index = min(15, max(0, round(pixels[p] * 255 / 16)))
            value = struct.unpack_from('>H', palette, 16 + (bank * 16 + index) * 2)[0]
            r, g, b = value >> 11, (value >> 5) & 63, value & 31
            row.extend(((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2), 255))
        rows.append(bytes(row))
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    target = EXTRACTED / 'textures/paletted' / source.name
    target.parent.mkdir(parents=True, exist_ok=True)
    data = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
    data += chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b'')
    target.write_bytes(data)
    return target


def clean_scene():
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    for mat in list(bpy.data.materials):
        bpy.data.materials.remove(mat)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)


def texture_materials(texture_pack, preserve_eye_whites=False):
    folder = EXTRACTED / 'textures' / texture_pack
    rows = (folder / 'index.txt').read_text().splitlines()
    texture_files = [folder / row.split(',')[1] for row in rows if ',' in row]
    if texture_pack == 'objtex_stg08':
        # This model's local texture0 uses the extracted EMERALD_RED texlist,
        # not texture0 of the entire stage archive (which is an enemy image).
        texlist = (EXTRACTED / 'emerald/emerald-red.satex').read_text()
        names = re.findall(r'TextureNames\[\d+\]=([^\r\n]+)', texlist)
        texture_files = [folder / f'{name}.png' for name in names]
    materials = []
    for mat in bpy.data.materials:
        props = mat.saio_material
        index = int(props.texture_id)
        diffuse = tuple(props.diffuse)
        mat.use_nodes = True
        mat.node_tree.nodes.clear()
        output = mat.node_tree.nodes.new('ShaderNodeOutputMaterial')
        principled = mat.node_tree.nodes.new('ShaderNodeBsdfPrincipled')
        principled.inputs['Base Color'].default_value = diffuse
        principled.inputs['Roughness'].default_value = 0.85
        principled.inputs['Alpha'].default_value = 1
        mat.node_tree.links.new(principled.outputs['BSDF'], output.inputs['Surface'])
        filename = None
        if 0 <= index < len(texture_files) and texture_files[index].exists():
            filename = texture_files[index]
            if texture_pack == 'al_body' and filename.stem in ['al_child02', 'al_child03', 'al_child04']:
                filename = child_palette_texture(filename, ['al_child02', 'al_child03', 'al_child04'].index(filename.stem))
            is_normal_eye = texture_pack == 'al_body' and filename.stem == 'al_eye01'
            eye_cutout = is_normal_eye and not preserve_eye_whites
            if is_normal_eye:
                filename = EXTRACTED / 'textures/al_eye/al_eye01.png'
                if eye_cutout:
                    filename = normal_eye_cutout(filename)
            loaded_image = bpy.data.images.load(str(filename), check_existing=True)
            has_alpha = any(alpha < 0.999 for alpha in list(loaded_image.pixels)[3::4])
            texture = mat.node_tree.nodes.new('ShaderNodeTexImage')
            texture.image = loaded_image
            texture.interpolation = 'Closest'
            mat.node_tree.links.new(texture.outputs['Color'], principled.inputs['Base Color'])
            # Legacy body texture alpha can encode native effects rather than
            # holes. Only the explicit pupil cutout may mask Chao geometry.
            allow_alpha = eye_cutout if texture_pack == 'al_body' else props.use_alpha
            if allow_alpha and has_alpha:
                if texture_pack == 'al_body':
                    cutoff = mat.node_tree.nodes.new('ShaderNodeMath')
                    cutoff.operation = 'GREATER_THAN'
                    cutoff.inputs[1].default_value = 0.5
                    mat.node_tree.links.new(texture.outputs['Alpha'], cutoff.inputs[0])
                    mat.node_tree.links.new(cutoff.outputs[0], principled.inputs['Alpha'])
                else:
                    mat.node_tree.links.new(texture.outputs['Alpha'], principled.inputs['Alpha'])
        mat.diffuse_color = diffuse
        materials.append({'name': mat.name, 'textureIndex': index, 'image': filename.name if filename else None})
    return materials


def export_scene(name, source, texture_pack):
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    open_eye_overlays = []
    if texture_pack == 'al_body':
        # The common 40-node Chao skeleton has the body-textured eyelid
        # overlay meshes at nodes19/22, adjacent to eyeballs18/21. Keep the
        # eye geometry and animation bones, omit the default half-closed lids.
        for obj in list(bpy.context.scene.objects):
            if obj.type == 'MESH' and obj.parent_bone[:4] in ('019_', '022_'):
                open_eye_overlays.append(obj.name)
                bpy.data.objects.remove(obj, do_unlink=True)
    materials = texture_materials(texture_pack, preserve_eye_whites=name in SPECIAL_NPCS)
    if texture_pack == 'al_body':
        # ChaoMouth enum starts at ToothySmile=1; the neutral default0 does
        # not draw either of the alternative open/toothy mouth polygons.
        for obj in list(bpy.context.scene.objects):
            if obj.type == 'MESH' and obj.data.polygons:
                remove_faces = [polygon.index for polygon in obj.data.polygons
                    if polygon.material_index < len(obj.data.materials)
                    and obj.data.materials[polygon.material_index].saio_material.texture_id in (38, 39)]
                if remove_faces:
                    import bmesh
                    mesh = bmesh.new()
                    mesh.from_mesh(obj.data)
                    mesh.faces.ensure_lookup_table()
                    bmesh.ops.delete(mesh, geom=[mesh.faces[i] for i in remove_faces], context='FACES')
                    mesh.to_mesh(obj.data)
                    mesh.free()
    bpy.context.view_layer.update()
    mesh_objects = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    if not mesh_objects:
        raise RuntimeError(f'No geometry imported: {source}')
    bounds = [obj.matrix_world @ Vector(corner) for obj in mesh_objects for corner in obj.bound_box]
    minimum = [min(c[i] for c in bounds) for i in range(3)]
    maximum = [max(c[i] for c in bounds) for i in range(3)]
    animation_feet = {}
    armature = next((obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE'), None)
    if armature and armature.animation_data:
        previous_action = armature.animation_data.action
        previous_slot = armature.animation_data.action_slot
        previous_frame = bpy.context.scene.frame_current
        for action in list(bpy.data.actions):
            armature.animation_data.action = action
            if action.slots:
                armature.animation_data.action_slot = action.slots[0]
            sampled = []
            for frame in [action.frame_range[0], sum(action.frame_range) / 2, action.frame_range[1]]:
                bpy.context.scene.frame_set(round(frame))
                bpy.context.view_layer.update()
                depsgraph = bpy.context.evaluated_depsgraph_get()
                evaluated = [obj.evaluated_get(depsgraph) for obj in mesh_objects]
                sampled.append(min((obj.matrix_world @ Vector(corner)).z for obj in evaluated for corner in obj.bound_box))
            animation_feet[action.name] = {'minY': min(sampled), 'maxY': max(sampled)}
        armature.animation_data.action = previous_action
        if previous_action and previous_slot:
            armature.animation_data.action_slot = previous_slot
        bpy.context.scene.frame_set(previous_frame)
        bpy.context.view_layer.update()
    target = OUTPUT / f'{name}.glb'
    temporary = OUTPUT / f'{name}.converting.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', export_animations=True, use_active_scene=True, export_vertex_color='ACTIVE')
    if texture_pack != 'al_body':
        # Legacy garden textures/vertex colors contain their lighting. A
        # standard unlit glTF baseline preserves them across modern engines.
        data = temporary.read_bytes()
        json_size = struct.unpack_from('<I', data, 12)[0]
        document = json.loads(data[20:20 + json_size])
        document.setdefault('extensionsUsed', []).append('KHR_materials_unlit')
        for material in document.get('materials', []):
            material.setdefault('extensions', {})['KHR_materials_unlit'] = {}
        encoded = json.dumps(document, separators=(',', ':')).encode()
        encoded += b' ' * (-len(encoded) % 4)
        binary_chunk = data[20 + json_size:]
        temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk))
            + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    replace_asset(temporary, target)
    reports.append({
        'name': name, 'source': str(source.relative_to(ROOT)), 'output': str(target.relative_to(ROOT)),
        'meshCount': len(mesh_objects), 'vertices': sum(len(o.data.vertices) for o in mesh_objects),
        'blenderBounds': {'min': minimum, 'max': maximum},
        'meshBounds': [{'name': obj.name, 'min': [min((obj.matrix_world @ Vector(c))[i] for c in obj.bound_box) for i in range(3)],
                        'max': [max((obj.matrix_world @ Vector(c))[i] for c in obj.bound_box) for i in range(3)]} for obj in mesh_objects],
        'animations': [action.name for action in bpy.data.actions],
        'openEyeOverlaysRemoved': open_eye_overlays,
        'originalEyeWhitesPreserved': name in SPECIAL_NPCS,
        'animationFootMinY': animation_feet,
        'materials': materials, 'bytes': target.stat().st_size,
    })


def render_chao_preview(name):
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12
    scene.render.resolution_x = 640
    scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.world.color = (0.3, 0.3, 0.3)
    camera_data = bpy.data.cameras.new('Preview Camera')
    camera = bpy.data.objects.new('Preview Camera', camera_data)
    scene.collection.objects.link(camera)
    camera.location = (7, -13, 6)
    camera.rotation_euler = (Vector((0, 0, 2.4)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera_data.type = 'ORTHO'
    camera_data.ortho_scale = 6.8
    scene.camera = camera
    lamp_data = bpy.data.lights.new('Preview Light', 'AREA')
    lamp_data.energy = 1200
    lamp_data.shape = 'DISK'
    lamp_data.size = 8
    lamp = bpy.data.objects.new('Preview Light', lamp_data)
    scene.collection.objects.link(lamp)
    lamp.location = (3, -7, 9)
    lamp.rotation_euler = (Vector((0, 0, 2)) - lamp.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.image_settings.file_format = 'PNG'
    scene.render.filepath = str(OUTPUT / f'{name}-preview.png')
    bpy.ops.render.render(write_still=True)



def main():
    export_original_icons()
    for output_name, native_name in [
        ('chao', 'al_ncz'), ('neutral-chaos', 'al_nchaos'), ('hero-chaos', 'al_hchaos'),
        ('dark-chaos', 'al_dchaos'), ('tails', 'al_tails'), ('knuckles', 'al_knuckles'), ('amy', 'al_amy'),
    ]:
        if SPECIAL_ONLY and output_name not in SPECIAL_NPCS:
            continue
        clean_scene()
        source = EXTRACTED / f'pc-chao/Chao/models/AL_RootObject/{native_name}.sa2mdl'
        result = bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}], import_as_armature=True)
        if result != {'FINISHED'}:
            raise RuntimeError(f'Chao import failed: {result}')
        armatures = [obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE']
        if armatures:
            bpy.context.view_layer.objects.active = armatures[0]
            bpy.ops.object.select_all(action='DESELECT')
            armatures[0].select_set(True)
            animation_dir = EXTRACTED / 'pc-chao/Chao/models/al_motion_table'
            for filename, name in [('alm_tati.saanim', 'Idle'), ('alm_aruki_ncz.saanim', 'Walk')]:
                animation = animation_dir / filename
                bpy.ops.saio.import_node_anim(filepath=str(animation), files=[{'name': filename}], short_rot=True)
                if armatures[0].animation_data and armatures[0].animation_data.action:
                    armatures[0].animation_data.action.name = name
                    armatures[0].animation_data.action.use_fake_user = True
        export_scene(output_name, source, 'al_body')
        if '--preview-all' in sys.argv or (output_name == ('knuckles' if '--preview-knuckles' in sys.argv else 'chao') and ('--preview' in sys.argv or '--preview-knuckles' in sys.argv)):
            render_chao_preview(output_name)

    for output_name, folder, native_name, texture_pack in [
        ('garden', 'stg_neut', 'objLandTableNeut', 'al_stg_neut_tex'),
        ('hero-garden', 'stg_hero', 'objLandTableHero', 'al_stg_hero_tex'),
        ('dark-garden', 'stg_dark', 'objLandTableDark', 'al_stg_dark_tex'),
    ]:
        if CHAO_ONLY:
            continue
        clean_scene()
        source = EXTRACTED / f'pc-chao/Chao/{folder}/{native_name}.sa2blvl'
        result = bpy.ops.saio.import_lvl(filepath=str(source), files=[{'name': source.name}])
        if result != {'FINISHED'}:
            raise RuntimeError(f'Garden import failed: {result}')
        # Keep visual geometry; imported collision-only meshes are separate.
        for obj in list(bpy.context.scene.objects):
            if obj.type == 'MESH' and not obj.saio_land_entry.is_visual:
                bpy.data.objects.remove(obj, do_unlink=True)
        export_scene(output_name, source, texture_pack)
    emerald = EXTRACTED / 'emerald/chaos-emerald.sa2mdl'
    if emerald.exists() and not CHAO_ONLY:
        clean_scene()
        result = bpy.ops.saio.import_mdl(filepath=str(emerald), files=[{'name': emerald.name}])
        if result != {'FINISHED'}:
            raise RuntimeError(f'Chaos Emerald import failed: {result}')
        export_scene('chaos-emerald', emerald, 'objtex_stg08')
    (OUTPUT / 'conversion-report.json').write_text(json.dumps(reports, indent=2) + '\n')
    print('SA2_BROWSER_CONVERSION=' + json.dumps([{key: value for key, value in report.items() if key not in ['materials', 'meshBounds']} for report in reports]))


if __name__ == "__main__":
    main()
