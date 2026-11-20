# Build private local presentation assets with the existing SAIO importer.
import bpy,importlib.util,json,re,sys
from pathlib import Path
from mathutils import Matrix
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('helpers',root/'scripts/convert-cinematic-assets.py')
h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
base=root/'.local/extracted/dev-roster'
roster={'metalsonic':(418,169,6,222,223,224,225),'sshadow':(355,87,88,95),'ssonic':(328,87,88,95),'sonic':(0,0,6,9),'terios':(65,169,6,171,172,173,174),'amy':(395,203,204,207),'miles':(208,89,96,114),'knuck':(142,128,125,9),'rouge':(171,168,150,153),'egg':(230,89,91,94),'chaos0':(507,260,261,264),'tical':(483,220,226,229),'cwalk':(442,89,91,93),'dwalk':(465,89,199,201),'twalk':(293,141,143,145),'ewalk':(248,89,91,93)}
out=root/'.local/imports/dev-roster';out.mkdir(exist_ok=True)
names=dict(zip(roster,['MetalSonic','SuperShadow','SuperSonic','Sonic','Shadow','Amy','Tails','Knuckles','Rouge','Eggman','Chaos','Tikal','ChaoWalker','DarkChaoWalker','MechTails','MechEggman']))
attack_ids=dict(metalsonic=99,sonic=99,terios=99,amy=245,miles=94,knuck=109,tical=109,chaos0=242,rouge=144,egg=163,cwalk=133,dwalk=133,twalk=184,ewalk=133,ssonic=95,sshadow=95)
tables={}
for line in (base/'pc-tables.tsv').read_text(encoding='utf-8-sig').splitlines():
 fields=line.split('\t')
 if fields[0].endswith('AnimList'):tables.setdefault(fields[0],{})[int(fields[1])]=(int(fields[2]),float(fields[7]))
def action_sources(stem):
 table=tables[names[stem]+'AnimList']
 labels=['Idle','Walk','Speedwalk','Run','FastSkate','SprintSkate','Jump','Fall','Attack']
 states=[0,6,7,8,9,10,65,68,attack_ids[stem]]
 if stem in ('ssonic','sshadow'):states=[0,88,88,88,94,95,91,93,95]
 # Prefer common action slots only where the PC table actually resolves the expected motion.
 extra={'sonic':{'Homing':100,'Grind':104},'terios':{'Homing':100,'Grind':104},'metalsonic':{'Homing':100,'Grind':104},
        'knuck':{'Climb':89,'Glide':105,'Dig':91},'rouge':{'Climb':159,'Glide':155,'Dig':165},
        'tical':{'Climb':89,'Glide':241,'Dig':235},'chaos0':{'Climb':100,'Glide':105,'Dig':91},
        'ssonic':{'Flight':88},'sshadow':{'Flight':88}}
 for name,mid in extra.get(stem,{}).items():
  state=100 if name=='Homing' else next(i for i,(motion,rate) in table.items() if motion==mid and rate>0)
  labels.append(name);states.append(state)
 maps={}
 for line in (root/'.local/tools/sa-tools-786/GameConfig/PC_SA2/MDL'/(stem.upper()+'MTN.ini')).read_text().splitlines():
  if '=' in line and line.split('=')[0].isdigit():maps[int(line.split('=')[0])]=line.split('=',1)[1]
 for label,state in zip(labels,states):
  if stem in ('cwalk','dwalk','twalk','ewalk') and label in ('Walk','Speedwalk','Run','FastSkate','SprintSkate'):
   # Unsupported common humanoid slots are not locomotion for a walker.
   state={'Walk':6,'Speedwalk':8,'Run':8,'FastSkate':10,'SprintSkate':10}[label]
  motion,rate=table[state]
  visited=set()
  while 'NO FILE' in maps.get(motion,'') and motion not in visited:
   visited.add(motion);m=re.search(r'uses (\d+)',maps[motion])
   if not m:break
   motion=int(m[1])
  mapped=maps.get(motion,'').split('|')[0].removeprefix('figure/bin/')
  file=base/mapped if mapped.endswith('.saanim') else base/(stem+'mtn')/(str(motion)+'.saanim')
  if not file.is_file():
   # splitMTN writes only the first motion for duplicate native pointers.
   folder=file.parent;ini=folder/(folder.name+'.ini')
   if ini.exists():
    entries=dict(l.split('=',1) for l in ini.read_text().splitlines() if '=' in l)
    for k,v in entries.items():
     candidate=folder/(k+'.saanim')
     if v==entries.get(str(motion)) and candidate.exists():file=candidate;break
  if not file.is_file():raise ValueError(f'{stem} {label}: missing native {motion}: {file}')
  yield label,file,rate,state,motion
def build(stem,ids,black=False):
 h.clear_scene();bpy.context.scene.render.fps=60
 model=base/(stem+'mdl')/(str(ids[0])+'.sa2mdl')
 bpy.ops.saio.import_mdl(filepath=str(model),files=[{'name':model.name}],import_as_armature=True)
 for obj in list(bpy.context.scene.objects):
  if obj.name.startswith('SAIO Bone Shape'):bpy.data.objects.remove(obj,do_unlink=True)
 arm=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
 bpy.context.view_layer.objects.active=arm;arm.select_set(True)
 mats=h.prepare_materials('metalsonicblacktex' if black else stem+'tex')
 for m in mats:
  mat=bpy.data.materials[m['name']];mat['nativeEnvironment']=bool(m['environment']);mat['nativeTextureIndex']=m['textureIndex']
 clips=[]
 meta={}
 for label,file,rate,state,motion in action_sources(stem):
  meta[label]={'state':state,'motion':motion,'rate':rate}
  arm.animation_data_clear()
  for b in arm.pose.bones:b.matrix_basis=Matrix.Identity(4)
  bpy.ops.saio.import_node_anim(filepath=str(file),files=[{'name':file.name}],short_rot=True)
  action=arm.animation_data.action;action.name=label;action.use_fake_user=True;clips.append(label)
 arm['sa2RosterActions']=json.dumps(meta)
 if clips:
  arm.animation_data.action=bpy.data.actions[clips[0]]
  arm.animation_data.action_slot=arm.animation_data.action.slots[0]
  bpy.context.scene.frame_set(0)
 # SAIO can recreate its editor-only bone shape while importing actions.
 # Export the playable rig and its children, excluding editor helpers.
 bpy.ops.object.select_all(action='DESELECT')
 for obj in [arm,*arm.children_recursive]:
  if not obj.name.startswith('SAIO Bone Shape'):obj.select_set(True)
 bpy.ops.export_scene.gltf(use_selection=True,filepath=str(out/(('metalsonicblack' if black else stem)+'.glb')),export_format='GLB',export_extras=True,export_animations=True,export_animation_mode='ACTIONS',export_force_sampling=True,export_reset_pose_bones=True)
 print('ROSTER DONE',stem,clips,flush=True)
for stem,ids in roster.items():
 if '--only-walkers' in sys.argv and not stem.endswith('walk'):continue
 if '--only-metal' in sys.argv and stem!='metalsonic':continue
 if '--only-shadow' in sys.argv and stem!='terios':continue
 try:build(stem,ids)
 except Exception:
  raise

if '--only-shadow' not in sys.argv and '--only-walkers' not in sys.argv:build('metalsonic',roster['metalsonic'],True)
