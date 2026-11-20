"""Create private pose sheets from installed SA2 Chao models and Chao motions.
Blender --background --factory-startup --python this-file -- dark-chaos ...
Never writes to the live model or atlas directories."""
import bpy, json, math, runpy, sys, hashlib
from pathlib import Path
from mathutils import Matrix, Vector
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.local/garden-model-poses'
OUT.mkdir(parents=True,exist_ok=True)
helpers=runpy.run_path(str(ROOT/'scripts/convert-assets.py'))
SRC=helpers['EXTRACTED']/'pc-chao/Chao/models'
poses=json.loads((ROOT/'asset-tools/crates/tiny-game-presentation/catalog/garden-pose-map.json').read_text())['poses']
models={'dark-chaos':'al_dchaos','hero-chaos':'al_hchaos','neutral-chaos':'al_nchaos','hero':'al_hnz','dark':'al_dnz','tails':'al_tails','knuckles':'al_knuckles','amy':'al_amy'}
TILE=64

def render(form):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    kind=form.split("+")[0]
    slots=form.split("+")[1:]
    samples=poses+[dict(id=67+i,motion='alm_nade_kimotii.saanim',phase=phase,yaw=0) for i,phase in enumerate((0,.25,.5,.75,1))] if 'chest' in slots else poses
    scene=bpy.context.scene
    source=SRC/'AL_RootObject'/(models[kind]+'.sa2mdl')
    bpy.ops.saio.import_mdl(filepath=str(source),files=[{'name':source.name}],import_as_armature=True)
    helpers['prepare_geometry'](kind,'al_body')
    meshes=[o for o in scene.objects if o.type=='MESH' and o.data.polygons]
    rig=next(o for o in scene.objects if o.type=='ARMATURE')
    bpy.ops.object.select_all(action='DESELECT')
    rig.select_set(True)
    bpy.context.view_layer.objects.active=rig
    actions={}
    for m in sorted({p['motion'] for p in samples}|{'alm_tati.saanim'}):
        anim=SRC/'al_motion_table'/m
        if not anim.is_file():raise ValueError(m)
        bpy.ops.saio.import_node_anim(filepath=str(anim),files=[{'name':m}],short_rot=True)
        act=next(t for t in rig.animation_data.nla_tracks if t.name==anim.stem).strips[0].action
        if not act:raise ValueError('No motion '+m)
        act.use_fake_user=True
        actions[m]=act
    rig.animation_data.use_tweak_mode=False
    while rig.animation_data.nla_tracks:rig.animation_data.nla_tracks.remove(rig.animation_data.nla_tracks[0])
    def reset_bones():
        for bone in rig.pose.bones:bone.matrix_basis=Matrix.Identity(4)
    def sample(m,phase):
        reset_bones()
        act=actions[m]
        rig.animation_data.action=act
        if act.slots:rig.animation_data.action_slot=act.slots[0]
        a,b=act.frame_range
        scene.frame_set(round(a+(b-a)*phase))
        bpy.context.view_layer.update()
    sample('alm_tati.saanim',0)
    graph=bpy.context.evaluated_depsgraph_get()
    def corners():return [e.matrix_world@Vector(v) for o in meshes for e in [o.evaluated_get(graph)] for v in e.bound_box]
    pts=corners()
    h=max(p.z for p in pts)-min(p.z for p in pts)
    scale=0.64/h
    gear=[]
    if slots:
        brand=runpy.run_path(str(ROOT/'scripts/render-brand-assets.py'))
        import re
        fittings=json.loads((brand['ASSETS']/'brand-gear-mounts.json').read_text())
        report=json.loads((brand['ASSETS']/'upgrade-conversion-report.json').read_text())['assets']
        ratio=float(re.search(r'CHAO_SHOE_LENGTH_RATIO\s*=\s*([.0-9]+)',(ROOT/'src/chao-attachments.js').read_text()).group(1))
        conv=Matrix(((-1,0,0,0),(0,0,-1,0),(0,1,0,0),(0,0,0,1)))
        rig.animation_data.action=None;reset_bones();bpy.context.view_layer.update()
        if 'chest' in slots:
            bone=next(b for b in rig.pose.bones if b.name.startswith('001_'))
            before=set(scene.objects)
            brand['add_chest_emerald'](meshes)
            bpy.context.view_layer.update()
            inv=(rig.matrix_world@bone.matrix).inverted()
            gear.extend((o,bone,inv@o.matrix_world.copy()) for o in set(scene.objects)-before if o.type=='MESH' and not o.hide_render and o.data.polygons)
        boots=None
        for entry in fittings:
            slot=entry['slot'];side=entry['side']
            if slot not in slots:continue
            index=({'right':6,'left':13} if slot=='footwear' else {'right':3,'left':10})[side]
            bone=next(b for b in rig.pose.bones if b.name.startswith(f'{index:03}_'))
            joint=rig.matrix_world@bone.matrix
            part=[o for o in meshes if o.parent_bone==bone.name]
            if not part:raise ValueError('no limb: '+bone.name)
            lo,hi=brand['bounds'](part)
            if slot=='hands':
                # The parent mesh includes the entire arm. Match the desktop
                # distal-palm fit, not the bounding box from shoulder to finger.
                tip=next(b for b in rig.pose.bones if b.name.startswith(f'{index+1:03}_'))
                start=joint.translation
                axis=((rig.matrix_world@tip.matrix).translation-start).normalized()
                points=brand['geometry_points'](part)
                reach=max((v-start).dot(axis) for v in points)
                if reach<=0:raise ValueError('Invalid hand reach: '+bone.name)
                palm=[v for v in points if (v-start).dot(axis)>=reach*.55]
                lo,hi=[Vector(tuple(fn(v[i] for v in palm) for i in range(3))) for fn in (min,max)]

            asset='shadow-air-shoes-right' if slot=='footwear' else entry['asset']
            wrap,props=brand['import_asset'](asset)
            if slot=='footwear':
                rot=brand['shoe_rotation'](next(v for v in report if v['name']==asset)['mountingReference'])
            else:
                vals=entry['matrix'];mat=Matrix(tuple(tuple(vals[c*4+r] for c in range(4)) for r in range(4)))
                rot=(conv@mat@conv.inverted()).to_quaternion()
            wrap.rotation_mode='QUATERNION';wrap.rotation_quaternion=rot
            bl,bh=brand['bounds'](props)
            k=((hi.y-lo.y)*ratio/(bh.y-bl.y)) if slot=='footwear' else max(hi-lo)*1.12/max(bh-bl)
            wrap.scale=(k,k,k);wrap.location=(hi+lo)*.5-(bl+bh)*.5*k
            if slot=='footwear':
                wrap.location.z=lo.z-bl.z*k;bpy.context.view_layer.update()
                if side=='right':boots=wrap.matrix_world.copy()
                else:wrap.matrix_world=Matrix.Diagonal((-1,1,1,1))@boots
                for o in part:o.hide_render=True
            bpy.context.view_layer.update()
            for o in props:
                if o.type=='MESH' and not o.hide_render:gear.append((o,bone,joint.inverted()@o.matrix_world.copy()))
    target=bpy.data.scenes.new('Garden pose sheet')
    import mathutils
    tilt=Matrix.Rotation(math.radians(9),4,'X')
    anchors=[]
    for p in samples:
        sample(p['motion'],p['phase'])
        rot=tilt@Matrix.Rotation(math.radians(p['yaw']),4,'Z')
        # Constant body scale and ground anchor for every frame in both views.
        # Recenter the original motion's horizontal root travel around its contact.
        points=[rot@v for v in corners()]
        cx=(min(v.x for v in points)+max(v.x for v in points))*.5
        floor=min(v.z for v in points)
        x=p['id']%8+.5;z=9-p['id']//8-.86
        placement=Matrix.Translation((x,0,z))@Matrix.Scale(scale,4)@Matrix.Translation((-cx,0,-floor))@rot
        draw=[(o,o.evaluated_get(graph).matrix_world.copy()) for o in meshes if not o.hide_render]
        draw += [(o,rig.matrix_world@bone.matrix@relative) for o,bone,relative in gear]
        for o,world_matrix in draw:
            e=o.evaluated_get(graph)
            mesh=bpy.data.meshes.new_from_object(e,depsgraph=graph,preserve_all_data_layers=True)
            obj=bpy.data.objects.new(o.name+'-pose',mesh)
            target.collection.objects.link(obj)
            obj.matrix_world=placement@world_matrix
        anchors.append(round((.86-(max(v.z for v in points)-floor)*scale)*TILE,2))
    cam=bpy.data.cameras.new('Orthographic sheet')
    cam.type='ORTHO';cam.ortho_scale=9
    obj=bpy.data.objects.new('Camera',cam);target.collection.objects.link(obj)
    obj.location=(4,-100,4.5);obj.rotation_euler=(math.pi/2,0,0)
    target.camera=obj
    target.render.engine='CYCLES';target.cycles.samples=8
    target.render.resolution_x=8*TILE;target.render.resolution_y=9*TILE;target.render.resolution_percentage=100
    target.render.film_transparent=True;target.render.image_settings.color_mode='RGBA'
    target.view_settings.view_transform='Standard'
    world=bpy.data.worlds.new('Garden ambient');world.use_nodes=True
    world.node_tree.nodes['Background'].inputs['Color'].default_value=(.85,.9,1,1)
    world.node_tree.nodes['Background'].inputs['Strength'].default_value=.8;target.world=world
    lamp=bpy.data.lights.new('Soft sun','SUN');lamp.energy=2;lamp.angle=.3
    sun=bpy.data.objects.new('Sun',lamp);target.collection.objects.link(sun);sun.rotation_euler=(.7,-.5,-.3)
    fname='garden-'+form.replace('+','-')+'.png'
    target.render.filepath=str(OUT/fname)
    bpy.ops.render.render(scene=target.name,write_still=True)
    (OUT/(form+'.json')).write_text(json.dumps(dict(filename=fname,sha256=hashlib.sha256((OUT/fname).read_bytes()).hexdigest(),headY=anchors,model=source.name,modelSha256=hashlib.sha256(source.read_bytes()).hexdigest(),motions={m:hashlib.sha256((SRC/'al_motion_table'/m).read_bytes()).hexdigest() for m in sorted(actions)})))
for form in sys.argv[sys.argv.index('--')+1:]:render(form)

