"""Convert the three original race worlds and Karate arena separately.

Run Blender after scripts/extract-chao-activities.js. Preserve source units
in GLBs; collision JSON and bounds use canonical native ×0.1 coordinates.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('activity_asset_pipeline', ROOT / 'scripts/convert-assets.py')
pipeline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pipeline)
OUTPUT = ROOT / '.local/imports/activities'
DATA = ROOT / '.local/activities'
OUTPUT.mkdir(parents=True, exist_ok=True)
pipeline.OUTPUT = OUTPUT
pipeline.reports = []
BLENDER_TO_GLTF = Matrix(((1, 0, 0), (0, 0, 1), (0, -1, 0)))


def convert(name, relative, texture_pack, world_id):
    pipeline.clean_scene()
    source = ROOT / '.local/extracted/activities' / relative
    result = bpy.ops.saio.import_lvl(filepath=str(source), files=[{'name': source.name}])
    if result != {'FINISHED'}:
        raise RuntimeError(f'Native activity landtable import failed: {source}')
    bpy.context.view_layer.update()
    collision = []
    for obj in bpy.context.scene.objects:
        if obj.type != 'MESH' or not obj.saio_land_entry.is_collision:
            continue
        obj.data.calc_loop_triangles()
        flags = {prop.identifier: bool(getattr(obj.saio_land_entry, prop.identifier))
                 for prop in obj.saio_land_entry.bl_rna.properties if prop.identifier.startswith('sf_')}
        points = [BLENDER_TO_GLTF @ (obj.matrix_world @ vertex.co) * 0.1 for vertex in obj.data.vertices]
        collision.append({'name': obj.name, 'flags': flags,
                          'vertices': [[round(value, 7) for value in point] for point in points],
                          'triangles': [list(triangle.vertices) for triangle in obj.data.loop_triangles]})
    points = [point for mesh in collision for point in mesh['vertices']]
    if not points:
        raise RuntimeError(f'No native collision geometry: {name}')
    minimum = [min(point[i] for point in points) for i in range(3)]
    maximum = [max(point[i] for point in points) for i in range(3)]
    collision_doc = {'version': 1, 'worldId': world_id, 'coordinateSpace': 'canonical native ×0.1; origin retained',
                     'source': str(source.relative_to(ROOT)), 'scale': 0.1, 'position': [0, 0, 0],
                     'bounds': {'min': minimum, 'max': maximum}, 'meshes': collision,
                     'note': 'Original collision flags and triangles, not a validated race route or complete native collision engine.'}
    collision_path = DATA / f'{name}-collision.json'
    collision_path.write_text(json.dumps(collision_doc, separators=(',', ':')) + '\n')
    for obj in list(bpy.context.scene.objects):
        if obj.type == 'MESH' and not obj.saio_land_entry.is_visual:
            bpy.data.objects.remove(obj, do_unlink=True)
    pipeline.export_scene(name, source, texture_pack)
    report = pipeline.reports[-1]
    target = OUTPUT / f'{name}.glb'
    data = target.read_bytes()
    json_size = struct.unpack_from('<I', data, 12)[0]
    document = json.loads(data[20:20 + json_size])
    document['asset'].setdefault('extras', {})['sa2ActivitySource'] = {
        'nativeLandtable': str(source.relative_to(ROOT)), 'textureArchive': f'resource/gd_PC/{texture_pack}.prs',
        'sourceSha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'worldId': world_id,
        'canonicalScale': 0.1, 'canonicalPosition': [0, 0, 0], 'originRecentered': False,
    }
    encoded = json.dumps(document, separators=(',', ':')).encode()
    encoded += b' ' * (-len(encoded) % 4)
    binary_chunk = data[20 + json_size:]
    temporary = target.with_suffix('.converting.glb')
    temporary.write_bytes(struct.pack('<III', 0x46546c67, 2, 20 + len(encoded) + len(binary_chunk)) + struct.pack('<II', len(encoded), 0x4e4f534a) + encoded + binary_chunk)
    pipeline.replace_asset(temporary, target)
    record = {key: report[key] for key in ['name', 'source', 'output', 'meshCount', 'vertices', 'blenderBounds']}
    record.update({'worldId': world_id, 'collisionMeshes': len(collision),
                   'collisionTriangles': sum(len(mesh['triangles']) for mesh in collision),
                   'canonicalBounds': {'min': minimum, 'max': maximum},
                   'scale': 0.1, 'position': [0, 0, 0], 'bytes': target.stat().st_size,
                   'sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
                   'collisionOutput': str(collision_path.relative_to(ROOT))})
    print('SA2_ACTIVITY_EXPORT=' + json.dumps(record))
    return record


records = [convert(*args) for args in [
    ('race-normal', 'Chao/stg_race/Normal/objLandTableRace.sa2blvl', 'al_stg_race_tex', 'native-race-normal'),
    ('race-dark', 'Chao/stg_race/Dark/objLandTableRaceDark.sa2blvl', 'al_stg_race_dark_tex', 'native-race-dark'),
    ('race-hero', 'Chao/stg_race/Hero/objLandTableRaceHero.sa2blvl', 'al_stg_race_hero_tex', 'native-race-hero'),
    ('karate-arena', 'Chao/stg_karate/objLandTableChaoKarate.sa2blvl', 'al_stg_karate_tex', 'native-karate-arena'),
]]
(OUTPUT / 'conversion-report.json').write_text(json.dumps({'version': 1, 'assets': records}, indent=2) + '\n')
race_file = DATA / 'races.json'
races = json.loads(race_file.read_text())
for course in races['courses']:
    record = records[course['source']['landtableIndex']]
    minimum, maximum = record['canonicalBounds']['min'], record['canonicalBounds']['max']
    course.update({'model': record['name'], 'worldId': record['worldId'],
                   'bounds': {'minX': minimum[0], 'maxX': maximum[0], 'minZ': minimum[2], 'maxZ': maximum[2]},
                   'geometryImported': True, 'geometryValidated': False,
                   'sourceTransform': {'scale': 0.1, 'position': [0, 0, 0]}})
race_file.write_text(json.dumps(races, indent=2) + '\n')
