"""Convert proven SA2 cutscene emeralds and original garden props locally.

Use portable Blender --background --factory-startup --python this-file.
Requires scripts/extract-cinematic-assets.js and prepared SAIO dependencies.
"""
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import zipfile

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
spec = importlib.util.spec_from_file_location('sonic_adventure_io', ADDON / '__init__.py', submodule_search_locations=[str(ADDON)])
addon = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = addon
spec.loader.exec_module(addon)
addon.register()

EVENT = ROOT / '.local/extracted/cutscene-e0207/e0207'
OUTPUT = ROOT / '.local/imports'
TEXTURES = ROOT / '.local/extracted/textures'
OUTPUT.mkdir(parents=True, exist_ok=True)
COLOR_IDS = {2: 'blue', 3: 'green', 4: 'cyan', 5: 'purple', 6: 'red', 7: 'white', 8: 'yellow'}


def clear_scene():
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)


def source_materials():
    return [{'name': mat.name, 'textureIndex': int(mat.saio_material.texture_id),
             'diffuse': list(mat.saio_material.diffuse), 'specular': list(mat.saio_material.specular),
             'specularExponent': mat.saio_material.specular_exponent,
             'useAlpha': mat.saio_material.use_alpha, 'doubleSided': mat.saio_material.double_sided,
             'sourceAlpha': mat.saio_material.source_alpha, 'destinationAlpha': mat.saio_material.destination_alpha,
             'environment': mat.saio_material.use_environment,
             'ignoreLighting': mat.saio_material.ignore_diffuse,
             'ignoreSpecular': mat.saio_material.ignore_specular}
            for mat in bpy.data.materials]


def inspect_event():
    audit = []
    for source in sorted(EVENT.rglob('*.sa2mdl')):
        clear_scene()
        bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}])
        mats = source_materials()
        texture_ids = {mat['textureIndex'] for mat in mats}
        meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH' and not obj.name.startswith('SAIO Bone Shape')]
        emerald = bool(texture_ids & set(COLOR_IDS))
        record = {'source': str(source.relative_to(ROOT)), 'meshCount': len(meshes),
                  'vertices': sum(len(obj.data.vertices) for obj in meshes),
                  'textures': sorted(texture_ids), 'emeraldCandidate': emerald, 'materials': mats}
        audit.append(record)
        if emerald:
            print('CINEMATIC_EMERALD_CANDIDATE=' + json.dumps(record))
    (OUTPUT / 'cutscene-model-audit.json').write_text(json.dumps(audit, indent=2) + '\n')


def prepare_materials(pack, cinematic=False):
    folder = TEXTURES / pack
    if cinematic:
        import re
        names = re.findall(r'TextureNames\[\d+\]=([^\r\n]+)', (EVENT / 'InternalTexlist.satex').read_text())
        files = [folder / f'{name}.png' for name in names]
    else:
        files = [folder / row.split(',')[1] for row in (folder / 'index.txt').read_text().splitlines() if ',' in row]
    source = source_materials()
    for mat, original in zip(bpy.data.materials, source):
        mat.use_nodes = True
        mat.use_backface_culling = not original['doubleSided']
        mat.node_tree.nodes.clear()
        out = mat.node_tree.nodes.new('ShaderNodeOutputMaterial')
        shader = mat.node_tree.nodes.new('ShaderNodeBsdfPrincipled')
        shader.inputs['Base Color'].default_value = original['diffuse']
        shader.inputs['Alpha'].default_value = original['diffuse'][3] if original['useAlpha'] else 1
        # Preserve native specular strength/exponent in a standard PBR form.
        shader.inputs['Specular IOR Level'].default_value = 0 if original['ignoreSpecular'] else 1
        shader.inputs['Roughness'].default_value = math.sqrt(2 / (original['specularExponent'] + 2)) if cinematic else 0.9
        mat.node_tree.links.new(shader.outputs['BSDF'], out.inputs['Surface'])
        index = original['textureIndex']
        if index < 0 or index >= len(files):
            raise RuntimeError(f'Unmapped source texture {index}: {mat.name}')
        image = bpy.data.images.load(str(files[index]), check_existing=True)
        texture = mat.node_tree.nodes.new('ShaderNodeTexImage')
        texture.image = image
        texture.interpolation = 'Linear'
        if cinematic and original['environment']:
            # Exact SAIO UV Tiling template path: world normal -> camera
            # normal -> normal.xy * .5 + .5. Original event UVs are all zero.
            geometry = mat.node_tree.nodes.new('ShaderNodeNewGeometry')
            camera_normal = mat.node_tree.nodes.new('ShaderNodeVectorTransform')
            camera_normal.vector_type = 'NORMAL'
            camera_normal.convert_from = 'WORLD'
            camera_normal.convert_to = 'CAMERA'
            projection = mat.node_tree.nodes.new('ShaderNodeVectorMath')
            projection.operation = 'MULTIPLY_ADD'
            projection.inputs[1].default_value = (0.5, 0.5, 0)
            projection.inputs[2].default_value = (0.5, 0.5, 0)
            mat.node_tree.links.new(geometry.outputs['Normal'], camera_normal.inputs['Vector'])
            mat.node_tree.links.new(camera_normal.outputs['Vector'], projection.inputs[0])
            mat.node_tree.links.new(projection.outputs['Vector'], texture.inputs['Vector'])
            original['environmentProjection'] = 'cameraSpaceNormal.xy * 0.5 + 0.5; original UVs are zero'
        mat.node_tree.links.new(texture.outputs['Color'], shader.inputs['Base Color'])
        if original['useAlpha'] and not cinematic:
            mat.node_tree.links.new(texture.outputs['Alpha'], shader.inputs['Alpha'])
        original['texture'] = str(files[index].relative_to(ROOT))
    return source


def replace_asset(temporary, target):
    for attempt in range(40):
        try:
            os.replace(temporary, target)
            return
        except PermissionError:
            if attempt == 39:
                raise
            time.sleep(0.05)


def export_native(name, native, pack, cinematic=False):
    clear_scene()
    result = bpy.ops.saio.import_mdl(filepath=str(native), files=[{'name': native.name}])
    if result != {'FINISHED'}:
        raise RuntimeError(f'Failed original model import: {native}')
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    originals = prepare_materials(pack, cinematic)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    bpy.context.view_layer.update()
    bounds = [obj.matrix_world @ Vector(corner) for obj in meshes for corner in obj.bound_box]
    minimum = [min(c[i] for c in bounds) for i in range(3)]
    maximum = [max(c[i] for c in bounds) for i in range(3)]
    target = OUTPUT / f'{name}.glb'
    temporary = OUTPUT / f'{name}.converting.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', use_active_scene=True, export_animations=False, export_vertex_color='ACTIVE')
    data = temporary.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    lookup = {mat['name']: mat for mat in originals}
    for mat in document.get('materials', []):
        original = lookup[mat['name']]
        mat.setdefault('extras', {})['sa2Source'] = original
        mat['doubleSided'] = original['doubleSided']
        mat.setdefault('pbrMetallicRoughness', {})['baseColorFactor'] = original['diffuse'] if original['useAlpha'] else original['diffuse'][:3] + [1]
        mat['pbrMetallicRoughness']['metallicFactor'] = 0
        mat['pbrMetallicRoughness']['roughnessFactor'] = math.sqrt(2 / (original['specularExponent'] + 2)) if cinematic else 0.9
        if original['useAlpha']:
            mat['alphaMode'] = 'BLEND' if cinematic or original['diffuse'][3] < 0.999 else 'MASK'
            if mat['alphaMode'] == 'MASK':
                mat['alphaCutoff'] = 0.5
        else:
            mat['alphaMode'] = 'OPAQUE'
    document['asset'].setdefault('extras', {})['sa2Source'] = {'model': str(native.relative_to(ROOT)), 'cinematic': cinematic, 'texturePack': pack, 'nativeEnvironmentMapping': cinematic}
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary_chunk = data[20 + json_size:]
    temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk)) + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    replace_asset(temporary, target)
    record = {'name': name, 'nativeModel': str(native.relative_to(ROOT)), 'output': str(target.relative_to(ROOT)), 'cinematic': cinematic, 'meshCount': len(meshes), 'vertices': sum(len(obj.data.vertices) for obj in meshes), 'blenderBounds': {'min': minimum, 'max': maximum}, 'materials': originals, 'bytes': target.stat().st_size}
    print('SA2_CINEMATIC_EXPORT=' + json.dumps(record))
    if '--preview' in sys.argv:
        render_preview(name, minimum, maximum)
    return record


def render_preview(name, minimum, maximum):
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 24
    scene.render.resolution_x = 512
    scene.render.resolution_y = 512
    scene.world.color = (0.15, 0.15, 0.15)
    center = Vector([(a + b) / 2 for a, b in zip(minimum, maximum)])
    size = max(b - a for a, b in zip(minimum, maximum))
    camera_data = bpy.data.cameras.new('Preview Camera')
    camera = bpy.data.objects.new('Preview Camera', camera_data)
    scene.collection.objects.link(camera)
    camera.location = center + Vector((size * 1.1, -size * 2, size * 0.7))
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera_data.type = 'ORTHO'
    camera_data.ortho_scale = size * 1.25
    scene.camera = camera
    light_data = bpy.data.lights.new('Preview Light', 'AREA')
    light_data.energy = max(1200, size * size * 60)
    light_data.size = size
    light = bpy.data.objects.new('Preview Light', light_data)
    scene.collection.objects.link(light)
    light.location = center + Vector((size, -size * 1.5, size * 2))
    light.rotation_euler = (center - light.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.image_settings.file_format = 'PNG'
    scene.render.filepath = str(OUTPUT / f'{name}-preview.png')
    bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    if '--audit' in sys.argv:
        inspect_event()
    else:
        records = []
        for texture_id, color in COLOR_IDS.items():
            records.append(export_native(f'emerald-{color}', EVENT / f'Scene 2/Entity {texture_id} Model.sa2mdl', 'e0207texture', cinematic=True))
        tree = ROOT / '.local/extracted/native-props/tree-neutral.sa2mdl'
        if tree.exists():
            records.append(export_native('tree-neutral', tree, 'al_neut_obj_tex'))
        (OUTPUT / 'cinematic-conversion-report.json').write_text(json.dumps(records, indent=2) + '\n')
