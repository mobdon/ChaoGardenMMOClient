# Export the original water footprint separately from walkable terrain.
import bpy,importlib.util
from pathlib import Path
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('helpers',root/'scripts/convert-cinematic-assets.py')
h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
for stem,zone,img in [('neut','garden','water_a00'),('hero','hero','hwater_a00'),('dark','dark','hotwater_a00')]:
 h.clear_scene();p=next((root/'.local/extracted/pc-chao/Chao'/('stg_'+stem)).glob('*.sa2blvl'))
 bpy.ops.saio.import_lvl(filepath=str(p),files=[{'name':p.name}])
 objects=[o for o in bpy.context.scene.objects if o.type=='MESH' and o.saio_land_entry.sf_water]
 for o in list(bpy.data.objects):
  if o.name.startswith('SAIO Bone Shape'):bpy.data.objects.remove(o,do_unlink=True)
 assert objects
 bpy.ops.object.select_all(action='DESELECT')
 mat=bpy.data.materials.new('water-'+zone);mat.use_nodes=True
 shader=mat.node_tree.nodes.get('Principled BSDF')
 tex=mat.node_tree.nodes.new('ShaderNodeTexImage');tex.image=bpy.data.images.load(str(h.TEXTURES/('al_stg_'+stem+'_tex')/(img+'.png')))
 mat.node_tree.links.new(tex.outputs['Color'],shader.inputs['Base Color'])
 for o in objects:
  o.hide_set(False);o.hide_viewport=False;o.hide_render=False
  for c in o.users_collection:c.hide_viewport=False;c.hide_render=False
  o.select_set(True);o.data.materials.clear();o.data.materials.append(mat)
  uv=o.data.uv_layers.active or o.data.uv_layers.new()
  for loop in o.data.loops:
   v=o.matrix_world @ o.data.vertices[loop.vertex_index].co;uv.data[loop.index].uv=(v.x/15,v.y/15)
  for c in list(o.data.color_attributes):o.data.color_attributes.remove(c)
 bpy.ops.export_scene.gltf(filepath=str(h.OUTPUT/('water-'+zone+'.glb')),export_format='GLB',use_selection=True,export_animations=False)
