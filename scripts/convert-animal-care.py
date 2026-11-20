"""Convert original Chao animal parts to GLB for attachment to the Chao rig.

Run portable Blender --background --factory-startup --python this-file after
scripts/extract-animal-care.js. Optional: -- --render NAME... renders checks.
Also exports the seven tree kinds (young/mature/dying) and the 24 fruits as
standalone Y-up GLBs (tree-<kind>-<stage>.glb, fruit-<key>.glb).
All game-derived output stays in ignored .local/ storage.

Each part GLB mirrors the converted Chao (scripts/convert-assets.py): its one
scene root is a handedness node rotated -90 degrees about X with no translation,
like the Chao's root joint 000, and the root's children are in the same Blender
Z-up joint frame as every Chao joint below 000. A client that drops the root and
parents its children to Chao joint N at identity reproduces the native draw:
the joint matrix, then the part object's own translate/rotate/scale
(ChaoMain al_draw_B -> fn_60_5113C on GSNE8P).
"""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('animal_care_helpers', ROOT / 'scripts/convert-cinematic-assets.py')
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)

CATALOG = ROOT / '.local/care/animals.json'
OUTPUT = ROOT / '.local/imports/care'
OUTPUT.mkdir(parents=True, exist_ok=True)
TEXTURE_PACK = 'al_mini_parts_tex'
HANDEDNESS = [-math.sqrt(0.5), 0, 0, math.sqrt(0.5)]
# Part kind -> Chao skeleton node index, from GSNE8P ChaoMain .data 0x43EE0
# (read by fn_60_23148, the part setter); list slot order from .data 0x43F20.
CHAO_NODE = {'leftArm': 3, 'rightArm': 10, 'leftEar': 24, 'rightEar': 26, 'forehead': 29, 'leftHorn': 30,
             'rightHorn': 31, 'leftLeg': 6, 'rightLeg': 13, 'tail': 8, 'leftWing': 37, 'rightWing': 39, 'tongue': 28}
CATEGORY = {'leftArm': 'arms', 'rightArm': 'arms', 'leftEar': 'ears', 'rightEar': 'ears', 'forehead': 'forehead',
            'leftHorn': 'horns', 'rightHorn': 'horns', 'leftLeg': 'legs', 'rightLeg': 'legs', 'tail': 'tail',
            'leftWing': 'wings', 'rightWing': 'wings', 'tongue': 'tongue'}


def read_glb(path):
    data = path.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    return json.loads(data[20:20 + json_size]), data[20 + json_size:]


def write_glb(path, document, binary_chunk):
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    path.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk))
                     + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)


def convert_part(animal, age, slot, model):
    native = ROOT / model['path']
    name = f"animal-part-{animal['key']}-{age}-{slot}"
    helpers.clear_scene()
    result = bpy.ops.saio.import_mdl(filepath=str(native), files=[{'name': native.name}], import_as_armature=True)
    if result != {'FINISHED'}:
        raise RuntimeError(f'Failed original model import: {native}')
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    originals = helpers.prepare_materials(TEXTURE_PACK)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    if not meshes:
        raise RuntimeError(f'No geometry imported: {native}')
    color_layers = sorted({layer.name for obj in meshes for layer in obj.data.color_attributes})
    temporary = OUTPUT / f'{name}.converting.glb'
    target = OUTPUT / f'{name}.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', use_active_scene=True,
                              export_animations=False, export_vertex_color='ACTIVE')
    document, binary_chunk = read_glb(temporary)
    nodes = document['nodes']
    scene = document['scenes'][document.get('scene', 0)]
    if len(scene['nodes']) != 1:
        raise RuntimeError(f'Expected one armature root: {native}')
    armature = nodes[scene['nodes'][0]]
    joints = armature.get('children', [])
    if len(joints) != 1:
        raise RuntimeError(f'Expected one export wrapper joint: {native}')
    wrapper = nodes[joints[0]]
    # The exported wrapper rotation is the handedness rotation times the
    # wrapper's own native rotation; keep only the handedness part.
    from mathutils import Quaternion
    x, y, z, w = wrapper.get('rotation', [0, 0, 0, 1])
    hx, hy, hz, hw = HANDEDNESS
    dropped = Quaternion((hw, hx, hy, hz)).inverted() @ Quaternion((w, x, y, z))
    wrapper_rotation = [dropped.x, dropped.y, dropped.z, dropped.w]
    wrapper_translation = wrapper.pop('translation', [0, 0, 0])
    wrapper.pop('scale', None)
    wrapper['rotation'] = HANDEDNESS
    wrapper['name'] = 'handedness'
    # The wrapper is SA Tools' export root (no geometry): only its child is the
    # object the native part list points to, so the wrapper's offset is dropped.
    if wrapper.get('mesh') is not None:
        raise RuntimeError(f'Export wrapper unexpectedly has geometry: {native}')
    armature['children'] = []
    scene['nodes'] = [joints[0]]
    for node in nodes:
        node.pop('skin', None)
    document.pop('skins', None)
    part_children = [nodes[index] for index in wrapper.get('children', [])]
    lookup = {mat['name']: mat for mat in originals}
    for mat in document.get('materials', []):
        original = lookup[mat['name']]
        mat.setdefault('extras', {})['sa2Source'] = original
        mat['doubleSided'] = original['doubleSided']
        pbr = mat.setdefault('pbrMetallicRoughness', {})
        pbr['baseColorFactor'] = original['diffuse'] if original['useAlpha'] else original['diffuse'][:3] + [1]
        pbr['metallicFactor'] = 0
        pbr['roughnessFactor'] = 0.9
        if original['useAlpha']:
            mat['alphaMode'] = 'BLEND' if original['diffuse'][3] < 0.999 else 'MASK'
            if mat['alphaMode'] == 'MASK':
                mat['alphaCutoff'] = 0.5
        else:
            mat['alphaMode'] = 'OPAQUE'
    attach = {'chaoNode': CHAO_NODE[slot], 'category': CATEGORY[slot], 'slot': slot,
              'frame': 'Children of the root are in the Chao joint frame (Blender Z-up, as below Chao joint 000). Drop the root and parent them to Chao node chaoNode at identity.'}
    document['asset'].setdefault('extras', {})['sa2Source'] = {
        'model': model['path'], 'modelName': model['name'], 'texturePack': TEXTURE_PACK,
        'nativeObjectAddress': model['nativeObjectAddress'], 'exportRootAddress': model['rootAddress'],
        'droppedExportRootTranslation': wrapper_translation, 'droppedExportRootRotation': wrapper_rotation, 'species': animal['key'], 'age': age, 'attach': attach}
    write_glb(temporary, document, binary_chunk)
    helpers.replace_asset(temporary, target)
    return {'name': name, 'species': animal['key'], 'animalId': animal['id'], 'age': age, 'slot': slot,
            'category': CATEGORY[slot], 'chaoNode': CHAO_NODE[slot], 'nativeModel': model['path'],
            'nativeModelSha256': hashlib.sha256(native.read_bytes()).hexdigest(),
            'nativeObjectAddress': model['nativeObjectAddress'], 'output': str(target.relative_to(ROOT)).replace('\\', '/'),
            'bytes': target.stat().st_size, 'meshCount': len(meshes), 'vertices': sum(len(obj.data.vertices) for obj in meshes),
            'vertexColorLayers': color_layers, 'partRootChildren': [node.get('name') for node in part_children],
            'partRootTranslation': [node.get('translation', [0, 0, 0]) for node in part_children],
            'materials': [{'name': mat['name'], 'texture': mat.get('texture'), 'textureIndex': mat['textureIndex'],
                           'useAlpha': mat['useAlpha'], 'doubleSided': mat['doubleSided']} for mat in originals]}


def main():
    catalog = json.loads(CATALOG.read_text())
    records, skipped = [], []
    for animal in catalog['animals']:
        for age in ('child', 'adult'):
            parts = animal['parts'][age]
            if not parts['mapped']:
                raise RuntimeError(f"Unresolved part list: {animal['key']} {age}")
            for slot in parts['slots']:
                if slot['model'] is None:
                    if slot['pointer'] is not None:
                        raise RuntimeError(f"Unmapped native part: {animal['key']} {age} {slot['slot']}")
                    continue
                records.append(convert_part(animal, age, slot['slot'], slot['model']))
                print('SA2_ANIMAL_PART=' + json.dumps({key: records[-1][key] for key in ('name', 'vertices', 'chaoNode')}))
            if parts['status'] == 'zero-initialized-empty-list':
                skipped.append({'species': animal['key'], 'age': age, 'reason': 'Native part list is all null (no parts).'})
    report = {'version': 1, 'texturePack': TEXTURE_PACK, 'count': len(records), 'chaoNodes': CHAO_NODE,
              'handednessRoot': {'rotation': HANDEDNESS, 'note': 'Same as converted Chao root joint 000.'},
              'noParts': skipped, 'parts': records}
    (OUTPUT / 'animal-parts-manifest.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'count': len(records), 'noParts': len(skipped)}))
    helpers.OUTPUT = OUTPUT
    garden = []
    for tree in catalog['treeModels']:
        for stage, model in tree['stages'].items():
            record = helpers.export_native(f"tree-{tree['key']}-{stage}", ROOT / model['path'], 'al_object')
            garden.append({'kind': 'tree', 'treeKind': tree['kind'], 'key': tree['key'], 'stage': stage, 'fruitId': tree['fruitId'],
                           'sourceModel': model['name'], 'nativeAddress': model['address'], **record})
    for fruit in catalog['fruitModels']:
        record = helpers.export_native(f"fruit-{fruit['key']}", ROOT / fruit['model']['path'], 'al_object')
        garden.append({'kind': 'fruit', 'fruitId': fruit['id'], 'key': fruit['key'], 'sourceModel': fruit['model']['name'],
                       'nativeAddress': fruit['model']['address'], **record})
    (OUTPUT / 'garden-models-manifest.json').write_text(json.dumps({'version': 1, 'texturePack': 'al_object', 'models': garden}, indent=2) + '\n')
    print(json.dumps({'garden': len(garden)}))
    if '--render' in sys.argv:
        for name in sys.argv[sys.argv.index('--render') + 1:]:
            helpers.clear_scene()
            bpy.ops.import_scene.gltf(filepath=str(OUTPUT / f'{name}.glb'))
            meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
            bounds = [obj.matrix_world @ Vector(c) for obj in meshes for c in obj.bound_box]
            helpers.OUTPUT = OUTPUT
            helpers.render_preview(name, [min(v[i] for v in bounds) for i in range(3)], [max(v[i] for v in bounds) for i in range(3)])


main()
