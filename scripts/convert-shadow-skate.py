"""Sample genuine Shadow motions without modifying or loading a Chao rig.

Portable Blender --background --factory-startup --python this-file.
Run extract-shadow-skate.js first. Game-derived output remains in .local/.
"""
import hashlib
import importlib.util
import json
import math
from pathlib import Path

import bpy
from mathutils import Matrix

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('shadow_skate_import_helpers', ROOT / 'scripts/convert-assets.py')
pipeline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pipeline)
manifest = json.loads((ROOT / '.local/extracted/shadow-skate/manifest.json').read_text())
OUTPUT = ROOT / '.local/imports/shadow-skate-motion.json'

SEMANTICS = {
    'body': 4, 'head': 44,
    'leftUpperArm': 37, 'leftForearm': 38, 'leftHand': 40,
    'rightUpperArm': 56, 'rightForearm': 57, 'rightHand': 59,
    'rightThigh': 7, 'rightShin': 9, 'rightFoot': 13, 'rightToe': 14,
    'leftThigh': 17, 'leftShin': 19, 'leftFoot': 23, 'leftToe': 24,
}
CONVERSION = Matrix(((1, 0, 0, 0), (0, 0, 1, 0), (0, -1, 0, 0), (0, 0, 0, 1)))
CONVERSION_INV = CONVERSION.inverted()


def index(bone):
    return int(bone.name.split('_')[0])


def position(matrix):
    return [round(float(value), 8) for value in matrix.translation]


def quaternion(matrix, previous=None):
    q = matrix.to_quaternion().normalized()
    values = [q.x, q.y, q.z, q.w]
    if previous is not None and sum(a*b for a, b in zip(values, previous)) < 0:
        values = [-value for value in values]
    return [round(float(value), 9) for value in values]


def digest(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


pipeline.clean_scene()
model = ROOT / manifest['nativeModel']
if digest(model) != manifest['nativeModelSha256']:
    raise RuntimeError('Extracted Shadow model changed')
result = bpy.ops.saio.import_mdl(filepath=str(model), files=[{'name': model.name}], import_as_armature=True)
if result != {'FINISHED'}:
    raise RuntimeError('Native Shadow model import failed')
armature = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
bpy.context.view_layer.objects.active = armature
bpy.ops.object.select_all(action='DESELECT')
armature.select_set(True)
bpy.context.scene.render.fps = 60
bpy.context.view_layer.update()
initial_rig = [(bone.name, bone.parent.name if bone.parent else None, list(bone.matrix_local)) for bone in armature.data.bones]
source_bones = []
for bone in armature.data.bones:
    world = CONVERSION @ (armature.matrix_world @ bone.matrix_local) @ CONVERSION_INV
    source_bones.append({'nodeId': index(bone), 'name': bone.name,
                         'parentNodeId': index(bone.parent) if bone.parent else None,
                         'restWorld': quaternion(world), 'restPosition': position(world)})
source_by_id = {bone['nodeId']: bone for bone in source_bones}
bones = [{'semantic': semantic, **source_by_id[node]} for semantic, node in SEMANTICS.items()]
pose_by_id = {index(bone): bone for bone in armature.pose.bones}
clips = []
for record in manifest['clips']:
    native = ROOT / record['nativeFile']
    if digest(native) != record['sha256']:
        raise RuntimeError(f'Extracted native motion changed: {native}')
    imported = bpy.ops.saio.import_node_anim(filepath=str(native), files=[{'name': native.name}], short_rot=False)
    if imported != {'FINISHED'}:
        raise RuntimeError(f'Native motion import failed: {native}')
    action = armature.animation_data.action
    if action is None:
        raise RuntimeError(f'Missing action: {native}')
    # Blender5 layered actions require the correct slot, not action alone.
    armature.animation_data.action_slot = action.slots[0]
    for track in list(armature.animation_data.nla_tracks):
        armature.animation_data.nla_tracks.remove(track)
    start, end = (int(round(value)) for value in action.frame_range)
    samples = []
    previous = {bone['nodeId']: bone['restWorld'] for bone in source_bones}
    for frame in range(start, end + 1):
        bpy.context.scene.frame_set(frame)
        bpy.context.view_layer.update()
        rotations_by_node, positions_by_node = {}, {}
        for node, bone in pose_by_id.items():
            world = CONVERSION @ (armature.matrix_world @ bone.matrix) @ CONVERSION_INV
            rotation = quaternion(world, previous[node])
            if not all(math.isfinite(value) for value in rotation + position(world)):
                raise RuntimeError(f'Non-finite native sample: {record["id"]}/{frame}/{node}')
            previous[node] = rotation
            rotations_by_node[str(node)] = rotation
            positions_by_node[str(node)] = position(world)
        samples.append({'timeSeconds': round((frame-start)/60, 9),
                        'rotations': {semantic: rotations_by_node[str(node)] for semantic, node in SEMANTICS.items()},
                        'positions': {semantic: positions_by_node[str(node)] for semantic, node in SEMANTICS.items()},
                        'rotationsByNode': rotations_by_node, 'positionsByNode': positions_by_node})
    moving = [node for node in pose_by_id if any(abs(a-b) > 1e-5 for sample in samples[1:] for a, b in zip(samples[0]['rotationsByNode'][str(node)], sample['rotationsByNode'][str(node)]))]
    if record['role'] == 'skate-cycle' and len(moving) < 8:
        raise RuntimeError(f'Expected native skating movement: {record["id"]}')
    clips.append({'nativeId': record['id'], 'name': record['description'], 'nativeName': record['nativeName'],
                  'source': record['nativeFile'], 'sourceSha256': record['sha256'], 'role': record['role'],
                  'fps': 60, 'frameCount': len(samples), 'sourceFrameStart': start, 'sourceFrameEnd': end,
                  'durationSeconds': len(samples)/60, 'nativeStates': record['nativeStates'],
                  'movingSourceNodes': moving, 'frames': samples})
    print('SHADOW_SKATE_SAMPLED=' + json.dumps({'nativeId': record['id'], 'frames': len(samples), 'movingNodes': len(moving)}), flush=True)
if initial_rig != [(bone.name, bone.parent.name if bone.parent else None, list(bone.matrix_local)) for bone in armature.data.bones]:
    raise RuntimeError('Source rig changed during sampling')
document = {
    'version': 1, 'basis': 'right-handed glTF Y-up model space', 'fps': 60,
    'quaternionOrder': 'xyzw', 'sourceForward': [-1, 0, 0], 'sourceUp': [0, 1, 0],
    'alignmentQuaternion': [0, math.sqrt(0.5), 0, math.sqrt(0.5)],
    'sourceModel': manifest['nativeModel'], 'sourceModelSha256': manifest['nativeModelSha256'],
    'sources': manifest['sources'], 'sourceBones': source_bones, 'bones': bones, 'clips': clips,
    'retargeting': 'A * sampledSourceWorld * inverse(sourceRestWorld) * inverse(A), applied to targetRestWorld; resolve desired target world against animated parent. Preserve every target local position, scale, hierarchy, weight and mesh.',
    'semanticEvidence': 'Numeric source hierarchy and bind-space limb positions; forward verified from native heel-to-toe direction. Semantic labels are retargeting correspondences, not claims of native semantic bone names.',
    'limits': ['Sampled native Shadow rotations and positions; Chao retargeting, amplitude limits and movement threshold/cadence remain deliberate runtime adaptations.',
               'Reference samples at60Hz do not establish fixed native playback. Preserve the independently decoded AnimationInfo states and distinguish state8/9/10 from direct172/173/174.',
               'World positions are source audit/reference data only; never copy Shadow limb lengths or root translation into a Chao.',
               'Skating character animations do not grant speed, acceleration, flight, collision, gear ownership or any server authority.',
               'Use native Chao idle/walk/run before the speed threshold and retain native Chao air/mech actions.'],
}
OUTPUT.write_text(json.dumps(document, separators=(',', ':')) + '\n')
report = {'version': 1, 'output': str(OUTPUT.relative_to(ROOT)), 'bytes': OUTPUT.stat().st_size,
          'sha256': digest(OUTPUT), 'sourceBoneCount': len(source_bones), 'semanticBoneCount': len(bones),
          'clips': [{key: value for key, value in clip.items() if key != 'frames'} for clip in clips],
          'rigUnchanged': True, 'chaoRigLoadedOrModified': False}
(OUTPUT.parent / 'shadow-skate-conversion-report.json').write_text(json.dumps(report, indent=2) + '\n')
print('SHADOW_SKATE_COMPLETE=' + json.dumps({key: value for key, value in report.items() if key != 'clips'}), flush=True)
