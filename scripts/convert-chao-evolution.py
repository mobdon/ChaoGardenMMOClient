"""Convert the native child/adult Chao body families into morphing GLBs.

Run with portable Blender (same setup as scripts/convert-assets.py):

    $env:BLENDER_USER_RESOURCES = Join-Path (Get-Location) '.local\\blender-user'
    & '.local\\tools\\blender-5.1.2-windows-x64\\blender.exe' --background --factory-startup `
        --python 'scripts\\convert-chao-evolution.py' [-- --preview] [-- --only al_nnz]

SA Tools PC_SA2/Chao_DLL.ini orders AL_RootObject[0..107] as 18 families of six
models: z, n, s, f, r, p (child nc/hc/dc, then adult nn hn dn, ns hs ds, nf hf df,
nr hr dr, np hp dp). Each family shares one 40-node tree and per-attach vertex
counts (verified on the local extraction), so the five non-z models become glTF
morph targets of the z model:

* Vertex targets are expressed in each node's local space, so the rig and native
  proportions are preserved and animation still drives the original nodes.
* Native node rest transforms also differ across a family. Those differences are
  written to the GLB as extras (`chaoMorphNodes`) for the renderer to blend.
* The z model's triangle list is kept for every target (eight attaches in two
  models are re-triangulated natively; positions still correspond 1:1).
* Eyelids (nodes 019/022) and the node-027 mouth parts (al_mouth02/03) are kept
  and tagged for src/chao-face.js instead of being deleted.

Everything written here is game-derived and stays under ignored .local/.
"""
import importlib.util
import json
from pathlib import Path
import sys

import bpy
from mathutils import Matrix

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('convert_assets_shared', ROOT / 'scripts/convert-assets.py')
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)  # registers SAIO once; main() is not run

EXTRACTED = shared.EXTRACTED
OUTPUT = shared.OUTPUT
MODELS = EXTRACTED / 'pc-chao/Chao/models/AL_RootObject'
MOTIONS = EXTRACTED / 'pc-chao/Chao/models/al_motion_table'
SUFFIXES = ['z', 'n', 's', 'f', 'r', 'p']
FAMILIES = [f'al_{a}{f}' for f in 'cnsfrp' for a in 'nhd']
EYELID_NODES = ('019_', '022_')
EYE_NODES = ('018_', '021_')
MOUTH_NODE = '027_'
TOLERANCE = 1e-5


def args_after_dashes():
    return sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []


def import_model(name, with_motions):
    before = set(bpy.context.scene.objects)
    source = MODELS / f'{name}.sa2mdl'
    result = bpy.ops.saio.import_mdl(filepath=str(source), files=[{'name': source.name}], import_as_armature=True)
    if result != {'FINISHED'}:
        raise RuntimeError(f'Import failed: {source}')
    objects = [obj for obj in bpy.context.scene.objects if obj not in before]
    armature = next(obj for obj in objects if obj.type == 'ARMATURE')
    if with_motions:
        bpy.context.view_layer.objects.active = armature
        bpy.ops.object.select_all(action='DESELECT')
        armature.select_set(True)
        for filename, label in [('alm_tati.saanim', 'Idle'), ('alm_aruki_ncz.saanim', 'Walk')]:
            bpy.ops.saio.import_node_anim(filepath=str(MOTIONS / filename), files=[{'name': filename}], short_rot=True)
            if armature.animation_data and armature.animation_data.action:
                armature.animation_data.action.name = label
                armature.animation_data.action.use_fake_user = True
    meshes = {obj.parent_bone: obj for obj in objects if obj.type == 'MESH' and obj.parent_bone}
    return source, armature, meshes, objects


def bone_rest(armature, bone_name):
    return armature.matrix_world @ armature.data.bones[bone_name].matrix_local


def local_rest(armature, bone_name):
    bone = armature.data.bones[bone_name]
    return bone.parent.matrix_local.inverted() @ bone.matrix_local if bone.parent else bone.matrix_local


def node_local_coords(armature, mesh):
    to_node = bone_rest(armature, mesh.parent_bone).inverted() @ mesh.matrix_world
    return [to_node @ v.co for v in mesh.data.vertices]


def add_targets(z_armature, z_meshes, family):
    report = {'targets': {}, 'nodeOffsets': {}}
    for suffix in SUFFIXES[1:]:
        name = f'{family}{suffix}'
        _, armature, meshes, objects = import_model(name, with_motions=False)
        if set(meshes) != set(z_meshes):
            raise RuntimeError(f'{name}: attach nodes differ from {family}z')
        maximum = 0.0
        for bone_name, z_mesh in z_meshes.items():
            target = node_local_coords(armature, meshes[bone_name])
            if len(target) != len(z_mesh.data.vertices):
                raise RuntimeError(f'{name} {bone_name}: vertex count {len(target)} != {len(z_mesh.data.vertices)}')
            if not z_mesh.data.shape_keys:
                z_mesh.shape_key_add(name='Basis', from_mix=False)
            key = z_mesh.shape_key_add(name=suffix, from_mix=False)
            to_mesh = z_mesh.matrix_world.inverted() @ bone_rest(z_armature, bone_name)
            for index, co in enumerate(target):
                key.data[index].co = to_mesh @ co
                maximum = max(maximum, (key.data[index].co - z_mesh.data.vertices[index].co).length)
        offsets = {}
        for bone in z_armature.data.bones:
            if bone.name not in armature.data.bones:
                raise RuntimeError(f'{name}: missing node {bone.name}')
            z_local, t_local = local_rest(z_armature, bone.name), local_rest(armature, bone.name)
            dt = t_local.to_translation() - z_local.to_translation()
            dq = z_local.to_quaternion().rotation_difference(t_local.to_quaternion())
            ds = [t / z if abs(z) > 1e-9 else 1.0 for t, z in zip(t_local.to_scale(), z_local.to_scale())]
            if dt.length > TOLERANCE or dq.angle > TOLERANCE or any(abs(s - 1) > TOLERANCE for s in ds):
                # Blender bone space; the renderer maps it with the same SAIO axis convention as the rig.
                offsets[bone.name] = {'t': list(dt), 'q': [dq.w, dq.x, dq.y, dq.z], 's': ds}
        report['targets'][suffix] = {'model': name, 'maxVertexDelta': maximum}
        report['nodeOffsets'][suffix] = offsets
        for obj in objects:
            bpy.data.objects.remove(obj, do_unlink=True)
    return report


def tag_face_parts(z_meshes):
    """Tag eyes/eyelids and split node-027 mouth parts by original texture."""
    tagged = {'eyes': [], 'eyelids': [], 'mouths': []}
    for bone_name, mesh in z_meshes.items():
        if bone_name.startswith(EYE_NODES):
            mesh['chaoFace'] = 'eye'
            tagged['eyes'].append(mesh.name)
        elif bone_name.startswith(EYELID_NODES):
            mesh['chaoFace'] = 'eyelid'
            tagged['eyelids'].append(mesh.name)
        elif bone_name.startswith(MOUTH_NODE):
            bpy.ops.object.select_all(action='DESELECT')
            mesh.select_set(True)
            bpy.context.view_layer.objects.active = mesh
            bpy.ops.object.mode_set(mode='EDIT')
            bpy.ops.mesh.separate(type='MATERIAL')
            bpy.ops.object.mode_set(mode='OBJECT')
            for part in [obj for obj in bpy.context.selected_objects if obj.type == 'MESH']:
                image = next((node.image.name for slot in part.material_slots if slot.material and slot.material.node_tree
                              for node in slot.material.node_tree.nodes if node.type == 'TEX_IMAGE' and node.image), None)
                part['chaoFace'] = 'mouth'
                part['mouthTexture'] = Path(image).stem if image else None
                tagged['mouths'].append({'mesh': part.name, 'texture': part['mouthTexture']})
    return tagged


def convert_family(family, preview):
    shared.clean_scene()
    source, armature, z_meshes, _ = import_model(f'{family}z', with_motions=True)
    for obj in list(bpy.context.scene.objects):
        if obj.name.startswith('SAIO Bone Shape'):
            bpy.data.objects.remove(obj, do_unlink=True)
    morph = add_targets(armature, z_meshes, family)
    materials = shared.texture_materials('al_body')
    face = tag_face_parts(z_meshes)
    armature['chaoMorph'] = json.dumps({'family': family, 'targets': SUFFIXES[1:],
                                        'nodeOffsets': morph['nodeOffsets'], 'space': 'blender-bone-local'})
    name = f'chao-body-{family}z'
    target = OUTPUT / f'{name}.glb'
    temporary = OUTPUT / f'{name}.converting.glb'
    bpy.ops.export_scene.gltf(filepath=str(temporary), export_format='GLB', export_animations=True,
                              use_active_scene=True, export_vertex_color='ACTIVE', export_extras=True,
                              export_morph=True, export_morph_normal=True, export_def_bones=False)
    shared.replace_asset(temporary, target)
    if preview:
        for suffix in SUFFIXES[1:]:
            for mesh in z_meshes.values():
                if mesh.data.shape_keys:
                    for block in mesh.data.shape_keys.key_blocks[1:]:
                        block.value = 1.0 if block.name == suffix else 0.0
            shared.render_chao_preview(f'{name}-{suffix}')
    return {'family': family, 'source': str(source.relative_to(ROOT)), 'output': str(target.relative_to(ROOT)),
            'bytes': target.stat().st_size, 'meshes': len(z_meshes), **morph, 'face': face,
            'materials': materials}


def export_face_textures():
    """Copy original eye images; child-style pupils get the existing matte cutout."""
    folder = EXTRACTED / 'textures/al_eye'
    names = [row.split(',')[1] for row in (folder / 'index.txt').read_text().splitlines() if ',' in row]
    eyes = {}
    for filename in names:
        stem = Path(filename).stem
        cut = shared.normal_eye_cutout(folder / filename)
        out = OUTPUT / f'chao-eye-{stem.lower()}.png'
        out.write_bytes(cut.read_bytes())
        eyes[stem] = out.name
    return eyes


def main():
    options = args_after_dashes()
    only = options[options.index('--only') + 1] if '--only' in options else None
    preview = '--preview' in options
    reports = []
    for family in FAMILIES:
        if only and f'{family}z' != only:
            continue
        reports.append(convert_family(family, preview))
    eye_files = export_face_textures()
    # Enum -> texture assignments stay empty until traced from the native face code.
    face_manifest = {'eyeFiles': eye_files, 'eyes': {}, 'mouths': {}, 'eyelidClosedAngle': None,
                     'source': 'al_eye.prs, al_body.prs node 027', 'mapping': 'unresolved'}
    (OUTPUT / 'chao-face.json').write_text(json.dumps(face_manifest, indent=2) + '\n')
    (OUTPUT / 'chao-evolution-report.json').write_text(json.dumps(reports, indent=2) + '\n')
    print('SA2_CHAO_EVOLUTION=' + json.dumps([{k: r[k] for k in ('family', 'output', 'bytes', 'targets')} for r in reports]))


if __name__ == '__main__':
    main()
