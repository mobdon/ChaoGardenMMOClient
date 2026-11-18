"""Export separate, centered original SA2 shoes/gloves for Chao attachment.

Run Blender --background --factory-startup --python this-file after
scripts/extract-upgrade-assets.js. All game-derived output stays in .local/.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('upgrade_asset_helpers', ROOT / 'scripts/convert-cinematic-assets.py')
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
OUTPUT = ROOT / '.local/imports'
MANIFEST = ROOT / '.local/extracted/upgrades/manifest.json'


def convert(asset):
    helpers.clear_scene()
    native = ROOT / asset['nativeModel']
    if hashlib.sha256(native.read_bytes()).hexdigest() != asset['nativeSha256']:
        raise RuntimeError(f'Native intermediate differs from extraction record: {native}')
    base_meshes = []
    mounting_reference = None
    if 'baseBoot' in asset:
        base = asset['baseBoot']
        base_file = ROOT / base['nativeModel']
        if hashlib.sha256(base_file.read_bytes()).hexdigest() != base['nativeSha256']:
            raise RuntimeError('Native Shadow body differs from extraction record')
        bpy.ops.saio.import_mdl(filepath=str(base_file), files=[{'name': base_file.name}], import_as_armature=True)
        bpy.context.view_layer.update()
        armature = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
        foot = next(bone for bone in armature.pose.bones if int(bone.name.split('_')[0]) == base['footNode'])
        toe = next(bone for bone in armature.pose.bones if int(bone.name.split('_')[0]) == base['footNode'] + 2)
        heel_matrix = armature.matrix_world @ foot.matrix
        blender_to_gltf = Matrix(((1, 0, 0), (0, 0, 1), (0, -1, 0)))
        local_rotation = heel_matrix.to_quaternion().to_matrix().inverted()
        mounting_reference = {
            'nativeHeelRestWorldBlenderMatrix': [list(row) for row in heel_matrix],
            'nativeHeelToToeWorldBlender': list((armature.matrix_world @ toe.matrix).translation - heel_matrix.translation),
            'exportedPropStandingUp': list(blender_to_gltf @ local_rotation @ Vector((0, 0, 1))),
            'exportedPropNominalForward': list(blender_to_gltf @ local_rotation @ Vector((-1, 0, 0))),
            'reference': 'Restore these exported local up/forward directions to Chao world +Y/+Z. Standing up is exact native world up; nominal Shadow forward is -BlenderX, checked against native heel-to-toe direction. Uniform scale only.',
        }
        original_boot = next(obj for obj in bpy.context.scene.objects if obj.type == 'MESH' and any(group.name == foot.name for group in obj.vertex_groups))
        depsgraph = bpy.context.evaluated_depsgraph_get()
        evaluated = original_boot.evaluated_get(depsgraph)
        mesh = bpy.data.meshes.new_from_object(evaluated, preserve_all_data_layers=True, depsgraph=depsgraph)
        # Bring native heel/toe geometry into the same foot-local space as
        # its standalone upgrade overlay, retaining all native proportions.
        mesh.transform((armature.matrix_world @ foot.matrix).inverted() @ original_boot.matrix_world)
        boot = bpy.data.objects.new(f"{asset['name']}-native-jetboot", mesh)
        bpy.context.scene.collection.objects.link(boot)
        base_meshes.append(boot)
        for obj in list(bpy.context.scene.objects):
            if obj != boot:
                bpy.data.objects.remove(obj, do_unlink=True)
    result = bpy.ops.saio.import_mdl(filepath=str(native), files=[{'name': native.name}])
    if result != {'FINISHED'}:
        raise RuntimeError(f'Failed to import original wearable: {native}')
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    used_materials = {material for obj in bpy.context.scene.objects if obj.type == 'MESH' for material in obj.data.materials}
    for material in list(bpy.data.materials):
        if material not in used_materials:
            bpy.data.materials.remove(material, do_unlink=True)
    originals = helpers.prepare_materials(asset['textures'])
    bpy.context.view_layer.update()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    source_meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    if not source_meshes:
        raise RuntimeError(f'No original geometry: {native}')
    # Bake only each prop's native rest transform. The playable Chao skeleton
    # is never loaded or modified by this conversion.
    baked = []
    for source in source_meshes:
        evaluated = source.evaluated_get(depsgraph)
        mesh = bpy.data.meshes.new_from_object(evaluated, preserve_all_data_layers=True, depsgraph=depsgraph)
        mesh.transform(source.matrix_world)
        obj = bpy.data.objects.new(f"{asset['name']}-{len(baked)}", mesh)
        bpy.context.scene.collection.objects.link(obj)
        baked.append(obj)
    for obj in list(bpy.context.scene.objects):
        if obj not in baked:
            bpy.data.objects.remove(obj, do_unlink=True)
    points = [vertex.co for obj in baked for vertex in obj.data.vertices]
    minimum = Vector(tuple(min(point[i] for point in points) for i in range(3)))
    maximum = Vector(tuple(max(point[i] for point in points) for i in range(3)))
    center = (minimum + maximum) / 2
    for obj in baked:
        obj.data.transform(Matrix.Translation(-center))
    bpy.context.view_layer.update()
    target = OUTPUT / f"{asset['name']}.glb"
    temporary = OUTPUT / f"{asset['name']}.converting.glb"
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', use_active_scene=True,
                              export_animations=False, export_vertex_color='ACTIVE')
    data = temporary.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    lookup = {material['name']: material for material in originals}
    for material in document.get('materials', []):
        original = lookup[material['name']]
        material.setdefault('extras', {})['sa2Source'] = original
        material['doubleSided'] = original['doubleSided']
        material['alphaMode'] = 'BLEND' if original['useAlpha'] else 'OPAQUE'
        pbr = material.setdefault('pbrMetallicRoughness', {})
        pbr['baseColorFactor'] = original['diffuse'] if original['useAlpha'] else original['diffuse'][:3] + [1]
        pbr['metallicFactor'] = 0
    metadata = {
        'itemId': asset['itemId'], 'slot': asset['slot'], 'side': asset['side'],
        'modelArchive': f"resource/gd_PC/{asset['archive']}.prs", 'modelId': asset['modelId'],
        'textureArchive': f"resource/gd_PC/{asset['textures']}.prs", 'nativeSha256': asset['nativeSha256'],
        'baseBoot': asset.get('baseBoot'),
        'mountingReference': mounting_reference,
        'nativeBlenderBounds': {'min': list(minimum), 'max': list(maximum)},
        'removedBlenderCenter': list(center),
        'glTFSize': [maximum.x - minimum.x, maximum.z - minimum.z, maximum.y - minimum.y],
        'attachment': 'Centered rigid prop; use uniform runtime scale and matching Chao hand/foot bone. Native orientations retained; per-bone mounting rotation is a runtime adaptation.',
        'geometryChanges': 'Native rest transforms baked; full Shadow heel/toe mesh assembled with native upgrade overlay where applicable; bounding-box center translated to origin. No shape edits or texture recolors.',
    }
    document['asset'].setdefault('extras', {})['sa2Source'] = metadata
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary_chunk = data[20 + json_size:]
    temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk)) + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    helpers.replace_asset(temporary, target)
    record = {**metadata, 'name': asset['name'], 'output': str(target.relative_to(ROOT)),
              'meshCount': len(baked), 'vertices': sum(len(obj.data.vertices) for obj in baked),
              'materials': originals, 'bytes': target.stat().st_size,
              'sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
    print('SA2_UPGRADE_EXPORT=' + json.dumps(record))
    if '--preview' in sys.argv and asset['side'] == 'left':
        helpers.render_preview(asset['name'], list(minimum - center), list(maximum - center))
    return record


if __name__ == '__main__':
    OUTPUT.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(MANIFEST.read_text())
    records = [convert(asset) for asset in manifest['assets']]
    (OUTPUT / 'upgrade-conversion-report.json').write_text(json.dumps({'version': 1, 'assets': records}, indent=2) + '\n')
