"""Independent reference inspection through existing SAIO/Blender tooling.

Run Blender --background --factory-startup --python this-file -- SOURCE_ROOT OUTPUT.
This orchestrates the installed importer; the production bundler is Rust.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

import bpy

args = sys.argv[sys.argv.index('--') + 1:]
source_root, output = [Path(value).resolve() for value in args]
if output.exists() or not output.parent.is_relative_to(Path.cwd().resolve() / '.local'):
    raise RuntimeError('Reference output must be new and under this checkout .local')
addon_dir = source_root / '.local/tools/saio-2.2.2'
sys.path.insert(0, str(source_root / '.local/tools/saio-python'))
spec = importlib.util.spec_from_file_location('sonic_adventure_io', addon_dir / '__init__.py', submodule_search_locations=[str(addon_dir)])
addon = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = addon
spec.loader.exec_module(addon)
addon.register()
rows = []
for model_id in range(92, 100):
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    native = source_root / f'.local/extracted/shadow-skate/teriosmdl/{model_id}.sa2mdl'
    assert bpy.ops.saio.import_mdl(filepath=str(native), files=[{'name': native.name}]) == {'FINISHED'}
    mesh = next(obj for obj in bpy.context.scene.objects if obj.type == 'MESH' and not obj.name.startswith('SAIO Bone Shape'))
    material = mesh.data.materials[0].saio_material
    # SAIO maps nativeXYZ to Blender(x,-z,y). Export/reference uses nativeXYZ.
    positions = [[vertex.co.x, vertex.co.z, -vertex.co.y] for vertex in mesh.data.vertices]
    colors = [list(item.color) for item in mesh.data.color_attributes['Color'].data]
    rows.append({
        'modelId': model_id, 'sourceSha256': hashlib.sha256(native.read_bytes()).hexdigest(),
        'positions': positions, 'triangles': [list(polygon.vertices) for polygon in mesh.data.polygons],
        'cornerColors': colors,
        'material': {'diffuse': list(material.diffuse), 'useTexture': material.use_texture,
                     'useAlpha': material.use_alpha, 'sourceAlpha': material.source_alpha,
                     'destinationAlpha': material.destination_alpha, 'doubleSided': material.double_sided,
                     'ignoreLighting': material.ignore_diffuse, 'ignoreSpecular': material.ignore_specular},
    })
output.write_text(json.dumps({'tool': 'Existing SAIO2.2.2/Blender reference', 'models': rows}, indent=2) + '\n')
print(f'Inspected all eight original models: {output}')
