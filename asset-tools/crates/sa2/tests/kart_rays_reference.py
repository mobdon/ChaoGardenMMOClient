"""Compare story kart rays to triangle interiors from independent SA Tools splits.
Usage: python kart_rays_reference.py CLI GAME_DIR SPLIT_COURSES SPLIT_MODELS PRIVATE_DIR
"""
import configparser
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

cli, game, courses, models, private = map(Path, sys.argv[1:])
config = configparser.ConfigParser()
config.read(models / "info.ini", encoding="utf-8-sig")
def transform(pos, angle, xyz):
    s,c=math.sin(angle*math.tau/65536),math.cos(angle*math.tau/65536)
    x,y,z=xyz
    return pos[0]+c*x+s*z,pos[1]+y,pos[2]-s*x+c*z

def interior(piece):
    b=(models/f"{piece:03}.sa1mdl").read_bytes()
    def u(i): return struct.unpack_from("<I",b,i)[0]
    def h(i): return struct.unpack_from("<H",b,i)[0]
    def xyz(i): return struct.unpack_from("<fff",b,i)
    root=u(8); attach=u(root+4); mesh=u(attach+12); cursor=u(mesh+4)
    offset=(0,0,0) if u(root)&1 else xyz(root+8)
    for strip in range(h(mesh+2)):
        tag=h(cursor); count=tag&32767
        indices=struct.unpack_from(f"<{count}H",b,cursor+2)
        cursor+=2+2*count
        for t in range(count-2):
            pts=[xyz(u(attach)+12*v) for v in indices[t:t+3]]
            a,bp,c=pts
            if abs((bp[0]-a[0])*(c[2]-a[2])-(bp[2]-a[2])*(c[0]-a[0]))>20:
                return strip,t,[offset[k]+sum(p[k] for p in pts)/3 for k in range(3)]
    raise AssertionError(piece)

with tempfile.TemporaryDirectory(dir=private) as temp:
    req=Path(temp)/"ray.json"
    catalog=Path(temp)/"courses.json"
    subprocess.run([cli,"kart-courses",game/"sonic2app.exe",catalog],check=True)
    rows={r["name"]:r for r in json.loads(catalog.read_text())}
    assert len(rows)==5
    for route in ["101","280","beginner","standard","expert"]:
        filename=f"Route {route}" if route.isdecimal() else route.title()
        ids=list(map(int,(courses/f"{filename}.txt").read_text(encoding="utf-8-sig").split()))
        assert rows[route]["piece_ids"]==ids
        pos=(0.,0.,0.); heading=0
        for i,piece in enumerate(ids):
            if i in (0,len(ids)//2,len(ids)-1):
                out=Path(temp)/f"{route}-{i}.json"
                strip,t,local=interior(piece)
                point=transform(pos,heading,local)
                req.write_text(json.dumps(dict(origin=[point[0],point[1]+1,point[2]],direction=[0,-2,0],maxDistance=2)))
                subprocess.run([cli,"kart-raycast",game/"sonic2app.exe",out,game/"resource/gd_PC/DLL/Win32/Data_DLL.dll",str(route),req],check=True)
                report=json.loads(out.read_text())
                assert report["occurrencesWithoutCollision"]==[[j,n] for j,n in enumerate(ids) if not (models/f"{n:03}.sa1mdl").exists()]
                hit=report["hit"]
                assert (hit["occurrence"],hit["piece_id"],hit["strip"],hit["triangle"])==(i,piece,strip,t),(route,i,hit)
                assert abs(hit["distance"]-1)<0.005,(route,i,hit)
            row=config[str(piece)]
            pos=transform(pos,heading,list(map(float,row["EndPoint"].split(","))))
            heading=(heading+int(row.get("YRotation","0"),16))&65535
print("Matched starting, middle and final piece hits for all five courses.")
