from pathlib import Path
import subprocess,shutil,os
root=Path(__file__).resolve().parents[1]
src=Path(os.environ.get('SA2_GAME_PATH',r'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2'))/'resource/gd_PC'
tools=root/'.local/tools/sa-tools-786/bin/lib'
env=dict(os.environ,DOTNET_ROLL_FORWARD='Major')
for stem in 'plcom metalsonic sonic terios miles knuck rouge egg amy chaos0 tical cwalk dwalk twalk ewalk ssonic sshadow sonic1 shadow1 bknuck brouge twalk1 ewalk1 ewalk2'.split():
 for suffix,util in [('mdl','splitMDL'),('mtn','splitMTN'),('tex','ArchiveTool')]:
  name=stem+suffix
  folder=root/'.local/extracted'/('textures' if suffix=='tex' else 'dev-roster')
  folder.mkdir(parents=True,exist_ok=True)
  if (folder/name).is_dir() or not (src/(name+'.prs')).exists():continue
  archive=folder/(name+'.prs');shutil.copyfile(src/archive.name,archive)
  subprocess.run(['dotnet',str(tools/(util+'.dll'))]+(['-png'] if suffix=='tex' else [])+[str(archive)],check=suffix!='mtn',stdout=subprocess.DEVNULL,env=env)
if not (root/'.local/extracted/dev-roster/chaos0mtn/242.saanim').is_file():
 subprocess.run(['pwsh','-NoProfile','-File',str(root/'scripts/prepare-chaos0-motions.ps1')],check=True)
# Preserve the original maps. Repaint only the blue alloy in a private copy.
from PIL import Image
original=root/'.local/extracted/textures/metalsonictex'
black=original.with_name('metalsonicblacktex')
shutil.copytree(original,black,dirs_exist_ok=True)
for name in ('mstx_body0.png','mstx_body1.png','mstx_ref1.png'):
 p=black/name;im=Image.open(p).convert('RGBA')
 def graphite(c):
  r,g,b,a=c
  if name!='mstx_ref1.png' and not (b>r*1.25 and b>g*1.12):return c
  v=int((r*.23+g*.56+b*.21)*.38)
  return (v,min(255,v+2),min(255,v+5),a)
 im.putdata([graphite(c) for c in im.getdata()]);im.save(p)
