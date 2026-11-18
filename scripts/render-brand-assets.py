"""Render private branding from the actual locally converted SA2 assets.

Run portable Blender --background --factory-startup --python this-file.
Uses existing ignored GLBs/PNG; does not extract or modify the installation.
An optional brand-gear-mounts.json contains renderer-audited rigid fits. It is
not required for a portrait and does not change the playable Chao skeleton.
"""
import hashlib
import json
import math
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / '.local/imports'
COLORS = ['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white']
USED = []


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
    USED.append({'asset': name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    return wrapper, objects


def bounds(objects):
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


def add_owner_gear(chao_objects):
    path = ASSETS / 'brand-gear-mounts.json'
    if not path.exists():
        return False
    # Babylon LH to Blender: X=-X, Y=-Z, Z=Y. Conjugation preserves the
    # exact audited rigid prop fit, including the source handedness root.
    conversion = Matrix(((-1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
    for record in json.loads(path.read_text()):
        wrapper, _objects = import_asset(record['asset'])
        values = record['matrix']
        matrix = Matrix(tuple(tuple(values[column * 4 + row] for column in range(4)) for row in range(4)))
        wrapper.matrix_world = conversion @ matrix @ conversion.inverted()
    for obj in chao_objects:
        if obj.type == 'MESH' and (obj.parent_bone.startswith('006_object_') or obj.parent_bone.startswith('013_object_')):
            obj.hide_render = True
            continue
        ancestor = obj.parent
        while ancestor:
            if obj.type == 'MESH' and (ancestor.name.startswith('006_object_') or ancestor.name.startswith('013_object_')):
                obj.hide_render = True
                break
            ancestor = ancestor.parent
    return True


def render():
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
    scene.render.resolution_x = scene.render.resolution_y = 1024
    scene.render.filepath = str(ASSETS / 'discord-icon.png')
    bpy.ops.render.render(write_still=True)
    # Full character portrait retains the same genuine model, native effects,
    # and renderer-audited owner equipment, with room around the silhouette.
    camera.location = (0.35, -2.5, 0.6)
    target = Vector((0, 0, 0.385))
    camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera_data.ortho_scale = 1.08
    flame.rotation_euler = camera.rotation_euler
    login_positions = [(-0.28, 0.58), (-0.33, 0.4), (-0.27, 0.22), (0, 0.14), (0.27, 0.22), (0.33, 0.4), (0.28, 0.58)]
    for (emerald, center, height), (x, z) in zip(emeralds, login_positions):
        scale = 0.078 / height
        emerald.scale = (scale, scale, scale)
        emerald.location = Vector((x, -0.12, z)) - center * scale
    scene.render.resolution_x = 1024
    scene.render.resolution_y = 1280
    scene.render.filepath = str(ASSETS / 'login-chao.png')
    bpy.ops.render.render(write_still=True)
    report = {'source': 'Actual locally converted game geometry and original embedded textures; no generated substitute',
              'ownerGear': owner_gear, 'flame': 'Original torch_blue00 additive sprite',
              'emeralds': 'All seven original e0207 models/textures with camera-normal projection',
              'inputs': USED, 'outputs': [{'path': 'discord-icon.png', 'size': [1024, 1024]}, {'path': 'login-chao.png', 'size': [1024, 1280]}]}
    (ASSETS / 'brand-render-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print('NATIVE_BRAND_RENDER=' + json.dumps(report))


if __name__ == '__main__':
    render()
