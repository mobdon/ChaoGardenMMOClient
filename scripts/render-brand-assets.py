"""Render private branding from the actual locally converted SA2 assets.

Run portable Blender --background --factory-startup --python this-file.
Uses existing ignored GLBs/PNG; does not extract or modify the installation.
Footwear is fitted from the same native toe/sole basis and uniform length
ratio as the browser. Optional brand-gear-mounts.json retains audited gloves.
No playable Chao skeleton, game installation, profile or save is changed.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import sys

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / '.local/imports'
COLORS = ['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white']
USED = []
HIDDEN_JETS = []
FITS = []


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def import_asset(name):
    path = ASSETS / f'{name}.glb'
    existing = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    objects = [obj for obj in bpy.context.scene.objects if obj not in existing]
    # Blender creates an Icosphere custom-bone widget during glTF import.
    # It is an editor helper, not source geometry, and must not affect bounds
    # or appear in an asset render when parenting imported roots together.
    custom_shapes = {bone.custom_shape for obj in objects if obj.type == 'ARMATURE' for bone in obj.pose.bones if bone.custom_shape}
    for shape in custom_shapes:
        if shape in objects:
            objects.remove(shape)
            bpy.data.objects.remove(shape, do_unlink=True)
    wrapper = bpy.data.objects.new(f'Native source {name}', None)
    bpy.context.scene.collection.objects.link(wrapper)
    for obj in objects:
        if obj.parent not in objects:
            obj.parent = wrapper
        obj.animation_data_clear()
        if obj.type == 'ARMATURE':
            for bone in obj.pose.bones:
                bone.matrix_basis.identity()
        if re.search(r'native-shadow-jet-9[2-9]-(left|right)-(toe|heel)-(narrow|wide)', obj.name):
            # All eight original jets are bundled with current footwear. Idle
            # portraits neither show them nor measure them as part of the boot.
            obj.hide_render = True
            HIDDEN_JETS.append(obj.name)
    USED.append({'asset': name, 'sha256': fingerprint(path)})
    return wrapper, objects


def geometry_points(objects):
    bpy.context.view_layer.update()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    points = []
    for obj in objects:
        if obj.type != 'MESH' or obj.hide_render:
            continue
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        points.extend(evaluated.matrix_world @ vertex.co for vertex in mesh.vertices)
        evaluated.to_mesh_clear()
    return points


def bounds(objects):
    points = geometry_points(objects)
    minimum = Vector(tuple(min(point[i] for point in points) for i in range(3)))
    maximum = Vector(tuple(max(point[i] for point in points) for i in range(3)))
    return minimum, maximum


def native_emerald_projection(objects):
    for material in {mat for obj in objects if obj.type == 'MESH' for mat in obj.data.materials}:
        nodes, links = material.node_tree.nodes, material.node_tree.links
        textures = [node for node in nodes if node.type == 'TEX_IMAGE']
        if not textures:
            raise RuntimeError('Native emerald lost its embedded image')
        geometry = nodes.new('ShaderNodeNewGeometry')
        camera_normal = nodes.new('ShaderNodeVectorTransform')
        camera_normal.vector_type = 'NORMAL'
        camera_normal.convert_from = 'WORLD'
        camera_normal.convert_to = 'CAMERA'
        projection = nodes.new('ShaderNodeVectorMath')
        projection.operation = 'MULTIPLY_ADD'
        projection.inputs[1].default_value = (0.5, 0.5, 0)
        projection.inputs[2].default_value = (0.5, 0.5, 0)
        links.new(geometry.outputs['Normal'], camera_normal.inputs['Vector'])
        links.new(camera_normal.outputs['Vector'], projection.inputs[0])
        for texture in textures:
            links.new(projection.outputs['Vector'], texture.inputs['Vector'])
        shader = next(node for node in nodes if node.type == 'BSDF_PRINCIPLED')
        shader.inputs['Metallic'].default_value = 0.15
        shader.inputs['Roughness'].default_value = 0.16
        shader.inputs['Alpha'].default_value = 0.8


def add_original_flame(camera, position):
    # This is the game's torch_blue00 texture, not a newly drawn flame. Its
    # black background contributes zero through the original additive rule.
    path = ASSETS / 'emotion-flame.png'
    image = bpy.data.images.load(str(path), check_existing=True)
    material = bpy.data.materials.new('Native blue flame: transparent plus original emission')
    material.use_nodes = True
    nodes, links = material.node_tree.nodes, material.node_tree.links
    nodes.clear()
    output = nodes.new('ShaderNodeOutputMaterial')
    texture = nodes.new('ShaderNodeTexImage')
    texture.image = image
    texture.interpolation = 'Closest'
    emission = nodes.new('ShaderNodeEmission')
    emission.inputs['Strength'].default_value = 1
    transparent = nodes.new('ShaderNodeBsdfTransparent')
    add = nodes.new('ShaderNodeAddShader')
    links.new(texture.outputs['Color'], emission.inputs['Color'])
    links.new(transparent.outputs[0], add.inputs[0])
    links.new(emission.outputs[0], add.inputs[1])
    links.new(add.outputs[0], output.inputs[0])
    bpy.ops.mesh.primitive_plane_add(size=0.18, location=position)
    plane = bpy.context.object
    plane.name = 'Original Dark Chaos emotion billboard'
    plane.rotation_euler = camera.rotation_euler
    plane.data.materials.append(material)
    plane.visible_shadow = False
    USED.append({'asset': 'emotion-flame.png', 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    return plane


def light(name, location, target, energy, size, color=(1, 1, 1)):
    data = bpy.data.lights.new(name, 'AREA')
    data.energy, data.size, data.color = energy, size, color
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    obj.location = location
    obj.rotation_euler = (Vector(target) - obj.location).to_track_quat('-Z', 'Y').to_euler()


def set_background():
    world = bpy.data.worlds.new('Navy portrait background')
    bpy.context.scene.world = world
    world.use_nodes = True
    nodes, links = world.node_tree.nodes, world.node_tree.links
    nodes.clear()
    output = nodes.new('ShaderNodeOutputWorld')
    camera = nodes.new('ShaderNodeLightPath')
    ambient = nodes.new('ShaderNodeBackground')
    ambient.inputs['Color'].default_value = (0.45, 0.5, 0.65, 1)
    ambient.inputs['Strength'].default_value = 0.5
    backdrop = nodes.new('ShaderNodeBackground')
    backdrop.inputs['Color'].default_value = (0.004, 0.009, 0.025, 1)
    backdrop.inputs['Strength'].default_value = 1
    mix = nodes.new('ShaderNodeMixShader')
    links.new(camera.outputs['Is Camera Ray'], mix.inputs[0])
    links.new(ambient.outputs[0], mix.inputs[1])
    links.new(backdrop.outputs[0], mix.inputs[2])
    links.new(mix.outputs[0], output.inputs[0])


def native_part(objects, index):
    prefix = f'{index:03d}_object_'
    result = []
    for obj in objects:
        if obj.type != 'MESH':
            continue
        if obj.parent_bone.startswith(prefix):
            result.append(obj)
            continue
        ancestor = obj.parent
        while ancestor:
            if ancestor.name.startswith(prefix):
                result.append(obj)
                break
            ancestor = ancestor.parent
    if not result:
        raise RuntimeError(f'Missing original native mesh at joint {index:03d}')
    return result


def shoe_rotation(reference):
    # glTF Y-up -> Blender Z-up. Use the actual native heel-to-toe axis,
    # projected on the sole, rather than the old nominal Shadow-forward axis.
    native_rotation = Matrix(reference['nativeHeelRestWorldBlenderMatrix']).to_3x3()
    toe = native_rotation.inverted() @ Vector(reference['nativeHeelToToeWorldBlender'])
    up = Vector(reference['exportedPropStandingUp'])
    up = Vector((up.x, -up.z, up.y)).normalized()
    forward = (toe - up * toe.dot(up)).normalized()
    source = Matrix((up.cross(forward).normalized(), up, forward)).transposed()
    target = Matrix(((1, 0, 0), (0, 0, 1), (0, -1, 0))).transposed()
    rotation = target @ source.transposed()
    if (rotation @ up - Vector((0, 0, 1))).length > 1e-5:
        raise RuntimeError('Native boot sole basis did not remain upright')
    if (rotation @ forward - Vector((0, -1, 0))).length > 1e-5:
        raise RuntimeError('Native heel-to-toe basis did not face forward')
    return rotation.to_quaternion()


def add_owner_gear(chao_objects):
    attachments = (ROOT / 'src/chao-attachments.js').read_text()
    ratio = float(re.search(r'CHAO_SHOE_LENGTH_RATIO\s*=\s*([.0-9]+)', attachments).group(1))
    reference_side = re.search(r"CHAO_SHOE_REFERENCE_SIDE\s*=\s*'([^']+)'", attachments).group(1)
    if not 0 < ratio <= 2:
        raise RuntimeError('Invalid browser footwear length ratio')
    if reference_side != 'right':
        raise RuntimeError('Portrait requires the agreed original native right boot reference')
    report_path = ASSETS / 'upgrade-conversion-report.json'
    records = json.loads(report_path.read_text())['assets']
    USED.append({'asset': report_path.name, 'sha256': fingerprint(report_path)})
    record = next(row for row in records if row.get('name') == 'shadow-air-shoes-right')
    right_placement = None
    right_bounds = None
    right_vertices = None
    for side, index in [('right', 6), ('left', 13)]:
        feet = native_part(chao_objects, index)
        foot_min, foot_max = bounds(feet)
        wrapper, objects = import_asset(record['name'])
        if side == 'right':
            wrapper.rotation_mode = 'QUATERNION'
            wrapper.rotation_quaternion = shoe_rotation(record['mountingReference'])
            lo, hi = bounds(objects)
            scale = (foot_max.y - foot_min.y) * ratio / (hi.y - lo.y)
            center = (lo + hi) / 2
            position = (foot_min + foot_max) / 2 - center * scale
            position.z = foot_min.z - lo.z * scale
            wrapper.scale = (scale, scale, scale)
            wrapper.location = position
            bpy.context.view_layer.update()
            right_placement = wrapper.matrix_world.copy()
        else:
            # Match mirroredShadowShoeRest: one native boot, reflected as a
            # complete rigid prop at rest. Never refit the separate left mesh.
            wrapper.matrix_world = Matrix.Diagonal((-1, 1, 1, 1)) @ right_placement
        final_min, final_max = bounds(objects)
        actual_ratio = (final_max.y - final_min.y) / (foot_max.y - foot_min.y)
        if abs(actual_ratio - ratio) > 1e-5 or abs(final_min.z - foot_min.z) > 1e-5:
            raise RuntimeError('Portrait footwear lost browser length/sole fitting')
        if side == 'right':
            right_bounds = (final_min, final_max)
            right_vertices = geometry_points(objects)
            symmetry_error = 0
        else:
            reflected_min = Vector((-right_bounds[1].x, right_bounds[0].y, right_bounds[0].z))
            reflected_max = Vector((-right_bounds[0].x, right_bounds[1].y, right_bounds[1].z))
            if (final_min - reflected_min).length > 1e-5 or (final_max - reflected_max).length > 1e-5:
                raise RuntimeError('Portrait footwear lost exact mirrored-rest symmetry')
            left_vertices = geometry_points(objects)
            if len(left_vertices) != len(right_vertices):
                raise RuntimeError('Portrait mirrored footwear changed native vertex count')
            symmetry_error = max((left - Vector((-right.x, right.y, right.z))).length
                                 for right, left in zip(right_vertices, left_vertices))
            if symmetry_error > 1e-5:
                raise RuntimeError('Portrait footwear vertices lost exact mirrored-rest symmetry')
        FITS.append({'asset': record['name'], 'side': side, 'mirrored': side == 'left',
                     'nativeFoot': f'{index:03d}', 'uniformScaleMagnitude': scale,
                     'lengthRatio': actual_ratio, 'toeBasis': 'projected original heel-to-toe',
                     'attachmentAdaptation': 'native-right-mirrored-at-rest; same as mirroredShadowShoeRest',
                     'maxMirrorVertexError': symmetry_error,
                     'footBounds': {'min': list(foot_min), 'max': list(foot_max)},
                     'shoeBounds': {'min': list(final_min), 'max': list(final_max)}})
        for obj in feet:
            obj.hide_render = True

    # Keep the previously audited native glove matrices; only the obsolete
    # footwear entries are replaced by the current computed native fit above.
    path = ASSETS / 'brand-gear-mounts.json'
    if not path.exists():
        return {'footwear': True, 'hands': False}
    USED.append({'asset': path.name, 'sha256': fingerprint(path)})
    # Babylon LH to Blender: X=-X, Y=-Z, Z=Y. Conjugation preserves the
    # exact audited rigid prop fit, including the source handedness root.
    conversion = Matrix(((-1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
    for record in json.loads(path.read_text()):
        if record['slot'] != 'hands':
            continue
        wrapper, _objects = import_asset(record['asset'])
        values = record['matrix']
        matrix = Matrix(tuple(tuple(values[column * 4 + row] for column in range(4)) for row in range(4)))
        wrapper.matrix_world = conversion @ matrix @ conversion.inverted()
    return {'footwear': True, 'hands': True}


def add_chest_emerald(chao_objects):
    torso = native_part(chao_objects, 1)
    lo, hi = bounds(torso)
    surface = Vector(((lo.x + hi.x) / 2, lo.y, lo.z + (hi.z - lo.z) * .62))
    origin = surface - Vector((0, max(1, hi.y - lo.y), 0))
    depsgraph = bpy.context.evaluated_depsgraph_get()
    nearest = None
    for obj in torso:
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        tree = BVHTree.FromPolygons([evaluated.matrix_world @ vertex.co for vertex in mesh.vertices],
                                   [list(polygon.vertices) for polygon in mesh.polygons])
        hit, _normal, _index, distance = tree.ray_cast(origin, Vector((0, 1, 0)))
        evaluated.to_mesh_clear()
        if hit is not None and (nearest is None or distance < nearest[0]):
            nearest = (distance, hit)
    if nearest is None:
        raise RuntimeError('Original torso skin did not intersect chest emerald anchor')
    surface = nearest[1]
    emerald, objects = import_asset('emerald-cyan')
    native_emerald_projection(objects)
    minimum, maximum = bounds(objects)
    center = (minimum + maximum) / 2
    scale = .09 / (maximum.z - minimum.z)
    depth = (maximum.y - minimum.y) * scale
    position = surface + Vector((0, depth * .25, 0))
    emerald.scale = (scale, scale, scale)
    emerald.location = position - center * scale
    return {'asset': 'emerald-cyan', 'height': .09, 'insetFractionOfDepth': .25,
            'nativeDepth': depth, 'skinSurface': list(surface), 'center': list(position)}


def publish_outputs(stage, report):
    # Never let a live asset request read a partial PNG. Keep the previous
    # portrait/report together in ignored storage before replacing aliases.
    backup = ROOT / '.local/backups' / f'brand-render-{report["renderId"]}'
    backup.mkdir(parents=True, exist_ok=False)
    names = [item['path'] for item in report['outputs']] + ['brand-render-report.json']
    previous = []
    for name in names:
        target = ASSETS / name
        if target.exists():
            shutil.copy2(target, backup / name)
            previous.append({'path': name, 'sha256': fingerprint(target)})
    report['publication'] = {'backup': str(backup.relative_to(ROOT)), 'previous': previous}
    (stage / 'brand-render-report.json').write_text(json.dumps(report, indent=2) + '\n')
    for name in names:
        source, target = stage / name, ASSETS / name
        temporary = target.with_name(f'.{target.name}.{report["renderId"]}.tmp')
        shutil.copy2(source, temporary)
        temporary.replace(target)
        if fingerprint(source) != fingerprint(target):
            raise RuntimeError(f'Published portrait hash mismatch: {name}')


def render(options):
    render_id = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    stage = ROOT / '.local/brand-renders' / render_id
    stage.mkdir(parents=True, exist_ok=False)
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 40
    scene.cycles.use_denoising = True
    scene.render.image_settings.file_format = 'PNG'
    scene.render.image_settings.color_mode = 'RGBA'
    scene.render.resolution_percentage = 100
    scene.view_settings.view_transform = 'Standard'
    set_background()
    character, chao_objects = import_asset('dark-chaos')
    character.scale = (0.1, 0.1, 0.1)
    minimum, maximum = bounds(chao_objects)
    print('NATIVE_BRAND_CHAO_BOUNDS=' + json.dumps({'min': list(minimum), 'max': list(maximum)}))
    character.location.z -= minimum.z
    owner_gear = add_owner_gear(chao_objects)
    chest = add_chest_emerald(chao_objects)
    camera_data = bpy.data.cameras.new('Brand portrait camera')
    camera = bpy.data.objects.new('Brand portrait camera', camera_data)
    scene.collection.objects.link(camera)
    camera_data.type = 'ORTHO'
    camera_data.ortho_scale = 0.8
    camera.location = (0, -2.5, 0.43)
    target = Vector((0, 0, 0.43))
    camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.camera = camera
    flame = add_original_flame(camera, (0, -0.005, maximum.z - minimum.z + 0.13))
    positions = [(-0.245, 0.63), (-0.27, 0.48), (-0.245, 0.32), (0, 0.18), (0.245, 0.32), (0.27, 0.48), (0.245, 0.63)]
    emeralds = []
    for color, (x, z) in zip(COLORS, positions):
        emerald, objects = import_asset(f'emerald-{color}')
        native_emerald_projection(objects)
        lo, hi = bounds(objects)
        center = (lo + hi) / 2
        scale = 0.055 / (hi.z - lo.z)
        emerald.scale = (scale, scale, scale)
        emerald.location = Vector((x, -0.12, z)) - center * scale
        emeralds.append((emerald, center, hi.z - lo.z))
    light('Soft key', (-1.5, -2.5, 3), (0, 0, 0.4), 180, 2.5)
    light('Soft fill', (1.5, -1.5, 1.2), (0, 0, 0.4), 60, 2)
    light('Gold rim', (0.8, 1.5, 1.4), (0, 0, 0.4), 70, 1.5, (1, 0.78, 0.4))
    outputs = []
    if not options.login_only:
        scene.render.resolution_x = scene.render.resolution_y = 1024
        scene.render.filepath = str(stage / 'discord-icon.png')
        bpy.ops.render.render(write_still=True)
        outputs.append({'path': 'discord-icon.png', 'size': [1024, 1024]})
    # Full character portrait retains the same genuine model, native effects,
    # and renderer-audited owner equipment, with room around the silhouette.
    camera.location = (0.35, -2.5, 0.6)
    target = Vector((0, 0, 0.385))
    camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera_data.ortho_scale = 1.08
    flame.rotation_euler = camera.rotation_euler
    # The lowest orbit gem stays separate from the inset chest attachment.
    login_positions = [(-0.28, 0.58), (-0.33, 0.4), (-0.27, 0.22), (0, -0.065), (0.27, 0.22), (0.33, 0.4), (0.28, 0.58)]
    for (emerald, center, height), (x, z) in zip(emeralds, login_positions):
        scale = 0.078 / height
        emerald.scale = (scale, scale, scale)
        emerald.location = Vector((x, -0.12, z)) - center * scale
    scene.render.resolution_x = 1024
    scene.render.resolution_y = 1280
    scene.render.filepath = str(stage / 'login-chao.png')
    bpy.ops.render.render(write_still=True)
    outputs.append({'path': 'login-chao.png', 'size': [1024, 1280]})
    for item in outputs:
        path = stage / item['path']
        image = bpy.data.images.load(str(path), check_existing=False)
        if list(image.size) != item['size']:
            raise RuntimeError(f'Incomplete portrait render: {path}')
        bpy.data.images.remove(image)
        item['sha256'] = fingerprint(path)
    report = {'renderId': render_id,
              'source': 'Actual locally converted game geometry and original embedded textures; no generated substitute',
              'ownerGear': owner_gear, 'flame': 'Original torch_blue00 additive sprite',
              'emeralds': 'All seven original e0207 models/textures with camera-normal projection',
              'chestEmerald': chest, 'footwearFits': FITS,
              'hiddenIdleJets': HIDDEN_JETS,
              'scriptSha256': fingerprint(Path(__file__)),
              'attachmentSourceSha256': fingerprint(ROOT / 'src/chao-attachments.js'),
              'stage': str(stage.relative_to(ROOT)), 'inputs': USED, 'outputs': outputs}
    (stage / 'brand-render-report.json').write_text(json.dumps(report, indent=2) + '\n')
    if not options.preview_only:
        publish_outputs(stage, report)
    print('NATIVE_BRAND_RENDER=' + json.dumps(report))


if __name__ == '__main__':
    arguments = argparse.ArgumentParser(description=__doc__)
    arguments.add_argument('--login-only', action='store_true', help='Preserve the existing Discord icon')
    arguments.add_argument('--preview-only', action='store_true', help='Render privately without updating live asset aliases')
    render(arguments.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []))
