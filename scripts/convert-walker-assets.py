"""Convert original SA2 Dark Chao Walker geometry, textures, and motions.

Run portable Blender --background --factory-startup --python this-file.
Original PRS inputs must first be extracted with extract-walker-assets.js.
"""
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('walker_asset_helpers', ROOT / 'scripts/convert-cinematic-assets.py')
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
SOURCE = ROOT / '.local/extracted/dark-walker'
OUTPUT = ROOT / '.local/imports'
MOTIONS = {
    88: 'IdleAlternate', 89: 'Idle', 94: 'SpringLaunch', 95: 'SpringFall',
    97: 'Land', 98: 'LandHeavy', 99: 'Brake', 100: 'Damage', 102: 'Wind',
    105: 'Turn', 106: 'SlopeUp', 107: 'SlopeDown', 108: 'Victory',
    113: 'Defeat', 114: 'LedgeBalance', 115: 'Jump', 116: 'Fall',
    119: 'Whistle', 120: 'DashPanel', 121: 'RunningJump', 123: 'Pulley',
    124: 'ShrineLook', 126: 'MysticMelody', 127: 'Hover', 133: 'FirePowerLaser',
    134: 'Lose', 135: 'SpecialAttack', 190: 'IdleLook', 192: 'OmochaoActivate',
    196: 'Intro', 197: 'IntroLoop', 199: 'Walk', 200: 'Jog', 201: 'Run',
}


def inspect_model():
    helpers.clear_scene()
    source = SOURCE / 'dwalkmdl/465.sa2mdl'
    bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}], import_as_armature=True)
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    report = {
        'source': str(source.relative_to(ROOT)),
        'materials': helpers.source_materials(),
        'objects': [],
        'bones': [],
    }
    for obj in bpy.context.scene.objects:
        if obj.type == 'MESH':
            bounds = [obj.matrix_world @ Vector(corner) for corner in obj.bound_box]
            report['objects'].append({
                'name': obj.name, 'parent': obj.parent.name if obj.parent else None,
                'parentBone': obj.parent_bone, 'vertices': len(obj.data.vertices),
                'groups': [group.name for group in obj.vertex_groups],
                'materials': [material.name for material in obj.data.materials],
                'min': [min(c[i] for c in bounds) for i in range(3)],
                'max': [max(c[i] for c in bounds) for i in range(3)],
            })
        elif obj.type == 'ARMATURE':
            for bone in obj.data.bones:
                report['bones'].append({'name': bone.name, 'parent': bone.parent.name if bone.parent else None,
                                        'head': list(bone.head_local), 'tail': list(bone.tail_local)})
    (OUTPUT / 'dark-walker-model-audit.json').write_text(json.dumps(report, indent=2) + '\n')
    print('DARK_WALKER_MODEL_AUDIT=' + json.dumps(report))


def export_walker():
    helpers.clear_scene()
    # SA2 motion frames use the game's 60 Hz clock. Blender's factory 24 fps
    # would stretch every source action by 2.5x when exporting time in seconds.
    bpy.context.scene.render.fps = 60
    bpy.context.scene.render.fps_base = 1
    source = SOURCE / 'dwalkmdl/465.sa2mdl'
    bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}], import_as_armature=True)
    removed = []
    pilot_meshes = []
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
        elif obj.type == 'MESH' and obj.parent_bone:
            index = int(obj.parent_bone.split('_')[0])
            # Official Character Model Structures: node12 is the original
            # pilot's floating emotion ball, nodes14–32 contain that pilot.
            if index == 12 or index >= 14:
                removed.append({'name': obj.name, 'nativeNode': index})
                if index >= 14:
                    pilot_meshes.append(obj)
                else:
                    bpy.data.objects.remove(obj, do_unlink=True)
    armature = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    motion_records = []
    for motion_id, name in MOTIONS.items():
        armature.animation_data_clear()
        for bone in armature.pose.bones:
            bone.matrix_basis = Matrix.Identity(4)
        animation = SOURCE / 'dwalkmtn' / f'{motion_id}.saanim'
        bpy.ops.saio.import_node_anim(filepath=str(animation), files=[{'name': animation.name}], short_rot=True)
        armature.animation_data.action.name = name
        armature.animation_data.action.use_fake_user = True
        motion_records.append({'nativeId': motion_id, 'name': name,
                               'sourceFrameRange': list(armature.animation_data.action.frame_range)})
    idle = bpy.data.actions['Idle']
    armature.animation_data.action = None
    for bone in armature.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    armature.animation_data.action = idle
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()
    pilot_bounds = [obj.matrix_world @ Vector(corner) for obj in pilot_meshes for corner in obj.bound_box]
    pilot_seat = Vector(((min(c.x for c in pilot_bounds) + max(c.x for c in pilot_bounds)) / 2,
                         (min(c.y for c in pilot_bounds) + max(c.y for c in pilot_bounds)) / 2,
                         min(c.z for c in pilot_bounds)))
    pilot_bone = next(bone for bone in armature.pose.bones if bone.name.startswith('014_'))
    seat = bpy.data.objects.new('ChaoPilotSeat', None)
    bpy.context.scene.collection.objects.link(seat)
    seat.parent = armature
    seat.parent_type = 'BONE'
    seat.parent_bone = pilot_bone.name
    seat.matrix_world = Matrix.Translation(pilot_seat)
    for obj in pilot_meshes:
        bpy.data.objects.remove(obj, do_unlink=True)
    originals = helpers.prepare_materials('dwalktex')
    bpy.context.view_layer.update()
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    bounds = [obj.matrix_world @ Vector(corner) for obj in meshes for corner in obj.bound_box]
    minimum = [min(c[i] for c in bounds) for i in range(3)]
    maximum = [max(c[i] for c in bounds) for i in range(3)]
    pilot_origin = list((armature.matrix_world @ pilot_bone.matrix).translation)
    temporary = OUTPUT / 'dark-chao-walker.converting.glb'
    target = OUTPUT / 'dark-chao-walker.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', use_active_scene=True,
                              export_animations=True, export_animation_mode='ACTIONS',
                              export_force_sampling=True, export_vertex_color='ACTIVE',
                              export_optimize_animation_size=False,
                              export_optimize_animation_keep_anim_armature=False,
                              export_reset_pose_bones=True)
    data = temporary.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    lookup = {entry['name']: entry for entry in originals}
    for material in document.get('materials', []):
        original = lookup[material['name']]
        material.setdefault('extras', {})['sa2Source'] = original
        material['doubleSided'] = original['doubleSided']
        pbr = material.setdefault('pbrMetallicRoughness', {})
        pbr['baseColorFactor'] = original['diffuse'] if original['useAlpha'] else original['diffuse'][:3] + [1]
        pbr['metallicFactor'] = 0
        pbr['roughnessFactor'] = math.sqrt(2 / (original['specularExponent'] + 2))
        material['alphaMode'] = 'BLEND' if original['useAlpha'] else 'OPAQUE'
    for node in document.get('nodes', []):
        if node.get('name', '').startswith('014_'):
            node.setdefault('extras', {})['chaoPilotAnchor'] = True
        if node.get('name') == 'ChaoPilotSeat':
            node.setdefault('extras', {})['chaoPilotSeat'] = True
    document['asset'].setdefault('extras', {})['sa2Source'] = {
        'model': 'resource/gd_PC/dwalkmdl.prs, model465',
        'textures': 'resource/gd_PC/dwalktex.prs',
        'animations': {name: f'resource/gd_PC/dwalkmtn.prs, motion{motion_id}' for motion_id, name in MOTIONS.items()},
        'sourceFramesPerSecond': 60,
        'nativePilotRemoved': removed,
        'pilotAnchorNativeNode': 14,
    }
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary_chunk = data[20 + json_size:]
    temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk)) + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    helpers.replace_asset(temporary, target)
    report = {
        'output': str(target.relative_to(ROOT)), 'sourceModel': str(source.relative_to(ROOT)),
        'bytes': target.stat().st_size, 'meshCount': len(meshes),
        'vertices': sum(len(obj.data.vertices) for obj in meshes),
        'boundsBlender': {'min': minimum, 'max': maximum},
        'originalPilotRemoved': removed, 'pilotOriginBlender': pilot_origin,
        'pilotSeatBlender': list(pilot_seat),
        'animations': motion_records, 'sourceFramesPerSecond': 60, 'materials': originals,
        'separateRigMotion': {'198': 'Dark Omochao Attack belongs to model474 mechanism; not retargeted onto Walker'},
        'constantStandingPose': 'Native motion89 is constant; glTF represents it in the exported standing transforms rather than a fabricated animation loop.',
        'exportedAnimationCount': len(document.get('animations', [])),
    }
    (OUTPUT / 'dark-walker-conversion-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print('DARK_WALKER_EXPORT=' + json.dumps({key: value for key, value in report.items() if key != 'materials'}))
    if '--preview' in sys.argv:
        preview_driver(pilot_seat, minimum, maximum)


def export_effect(model_id, name):
    helpers.clear_scene()
    source = SOURCE / 'dwalkmdl' / f'{model_id}.sa2mdl'
    bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}])
    flags = {material.name: bool(material.saio_material.use_texture) for material in bpy.data.materials}
    originals = helpers.prepare_materials('dwalktex')
    for original in originals:
        original['useTexture'] = flags[original['name']]
        if not original['useTexture']:
            original['texture'] = None
            material = bpy.data.materials[original['name']]
            shader = next(node for node in material.node_tree.nodes if node.type == 'BSDF_PRINCIPLED')
            for link in list(shader.inputs['Base Color'].links):
                material.node_tree.links.remove(link)
            for link in list(shader.inputs['Alpha'].links):
                material.node_tree.links.remove(link)
            shader.inputs['Base Color'].default_value = original['diffuse']
            shader.inputs['Alpha'].default_value = original['diffuse'][3]
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.context.view_layer.update()
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    bounds = [obj.matrix_world @ Vector(corner) for obj in meshes for corner in obj.bound_box]
    minimum = [min(c[i] for c in bounds) for i in range(3)]
    maximum = [max(c[i] for c in bounds) for i in range(3)]
    temporary = OUTPUT / f'{name}.converting.glb'
    target = OUTPUT / f'{name}.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', use_active_scene=True,
                              export_animations=False, export_vertex_color='ACTIVE')
    data = temporary.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    lookup = {entry['name']: entry for entry in originals}
    for material in document.get('materials', []):
        original = lookup[material['name']]
        material.setdefault('extras', {})['sa2Source'] = original
        material['doubleSided'] = original['doubleSided']
        pbr = material.setdefault('pbrMetallicRoughness', {})
        pbr['baseColorFactor'] = original['diffuse']
        pbr['metallicFactor'] = 0
        pbr['roughnessFactor'] = 1
        material['alphaMode'] = 'BLEND' if original['useAlpha'] else 'OPAQUE'
    document['asset'].setdefault('extras', {})['sa2Source'] = {'model': f'resource/gd_PC/dwalkmdl.prs, model{model_id}', 'sourceFrameRate': 60}
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary_chunk = data[20 + json_size:]
    temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk)) + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    helpers.replace_asset(temporary, target)
    report = {'name': name, 'nativeModelId': model_id, 'bytes': target.stat().st_size,
              'boundsBlender': {'min': minimum, 'max': maximum}, 'materials': originals}
    (OUTPUT / f'{name}-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print('DARK_WALKER_EFFECT=' + json.dumps(report))


def preview_driver(pilot_seat, minimum, maximum):
    existing = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'dark-chaos.glb'))
    imported = [obj for obj in bpy.context.scene.objects if obj not in existing]
    pilot = bpy.data.objects.new('Actual Dark Chaos Pilot', None)
    bpy.context.scene.collection.objects.link(pilot)
    for obj in imported:
        if obj.parent not in imported:
            obj.parent = pilot
    pilot.rotation_euler.z = -math.pi / 2
    bpy.context.view_layer.update()
    feet = min((obj.matrix_world @ Vector(corner)).z for obj in imported if obj.type == 'MESH' for corner in obj.bound_box)
    pilot.location = pilot_seat - Vector((0, 0, feet))
    bpy.context.view_layer.update()
    helpers.render_preview('dark-chao-walker-with-driver', minimum, maximum)
    scene = bpy.context.scene
    center = Vector([(a + b) / 2 for a, b in zip(minimum, maximum)])
    size = max(b - a for a, b in zip(minimum, maximum))
    scene.camera.location = center + Vector((-size * 1.8, -size * 1.2, size * 0.65))
    scene.camera.rotation_euler = (center - scene.camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(OUTPUT / 'dark-chao-walker-with-driver-front.png')
    bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    if '--audit' in sys.argv:
        inspect_model()
    else:
        export_walker()
        export_effect(267, 'dark-walker-shot')
        export_effect(283, 'dark-walker-booster')
