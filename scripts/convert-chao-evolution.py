"""Convert the native child/adult Chao body families into morphing GLBs.

Run with portable Blender (same setup as scripts/convert-assets.py):

    $env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local\\blender-user'
    & '.local\\tools\\blender-5.1.2-windows-x64\\blender.exe' --background --factory-startup `
        --python 'scripts\\convert-chao-evolution.py' [-- --preview] [-- --only al_nnz]

SA Tools PC_SA2/Chao_DLL.ini orders AL_RootObject[0..107] as 18 families of six
models: z, n, s, f, r, p (child nc/hc/dc, then adult nn hn dn, ns hs ds, nf hf df,
nr hr dr, np hp dp). Each family shares one 40-node tree and per-attach vertex
counts (verified on the local extraction), so they become glTF morph targets of
one base model. Adults: one GLB per family, base z, targets n, s, f, r, p.
Children: a single GLB on al_ncz with 17 targets n, s, f, r, p, hz..hp, dz..dp,
since the native child blend also lerps toward the hero/dark child set by |APos|.

* Nodes are matched by their three-digit tree index. SAIO names each node after
  its file offset as well (001_object_00016D7C), which differs between models.
* Vertex targets are expressed in each node's local space, so the rig and native
  proportions are preserved and animation still drives the original nodes.
* Native node rest transforms also differ across a family. Each target model is
  exported alone with the same settings and its glTF node TRS is diffed against
  the family GLB's nodes; the differences are written into the GLB JSON as
  extras.chaoMorph on the armature node (space 'gltf-node-local'):
  t = target.t - z.t, q = inverse(z.q) * target.q ([x, y, z, w]), s = target.s / z.s.
* The z model's triangle list is kept for every target (eight attaches in two
  models are re-triangulated natively; positions still correspond 1:1).
* Eyelids (nodes 019/022) and the node-027 mouth parts (al_mouth02/03) are kept
  and tagged for src/chao-face.js instead of being deleted.

--preview renders each target at weight 1 (shape keys plus the node offsets as a
pose) beside the native model rendered alone with the same camera and light, and
records a per-pixel difference in the report.

Everything written here is game-derived and stays under ignored .local/.
"""
import importlib.util
import json
import math
from pathlib import Path
import re
import struct
import sys

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('convert_assets_shared', ROOT / 'scripts/convert-assets.py')
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)  # registers SAIO once; main() is not run

EXTRACTED = shared.EXTRACTED
OUTPUT = shared.OUTPUT
MODELS = EXTRACTED / 'pc-chao/Chao/models/AL_RootObject'
MOTIONS = EXTRACTED / 'pc-chao/Chao/models/al_motion_table'
SUFFIXES = ['z', 'n', 's', 'f', 'r', 'p']
# Children are one GLB (al_ncz) with 17 targets, because the native child blend
# (AL_DeformObjectChild) also lerps between the neutral and hero/dark child sets
# by |APos|. Adults are one GLB per family with the five targets n, s, f, r, p.
CHILD_TARGETS = ([(s, f'al_nc{s}') for s in 'nsfrp'] + [(f'h{s}', f'al_hc{s}') for s in 'znsfrp']
                 + [(f'd{s}', f'al_dc{s}') for s in 'znsfrp'])
SPECS = [('al_ncz', CHILD_TARGETS)] + [(f'al_{a}{f}z', [(s, f'al_{a}{f}{s}') for s in 'nsfrp'])
                                      for f in 'nsfrp' for a in 'nhd']
EYELID_NODES = ('019', '022')
EYE_NODES = ('018', '021')
MOUTH_NODE = '027'
TOLERANCE = 1e-5
NODE_NAME = re.compile(r'^(\d{3})_')
EXPORT_OPTIONS = dict(export_format='GLB', use_active_scene=True, export_vertex_color='ACTIVE', export_extras=True,
                      export_morph=True, export_morph_normal=True, export_def_bones=False)
# SAIO keeps native node scale in the pose bones (a Blender rest bone has no scale), so a
# model's native node TRS is its import pose. Those are read from a separate export of
# the model alone in that pose; the family GLB's bone nodes are then set to the base's.
NODE_EXPORT_OPTIONS = dict(EXPORT_OPTIONS, export_rest_position_armature=False)


def args_after_dashes():
    return sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def node_index(name):
    match = NODE_NAME.match(name or '')
    return match.group(1) if match else None


def import_model(name, with_motions):
    before = set(bpy.context.scene.objects)
    source = MODELS / f'{name}.sa2mdl'
    result = bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}], import_as_armature=True)
    if result != {'FINISHED'}:
        raise RuntimeError(f'Import failed: {source}')
    objects = [obj for obj in bpy.context.scene.objects if obj not in before]
    for obj in [obj for obj in objects if obj.name.startswith('SAIO Bone Shape')]:
        objects.remove(obj)
        bpy.data.objects.remove(obj, do_unlink=True)
    armature = next(obj for obj in objects if obj.type == 'ARMATURE')
    if with_motions:
        load_motions(armature)
    meshes = {node_index(obj.parent_bone): obj for obj in objects if obj.type == 'MESH' and node_index(obj.parent_bone)}
    return source, armature, meshes, objects


def load_motions(armature):
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    for filename, label in [('alm_tati.saanim', 'Idle'), ('alm_aruki_ncz.saanim', 'Walk')]:
        bpy.ops.saio.import_node_anim(filepath=str(MOTIONS / filename), files=[{'name': filename}], short_rot=True)
        if armature.animation_data and armature.animation_data.action:
            armature.animation_data.action.name = label
            armature.animation_data.action.use_fake_user = True


def initial_pose(armature):
    return {bone.name: bone.matrix_basis.copy() for bone in armature.pose.bones}


def bone_frame(armature, bone_name):
    """Native node frame: the bone as SAIO poses it on import (rest plus native scale)."""
    return armature.matrix_world @ armature.pose.bones[bone_name].matrix


def bones_by_index(armature):
    return {node_index(bone.name): bone.name for bone in armature.data.bones if node_index(bone.name)}


def bone_rest(armature, bone_name):
    return armature.matrix_world @ armature.data.bones[bone_name].matrix_local


def local_rest(armature, bone_name):
    bone = armature.data.bones[bone_name]
    return bone.parent.matrix_local.inverted() @ bone.matrix_local if bone.parent else bone.matrix_local


def node_local_coords(armature, mesh):
    to_node = bone_frame(armature, mesh.parent_bone).inverted() @ mesh.matrix_world
    return [to_node @ v.co for v in mesh.data.vertices]


def export_native_nodes(objects, path):
    select_only([obj for obj in objects if obj.name in bpy.context.scene.objects and obj.type in ('ARMATURE', 'MESH')])
    bpy.ops.export_scene.gltf(filepath=str(path), use_selection=True, export_animations=False, **NODE_EXPORT_OPTIONS)
    document, _ = read_glb(path)
    path.unlink()
    return gltf_bone_nodes(document)


def select_only(objects):
    bpy.ops.object.select_all(action='DESELECT')
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = next((obj for obj in objects if obj.type == 'ARMATURE'), objects[0])


# --- GLB JSON helpers -------------------------------------------------------------------------------

def read_glb(path):
    data = path.read_bytes()
    magic, version, _ = struct.unpack_from('<III', data, 0)
    if magic != 0x46546C67 or version != 2:
        raise RuntimeError(f'{path}: not a glTF 2 GLB')
    length, kind = struct.unpack_from('<II', data, 12)
    if kind != 0x4E4F534A:
        raise RuntimeError(f'{path}: first chunk is not JSON')
    document = json.loads(data[20:20 + length].decode('utf-8'))
    return document, data[20 + length:]


def write_glb(path, document, rest):
    payload = json.dumps(document, separators=(',', ':')).encode('utf-8')
    payload += b' ' * (-len(payload) % 4)
    header = struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(payload) + len(rest))
    path.write_bytes(header + struct.pack('<II', len(payload), 0x4E4F534A) + payload + rest)


def gltf_bone_nodes(document):
    nodes = {}
    for node in document.get('nodes', []):
        index = node_index(node.get('name'))
        if index and 'mesh' not in node:
            if index in nodes:
                raise RuntimeError(f'duplicate glTF node index {index}')
            nodes[index] = node
    return nodes


def drop_unit_scale_channels(document, binary):
    """Remove animation scale channels that only hold (1, 1, 1).

    The SAIO motions key a unit scale on every node. Native motions without scale
    leave the node's own scale (the eye parents' 0.3 depth, for example), so a
    unit channel would flatten it. Samplers are compacted to stay valid.
    """
    base = 8  # binary chunk header (length, type) precedes the payload
    removed = 0
    for animation in document.get('animations', []):
        keep = []
        for channel in animation['channels']:
            if channel['target']['path'] == 'scale':
                accessor = document['accessors'][animation['samplers'][channel['sampler']]['output']]
                view = document['bufferViews'][accessor['bufferView']]
                offset = base + view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
                stride = view.get('byteStride', 12)
                values = [struct.unpack_from('<3f', binary, offset + stride * i) for i in range(accessor['count'])]
                if all(abs(c - 1.0) <= 1e-6 for value in values for c in value):
                    removed += 1
                    continue
            keep.append(channel)
        used = sorted({channel['sampler'] for channel in keep})
        remap = {old: new for new, old in enumerate(used)}
        animation['samplers'] = [animation['samplers'][old] for old in used]
        for channel in keep:
            channel['sampler'] = remap[channel['sampler']]
        animation['channels'] = keep
    return removed


def trs(node):
    return (node.get('translation', [0.0, 0.0, 0.0]), node.get('rotation', [0.0, 0.0, 0.0, 1.0]),
            node.get('scale', [1.0, 1.0, 1.0]))


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz]


def quat_inverse(q):
    norm = sum(c * c for c in q) or 1.0
    return [-q[0] / norm, -q[1] / norm, -q[2] / norm, q[3] / norm]


def node_offsets(z_nodes, target_nodes):
    offsets = {}
    for index, z_node in sorted(z_nodes.items()):
        if index not in target_nodes:
            raise RuntimeError(f'target is missing node {index}')
        zt, zq, zs = trs(z_node)
        tt, tq, ts = trs(target_nodes[index])
        dt = [b - a for a, b in zip(zt, tt)]
        dq = quat_mul(quat_inverse(zq), tq)
        if dq[3] < 0:
            dq = [-c for c in dq]
        ds = [b / a if abs(a) > 1e-9 else 1.0 for a, b in zip(zs, ts)]
        angle = 2 * math.acos(min(1.0, abs(dq[3])))
        if max(abs(c) for c in dt) > TOLERANCE or angle > TOLERANCE or any(abs(s - 1) > TOLERANCE for s in ds):
            offsets[z_node['name']] = {'t': dt, 'q': dq, 's': ds}
    return offsets


# --- conversion -------------------------------------------------------------------------------------

def add_targets(z_armature, z_meshes, base, targets):
    report = {'targets': {}, 'gltfTargetNodes': {}, 'poseBasis': {}}
    z_bones = bones_by_index(z_armature)
    # Every rig here carries only SAIO's import pose (motions are loaded after this), so
    # vertices are measured in each model's native node frame.
    for suffix, name in targets:
        _, armature, meshes, objects = import_model(name, with_motions=False)
        if set(meshes) != set(z_meshes):
            raise RuntimeError(f'{name}: attach nodes {sorted(meshes)} differ from {base} {sorted(z_meshes)}')
        t_bones = bones_by_index(armature)
        bpy.context.view_layer.update()
        if set(t_bones) != set(z_bones):
            raise RuntimeError(f'{name}: node indices differ from {base}')
        maximum = 0.0
        for index, z_mesh in z_meshes.items():
            target = node_local_coords(armature, meshes[index])
            if len(target) != len(z_mesh.data.vertices):
                raise RuntimeError(f'{name} node {index}: vertex count {len(target)} != {len(z_mesh.data.vertices)}')
            if not z_mesh.data.shape_keys:
                z_mesh.shape_key_add(name='Basis', from_mix=False)
            key = z_mesh.shape_key_add(name=suffix, from_mix=False)
            key.value = 0.0
            to_mesh = z_mesh.matrix_world.inverted() @ bone_frame(z_armature, z_mesh.parent_bone)
            for i, co in enumerate(target):
                key.data[i].co = to_mesh @ co
                # Node-local delta: the same distance measured in the z node's frame.
                maximum = max(maximum, (co - (bone_frame(z_armature, z_mesh.parent_bone).inverted()
                                              @ z_mesh.matrix_world @ z_mesh.data.vertices[i].co)).length)
        # Pose basis that turns the z rest into this target's rest (used by the preview only).
        t_pose = initial_pose(armature)
        report['poseBasis'][suffix] = {z_bones[i]: local_rest(z_armature, z_bones[i]).inverted() @ local_rest(armature, t_bones[i])
                                       @ t_pose[t_bones[i]] for i in z_bones}
        # Export the target alone with the family settings to read its glTF node TRS.
        report['gltfTargetNodes'][suffix] = export_native_nodes(objects, OUTPUT / f'chao-body-{name}.nodes.glb')
        report['targets'][suffix] = {'model': name, 'maxVertexDelta': maximum}
        for obj in objects:
            bpy.data.objects.remove(obj, do_unlink=True)
    return report


def tag_face_parts(z_meshes):
    """Tag eyes/eyelids and split node-027 mouth parts by original texture."""
    tagged = {'eyes': [], 'eyelids': [], 'mouths': []}
    for index, mesh in sorted(z_meshes.items()):
        if index in EYE_NODES:
            mesh['chaoFace'] = 'eye'
            tagged['eyes'].append(mesh.name)
        elif index in EYELID_NODES:
            mesh['chaoFace'] = 'eyelid'
            mesh['lidSide'] = 'left' if index == '019' else 'right'
            tagged['eyelids'].append(mesh.name)
        elif index == MOUTH_NODE:
            select_only([mesh])
            bpy.context.view_layer.objects.active = mesh
            bpy.ops.object.mode_set(mode='EDIT')
            bpy.ops.mesh.select_all(action='SELECT')
            bpy.ops.mesh.separate(type='MATERIAL')
            bpy.ops.object.mode_set(mode='OBJECT')
            for part in [obj for obj in bpy.context.selected_objects if obj.type == 'MESH']:
                image = next((node.image.name for slot in part.material_slots if slot.material and slot.material.node_tree
                              for node in slot.material.node_tree.nodes if node.type == 'TEX_IMAGE' and node.image), None)
                part['chaoFace'] = 'mouth'
                part['mouthTexture'] = Path(image).stem if image else None
                tagged['mouths'].append({'mesh': part.name, 'texture': part['mouthTexture']})
    return tagged


def verify_glb(document, armature_name, offsets, names_expected):
    meshes = document.get('meshes', [])
    names = [mesh.get('extras', {}).get('targetNames') for mesh in meshes]
    face_nodes = [{'node': node.get('name'), **{k: v for k, v in node.get('extras', {}).items() if k in ('chaoFace', 'mouthTexture', 'lidSide')}}
                  for node in document.get('nodes', []) if 'chaoFace' in node.get('extras', {})]
    root = next((node for node in document['nodes'] if node.get('name') == armature_name), None)
    return {
        'meshCount': len(meshes),
        'meshesWithTargetNames': sum(1 for n in names if n == names_expected),
        'meshesWithAllTargets': sum(1 for mesh in meshes if all(len(p.get('targets', [])) == len(names_expected) for p in mesh['primitives'])),
        'faceNodes': face_nodes,
        'armatureNodeHasChaoMorph': bool(root and isinstance(root.get('extras', {}).get('chaoMorph'), dict)),
        'animations': [a.get('name') for a in document.get('animations', [])],
        'nodeOffsetCounts': {suffix: len(offsets[suffix]) for suffix in offsets},
    }


# --- preview ----------------------------------------------------------------------------------------

def preview_rig():
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12
    scene.cycles.seed = 0
    scene.render.resolution_x = scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.world.color = (0.3, 0.3, 0.3)
    camera = bpy.data.objects.get('Evolution Preview Camera')
    if not camera:
        camera = bpy.data.objects.new('Evolution Preview Camera', bpy.data.cameras.new('Evolution Preview Camera'))
        scene.collection.objects.link(camera)
        camera.location = (7, -13, 6)
        camera.rotation_euler = (Vector((0, 0, 2.4)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.type = 'ORTHO'
        camera.data.ortho_scale = 6.8
        lamp = bpy.data.objects.new('Evolution Preview Light', bpy.data.lights.new('Evolution Preview Light', 'AREA'))
        lamp.data.energy, lamp.data.shape, lamp.data.size = 1200, 'DISK', 8
        scene.collection.objects.link(lamp)
        lamp.location = (3, -7, 9)
        lamp.rotation_euler = (Vector((0, 0, 2)) - lamp.location).to_track_quat('-Z', 'Y').to_euler()
    scene.camera = camera


def render(path):
    preview_rig()
    scene = bpy.context.scene
    scene.render.image_settings.file_format = 'PNG'
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def image_difference(a, b):
    first = bpy.data.images.load(str(a))
    second = bpy.data.images.load(str(b))
    pa, pb = list(first.pixels), list(second.pixels)
    bpy.data.images.remove(first)
    bpy.data.images.remove(second)
    diffs = [abs(x - y) for x, y in zip(pa, pb)]
    channels = [d for i, d in enumerate(diffs) if i % 4 != 3]
    return {'meanAbs': sum(channels) / len(channels), 'max': max(channels),
            'pixelsOver10Percent': sum(1 for i in range(0, len(diffs), 4) if max(diffs[i:i + 3]) > 0.1)}


def render_previews(name, targets, armature, z_meshes, morph, z_objects):
    results = {}
    # The family GLB is already written. Drop the rig's actions/NLA so the preview shows
    # the rest pose plus each target's node offsets, like the native model drawn alone.
    armature.animation_data_clear()
    shape_meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH' and obj.data.shape_keys]
    for suffix, model in targets:
        for mesh in shape_meshes:
            for block in mesh.data.shape_keys.key_blocks[1:]:
                block.value = 1.0 if block.name == suffix else 0.0
        for bone_name, basis in morph['poseBasis'][suffix].items():
            armature.pose.bones[bone_name].matrix_basis = basis
        bpy.context.view_layer.update()
        morphed = OUTPUT / f'{name}-{suffix}-preview.png'
        render(morphed)
        # The native target alone, same camera and light, z model hidden.
        for obj in z_objects:
            obj.hide_render = True
        _, _, _, objects = import_model(model, with_motions=False)
        shared.texture_materials('al_body')
        native = OUTPUT / f'chao-native-{model}-preview.png'
        render(native)
        for obj in objects:
            bpy.data.objects.remove(obj, do_unlink=True)
        for obj in z_objects:
            obj.hide_render = False
        results[suffix] = {'morphed': morphed.name, 'native': native.name, **image_difference(morphed, native)}
    for mesh in shape_meshes:
        for block in mesh.data.shape_keys.key_blocks[1:]:
            block.value = 0.0
    for bone in armature.pose.bones:
        bone.matrix_basis.identity()
    return results


def convert_family(base, targets, preview):
    shared.clean_scene()
    names = [suffix for suffix, _ in targets]
    source, armature, z_meshes, z_objects = import_model(base, with_motions=False)
    z_pose = initial_pose(armature)
    z_native = export_native_nodes(z_objects, OUTPUT / f'chao-body-{base}.nodes.glb')
    morph = add_targets(armature, z_meshes, base, targets)
    load_motions(armature)
    # Export from SAIO's import pose (the native rest); the actions are exported as clips.
    armature.animation_data.action = None
    for bone in armature.pose.bones:
        bone.matrix_basis = z_pose[bone.name]
    bpy.context.view_layer.update()
    materials = shared.texture_materials('al_body')
    face = tag_face_parts(z_meshes)
    name = f'chao-body-{base}'
    target = OUTPUT / f'{name}.glb'
    temporary = OUTPUT / f'{name}.converting.glb'
    select_only([obj for obj in bpy.context.scene.objects if obj.type in ('ARMATURE', 'MESH')])
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_animations=True, **EXPORT_OPTIONS)
    document, rest = read_glb(temporary)
    z_nodes = gltf_bone_nodes(document)
    if set(z_nodes) != set(z_native):
        raise RuntimeError(f'{name}: family GLB bone nodes differ from the native node export')
    for index, node in z_nodes.items():
        for key in ('translation', 'rotation', 'scale'):
            node.pop(key, None)
            if key in z_native[index]:
                node[key] = z_native[index][key]
    offsets = {suffix: node_offsets(z_native, morph['gltfTargetNodes'][suffix]) for suffix in names}
    unit_scale_channels = drop_unit_scale_channels(document, rest)
    root = next((node for node in document['nodes'] if node.get('name') == armature.name), None)
    if root is None:
        raise RuntimeError(f'{name}: armature node {armature.name} not in GLB')
    root.setdefault('extras', {})['chaoMorph'] = {'family': base, 'targets': names,
                                                  'nodeOffsets': offsets, 'space': 'gltf-node-local'}
    write_glb(temporary, document, rest)
    shared.replace_asset(temporary, target)
    checked, _ = read_glb(target)
    verification = verify_glb(checked, armature.name, offsets, names)
    previews = None
    if preview:
        z_objects = [obj for obj in bpy.context.scene.objects if obj.type in ('ARMATURE', 'MESH')]
        previews = render_previews(name, targets, armature, z_meshes, morph, z_objects)
    return {'family': base, 'targetModels': dict(targets), 'source': str(source.relative_to(ROOT)), 'output': str(target.relative_to(ROOT)),
            'bytes': target.stat().st_size, 'meshes': len(z_meshes), 'targets': morph['targets'],
            'nodeOffsets': offsets, 'droppedUnitScaleChannels': unit_scale_channels, 'face': face, 'verification': verification, 'previews': previews,
            'materials': materials}


def export_face_textures():
    """Copy original eye images; child-style pupils get the existing matte cutout."""
    folder = EXTRACTED / 'textures/al_eye'
    names = [row.split(',')[1] for row in (folder / 'index.txt').read_text().splitlines() if ',' in row]
    eyes = []
    for filename in names:
        stem = Path(filename).stem
        cut = shared.normal_eye_cutout(folder / filename)
        out = OUTPUT / f'chao-eye-{stem.lower()}.png'
        out.write_bytes(cut.read_bytes())
        eyes.append(out.name)
    return eyes


def main():
    options = args_after_dashes()
    only = options[options.index('--only') + 1] if '--only' in options else None
    preview = '--preview' in options
    reports = []
    for base, targets in SPECS:
        if only and base != only:
            continue
        reports.append(convert_family(base, targets, preview))
    eye_files = export_face_textures()
    # eyeFiles follows the al_eye archive order (index 0..12), which the native eye tables index.
    # eyelid: the exported eyelid mesh nodes' local frames equal the native node-local frames
    # (glTF vertices == native vertices), so native njRotateZ(+SlopeAng)·njRotateX(CloseAng) on
    # node 019 are node-space z then x with positive sign (node 022 negates the slope). Checked by
    # render: CloseAng 0 is the modelled half lid, +0x4000 fully closed, eye 10 (75 deg, slope 20)
    # gives the angled lids. Mouth texture mappings stay unresolved.
    face_manifest = {'eyeFiles': eye_files,
                     'eyelid': {'closeAxis': 'x', 'closeSign': 1, 'slopeAxis': 'z', 'slopeSign': 1,
                                'space': 'eyelid mesh node local (native node local)'},
                     'source': 'al_eye.prs archive order; al_draw_B 0x8CD4-0x8EA8'}
    (OUTPUT / 'chao-face.json').write_text(json.dumps(face_manifest, indent=2) + '\n')
    (OUTPUT / 'chao-evolution-report.json').write_text(json.dumps(reports, indent=2) + '\n')
    print('SA2_CHAO_EVOLUTION=' + json.dumps([
        {'family': r['family'], 'bytes': r['bytes'],
         'maxVertexDelta': {s: round(t['maxVertexDelta'], 5) for s, t in r['targets'].items()},
         'nodeOffsetCounts': r['verification']['nodeOffsetCounts'],
         'meshesWithTargetNames': r['verification']['meshesWithTargetNames'], 'meshCount': r['verification']['meshCount'],
         'previews': {s: round(p['meanAbs'], 5) for s, p in (r['previews'] or {}).items()}} for r in reports]))


if __name__ == '__main__':
    main()
