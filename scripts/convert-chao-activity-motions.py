"""Convert every extracted Chao activity clip into separate local action libraries.

Portable Blender --background --factory-startup --python this-file.
Run scripts/extract-chao-activity-motions.js first. Existing live models are
never replaced; all exports are under .local/imports/activity-motions/.
"""
import importlib.util
import json
from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('chao_native_pipeline', ROOT / 'scripts/convert-assets.py')
pipeline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pipeline)
ACTIVITY = ROOT / '.local/extracted/activity-motions'
OUTPUT = ROOT / '.local/imports/activity-motions'
OUTPUT.mkdir(parents=True, exist_ok=True)
pipeline.OUTPUT = OUTPUT
manifest = json.loads((ACTIVITY / 'manifest.json').read_text())
records = []


def convert_library(name, native_model, clips, texture_pack, short_rot):
    pipeline.clean_scene()
    result = bpy.ops.saio.import_mdl(filepath=str(native_model), files=[{'name': native_model.name}], import_as_armature=True)
    if result != {'FINISHED'}:
        raise RuntimeError(f'Native model import failed: {native_model}')
    armature = next((obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE'), None)
    if armature is None:
        raise RuntimeError(f'Expected source model armature: {native_model}')
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    bpy.context.scene.render.fps = 60
    actions = []
    for index, clip in enumerate(clips):
        native = ACTIVITY / clip['relativePath']
        result = bpy.ops.saio.import_node_anim(filepath=str(native), files=[{'name': native.name}], short_rot=short_rot)
        if result != {'FINISHED'}:
            raise RuntimeError(f'Native motion import failed: {native}')
        action = armature.animation_data.action
        if action is None:
            raise RuntimeError(f'No action decoded for {native}')
        action.name = clip.get('actionName', native.stem)
        action.use_fake_user = True
        actions.append({'name': action.name, 'nativeFile': clip['relativePath'], 'sourceSha256': clip['sha256'], 'frameRange': list(action.frame_range), 'nativeMotionLabel': clip.get('nativeMotionLabel')})
        # Keep separate source actions without overlapping imported NLA tracks.
        for track in list(armature.animation_data.nla_tracks):
            armature.animation_data.nla_tracks.remove(track)
        if (index + 1) % 50 == 0:
            print(f'IMPORTED_ACTIVITY_ACTIONS={name}:{index + 1}/{len(clips)}', flush=True)
    armature.animation_data.action = bpy.data.actions[actions[0]['name']]
    armature.animation_data.action_slot = armature.animation_data.action.slots[0]
    bpy.context.scene.frame_set(0)
    pipeline.export_scene(name, native_model, texture_pack)
    record = {'name': name, 'sourceModel': str(native_model.relative_to(ROOT)), 'output': str((OUTPUT / f'{name}.glb').relative_to(ROOT)), 'nodeCount': len(armature.data.bones), 'clips': len(actions), 'conversionReferenceFps': 60, 'shortRotationSource': short_rot, 'actions': actions, 'bytes': (OUTPUT / f'{name}.glb').stat().st_size}
    records.append(record)
    print('CONVERTED_ACTIVITY_LIBRARY=' + json.dumps({key: value for key, value in record.items() if key != 'actions'}), flush=True)


common = sorted(manifest['commonClips'], key=lambda clip: next(state['index'] for state in manifest['commonStates'] if state['nativeFile'] == clip['relativePath']))
convert_library('chao-actions', ROOT / '.local/extracted/pc-chao/Chao/models/AL_RootObject/al_ncz.sa2mdl', common, 'al_body', True)

support = manifest['supportClips']
for name, model, prefix, pack in [
    ('karate-referee-actions', 'karate/Referee.sa2mdl', 'karate/karate_judgeMotionTable/', 'al_karate_obj_tex'),
    ('race-spectator-actions', 'race/OmochaoSpectator.sa2mdl', 'race/omo_originalMotionTable/', 'al_body'),
    ('race-crab-actions', 'race/alo_race_kani.sa2mdl', 'race/race_kaniMotionTable/', 'al_race_obj_c_tex'),
]:
    clips = sorted((clip for clip in support if clip['relativePath'].startswith(prefix)), key=lambda clip: clip['relativePath'])
    convert_library(name, ACTIVITY / model, clips, pack, False)
for side in ['Right', 'Left']:
    drummer = [clip for clip in support if clip['filename'] in [f'{side}Drummer1.saanim', f'{side}Drummer2.saanim']]
    door = [clip for clip in support if clip['filename'] == f'{side}Door.saanim']
    convert_library(f'karate-{side.lower()}-drummer-actions', ACTIVITY / f'karate/{side}Drummer.sa2mdl', drummer, 'al_karate_obj_tex', False)
    convert_library(f'karate-{side.lower()}-door-actions', ACTIVITY / f'karate/{side}Door.sa2mdl', door, 'al_karate_obj_tex', False)

(OUTPUT / 'conversion-report.json').write_text(json.dumps({'libraries': records, 'totalConvertedClips': sum(record['clips'] for record in records), 'sourceManifest': '.local/extracted/activity-motions/manifest.json', 'note': 'Raw native action libraries at60 reference samples/second. Original622-state timing/transition metadata remains in the source manifest and must be applied by the runtime.'}, indent=2) + '\n')
(OUTPUT / 'geometry-report.json').write_text(json.dumps(pipeline.reports, indent=2) + '\n')
print('ACTIVITY_LIBRARY_CONVERSION_COMPLETE=' + str(sum(record['clips'] for record in records)), flush=True)
