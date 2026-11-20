"""Render the privately extracted SA2 Neutral Garden Fruit for the Tiny shop/bag."""
import bpy, runpy
from pathlib import Path
from mathutils import Vector
root=Path(__file__).resolve().parents[1]
helpers=runpy.run_path(str(root/'scripts/convert-assets.py'))
bpy.ops.wm.read_factory_settings(use_empty=True)
model=root/'.local/extracted/garden-fruit/NeutralGardenFruit.sa2mdl'
bpy.ops.saio.import_mdl(filepath=str(model),files=[{'name':model.name}])
helpers['prepare_geometry']('neutral-garden-fruit','al_object')
scene=bpy.context.scene
pts=[o.matrix_world@Vector(c) for o in scene.objects if o.type=='MESH' and o.data.polygons for c in o.bound_box]
lo,hi=[Vector([f(p[i] for p in pts) for i in range(3)]) for f in (min,max)]
center=(lo+hi)/2
cam=bpy.data.cameras.new('Fruit');cam.type='ORTHO';cam.ortho_scale=max(hi-lo)*1.25
o=bpy.data.objects.new('Camera',cam);scene.collection.objects.link(o);o.location=center+Vector((1,-4,1))*max(hi-lo);o.rotation_euler=(center-o.location).to_track_quat('-Z','Y').to_euler();scene.camera=o
scene.world=bpy.data.worlds.new('Ambient');scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs[0].default_value=(.88,.88,.88,1)
scene.render.engine='CYCLES';scene.cycles.samples=16;scene.render.film_transparent=True;scene.view_settings.view_transform='Standard'
scene.render.resolution_x=scene.render.resolution_y=48;scene.render.resolution_percentage=100
scene.render.filepath=str(root/'.local/extracted/garden-fruit/garden-fruit.png');bpy.ops.render.render(write_still=True)
