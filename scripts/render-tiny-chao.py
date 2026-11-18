"""Render private Tiny Garden Chaos sprite atlases from native models/motions.

Blender --background --factory-startup --python scripts/render-tiny-chao.py
These are new 2D presentations of the real desktop forms, not recovered GBA art.
"""
import hashlib
import json
import math
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / '.local/imports'
TILE = 96
FRAMES = 4
DIRECTIONS = 8


def bounds(objects):
    bpy.context.view_layer.update()
    graph = bpy.context.evaluated_depsgraph_get()
    points = []
    for obj in objects:
        if obj.type != 'MESH' or obj.hide_render:
            continue
        evaluated = obj.evaluated_get(graph)
        mesh = evaluated.to_mesh()
        points.extend(evaluated.matrix_world @ v.co for v in mesh.vertices)
        evaluated.to_mesh_clear()
    lo = Vector(tuple(min(p[i] for p in points) for i in range(3)))
    hi = Vector(tuple(max(p[i] for p in points) for i in range(3)))
    return lo, hi


def area_light(name, location, energy):
    data = bpy.data.lights.new(name, 'AREA')
    data.energy, data.size = energy, 3
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    obj.location = location
    obj.rotation_euler = (Vector((0, 0, .2)) - obj.location).to_track_quat('-Z', 'Y').to_euler()


def render_variant(variant):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12
    scene.cycles.use_denoising = True
    scene.render.film_transparent = True
    scene.render.resolution_x = scene.render.resolution_y = TILE
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'PNG'
    scene.render.image_settings.color_mode = 'RGBA'
    scene.view_settings.view_transform = 'Standard'
    world = bpy.data.worlds.new('Soft native sprite light')
    scene.world = world
    world.use_nodes = True
    world.node_tree.nodes['Background'].inputs['Color'].default_value = (.4, .5, .65, 1)
    world.node_tree.nodes['Background'].inputs['Strength'].default_value = .6
    source = ASSETS / f'{variant}-chaos.glb'
    bpy.ops.import_scene.gltf(filepath=str(source))
    objects = list(scene.objects)
    helpers = {b.custom_shape for obj in objects if obj.type == 'ARMATURE' for b in obj.pose.bones if b.custom_shape}
    for helper in helpers:
        if helper in objects:
            objects.remove(helper)
            bpy.data.objects.remove(helper, do_unlink=True)
    wrapper = bpy.data.objects.new('Uniform native sprite scale', None)
    scene.collection.objects.link(wrapper)
    for obj in objects:
        if obj.parent not in objects:
            obj.parent = wrapper
    wrapper.scale = (.1, .1, .1)
    animated = [obj for obj in objects if obj.animation_data]
    for obj in animated:
        # glTF import activates Idle as an action while storing muted NLA tracks.
        # Clear the active layer before enabling a source track, so Idle cannot override Walk.
        obj.animation_data.action = None
    for action in ['Idle', 'Walk']:
        if not any(any(track.name == action for track in obj.animation_data.nla_tracks) for obj in animated):
            raise RuntimeError(f'{source.name} has no native {action} track')
    scene.frame_set(0)
    lo, hi = bounds(objects)
    wrapper.location -= Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z))
    height = hi.z - lo.z
    target = Vector((0, 0, height * .48))
    camera_data = bpy.data.cameras.new('Eight direction sprite camera')
    camera_data.type = 'ORTHO'
    camera_data.ortho_scale = max(height * 1.35, (hi.x - lo.x) * 1.4)
    camera = bpy.data.objects.new('Eight direction sprite camera', camera_data)
    scene.collection.objects.link(camera)
    scene.camera = camera
    area_light('Key', (-1, -2, 3), 100)
    area_light('Fill', (1, 1, 2), 65)
    atlas = [0.0] * (TILE * FRAMES * 2 * TILE * DIRECTIONS * 4)
    width = TILE * FRAMES * 2
    rigs_before = {obj.name: len(obj.pose.bones) for obj in objects if obj.type == 'ARMATURE'}
    pose_signatures = {'Idle': set(), 'Walk': set()}
    for direction in range(DIRECTIONS):
        angle = direction * math.tau / DIRECTIONS
        camera.location = (math.sin(angle) * 2.5, -math.cos(angle) * 2.5, height * .7)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        for action_index, action in enumerate(['Idle', 'Walk']):
            for obj in animated:
                for track in obj.animation_data.nla_tracks:
                    track.mute = track.name != action
            for frame in range(FRAMES):
                scene.frame_set(1 + frame * 3)
                pose = [round(value, 6) for obj in objects if obj.type == 'ARMATURE' for bone in obj.pose.bones for row in bone.matrix for value in row]
                pose_signatures[action].add(hashlib.sha256(json.dumps(pose).encode()).hexdigest())
                tile_path = ASSETS / 'tiny-render-tile.png'
                scene.render.filepath = str(tile_path)
                bpy.ops.render.render(write_still=True)
                tile_image = bpy.data.images.load(str(tile_path), check_existing=False)
                pixels = list(tile_image.pixels[:])
                bpy.data.images.remove(tile_image)
                if len(pixels) != TILE * TILE * 4:
                    raise RuntimeError('Unexpected render tile size')
                # Blender pixels start at the lower-left. Row zero is front in the saved PNG.
                x_offset = (action_index * FRAMES + frame) * TILE
                y_offset = (DIRECTIONS - 1 - direction) * TILE
                for row in range(TILE):
                    destination = ((y_offset + row) * width + x_offset) * 4
                    atlas[destination:destination + TILE * 4] = pixels[row * TILE * 4:(row + 1) * TILE * 4]
    rigs_after = {obj.name: len(obj.pose.bones) for obj in objects if obj.type == 'ARMATURE'}
    if rigs_before != rigs_after:
        raise RuntimeError('Native rig changed during sprite rendering')
    if any(len(signatures) < 2 for signatures in pose_signatures.values()):
        raise RuntimeError('Native source motion did not animate the sprite frames')
    image = bpy.data.images.new(f'Tiny {variant} Chaos native atlas', width=width, height=TILE * DIRECTIONS, alpha=True)
    image.pixels.foreach_set(atlas)
    image.filepath_raw = str(ASSETS / f'tiny-{variant}-chaos.png')
    image.file_format = 'PNG'
    image.save()
    return {'variant': variant, 'source': source.name, 'sourceSha256': hashlib.sha256(source.read_bytes()).hexdigest(),
            'output': image.filepath_raw, 'outputSha256': hashlib.sha256(Path(image.filepath_raw).read_bytes()).hexdigest(),
            'nativeBones': rigs_before, 'rigUnchanged': True, 'nativeActions': ['Idle', 'Walk'],
            'distinctSourcePoses': {action: len(signatures) for action, signatures in pose_signatures.items()}}


records = [render_variant(variant) for variant in ['neutral', 'hero', 'dark']]
report = {'schemaVersion': 1, 'status': 'New phone sprite presentation rendered from native desktop Chaos forms; not unused GBA art.',
          'tile': TILE, 'columns': 8, 'rows': 8, 'framesPerAction': FRAMES,
          'actions': {'idle': {'firstColumn': 0, 'frames': FRAMES}, 'walk': {'firstColumn': 4, 'frames': FRAMES}},
          'directions': ['front', 'front-right', 'right', 'back-right', 'back', 'back-left', 'left', 'front-left'],
          'headEffect': 'Composite verified native emotion effects separately in the browser.', 'variants': records}
(ASSETS / 'tiny-chao-atlas.json').write_text(json.dumps(report, indent=2) + '\n')
print('TINY_CHAOS_ATLASES=' + json.dumps(report))
