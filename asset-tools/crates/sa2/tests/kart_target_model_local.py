"""Compare CartD hierarchies against independent little-endian PC DLL exports."""
import json, struct, subprocess, sys, tempfile
from pathlib import Path

cli, game = map(Path,sys.argv[1:])
gd=game/"resource/gd_PC"
dll=(gd/"DLL/Win32/Data_DLL.dll").read_bytes()
def read(fmt,at):
    return struct.unpack_from("<"+fmt,dll,at)
pe,=read("I",60)
assert dll[pe:pe+4]==b"PE\0\0"
sections,=read("H",pe+6)
opt=pe+24
assert read("H",opt)==(0x10b,)
base,=read("I",opt+28)
table=opt+read("H",pe+20)[0]
def offset(rva):
    for i in range(sections):
        _,start,size,raw=read("4I",table+i*40+8)
        if start<=rva<start+size:return raw+rva-start
    raise ValueError(rva)
directory=offset(read("I",opt+96)[0])
export_count,functions,names,ordinals=read("4I",directory+24)
def export(name):
    for i in range(export_count):
        at=offset(read("I",offset(names)+i*4)[0])
        if dll[at:dll.index(0,at)]==name.encode():
            ordinal,=read("H",offset(ordinals)+i*2)
            return base+read("I",offset(functions)+ordinal*4)[0]
    raise ValueError(name)
with tempfile.TemporaryDirectory(dir=".local") as td:
    for mode,name in enumerate(("object_chara_limoall_limoall","object_tornado_3rd_tornado_3rd")):
        out=Path(td)/f"{mode}.json"
        subprocess.run([cli,"--source-root",game,"kart-target-model",gd/"CartD.rel",out,str(mode)],check=True)
        report=json.loads(out.read_text());rows=report["nodes"]
        stack=[(export(name),None)];seen=set();vertices=triangles=corners=0
        texout=Path(td)/f"bank{mode}.json"
        subprocess.run([cli,"--source-root",game,"kart-target-textures",gd/"CartD.rel",texout,str(mode)],check=True)
        bank=json.loads(texout.read_text())["textures"]
        assert report["textureCount"]==len(bank) and report["textureBank"]==("cartLim","limtails")[mode]
        material=dict.fromkeys(["diffuseRgba","ambientRgba","specularRgbe","blendFlags","textureId","textureWords"])
        batches=0
        for row in rows:
            va,parent=stack.pop();assert va not in seen;seen.add(va)
            at=offset(va-base)
            flags,attach=read("II",at);child,sibling=read("II",at+44)
            assert (parent,flags,bool(attach))==(row["parent"],row["flags"],row["attach"] is not None),(name,len(seen))
            for key,fmt,field in (("position","3f",8),("angles","3i",20),("scale","3f",32)):
                assert struct.pack("<"+fmt,*row[key])==dll[at+field:at+field+12],(name,len(seen),key)
            if attach:
                a=offset(attach-base);g=row["geometry"]
                assert struct.pack("<4f",*g["center"],g["radius"])==dll[a+8:a+24]
                vptr,=read("I",a)
                if vptr:
                    v=offset(vptr-base)
                    for chunk in g["chunks"]:
                        kind,flags,size,index,count=read("BB3H",v)
                        assert (kind,flags,index,count)==(41,chunk["flags"],chunk["index_offset"],len(chunk["vertices"]))
                        assert size==1+count*6
                        for j,vert in enumerate(chunk["vertices"]):
                            atv=v+8+j*24
                            assert struct.pack("<6f",*vert["position"],*vert["normal"])==dll[atv:atv+24],(name,len(seen),j)
                        vertices+=count;v+=4+size*4
                    assert read("B",v)==(255,)
                else:assert not g["chunks"]
                poly=offset(read("I",a+4)[0]-base)
                for batch in g["polygons"]:
                    c=batch["chunk"];kind=c["kind"];words=c["rawWords"]
                    assert struct.pack("<"+"H"*len(words),*words)==dll[poly:poly+2*len(words)],(name,len(seen),kind)
                    if kind==8:
                        h,tex=read("HH",poly)
                        material.update(textureId=tex&0x1fff,textureWords=[h,tex])
                    elif 17<=kind<=23:
                        material["blendFlags"]=dll[poly+1];pos=poly+4
                        for bit,key in ((1,"diffuseRgba"),(2,"ambientRgba"),(4,"specularRgbe")):
                            if kind&bit:
                                b,gc,r,alpha=dll[pos:pos+4];pos+=4
                                material[key]=[r,gc,b,alpha]
                    if kind in (64,65):
                        batches+=1
                        assert batch["material"]==material
                        if kind==65:
                            idx=material["textureId"];assert bank[idx]["index"]==idx and bank[idx]["name"]

                        cursor=poly+6
                        for strip in batch["strips"]:
                            n,=read("h",cursor);cursor+=2
                            assert (n<0,abs(n))==(strip["reversed"],len(strip["corners"]))
                            ids=[]
                            for corner in strip["corners"]:
                                vertex,=read("H",cursor);cursor+=2
                                assert vertex==corner["vertex"];ids.append(vertex)
                                if kind==65:
                                    uv=read("hh",cursor);cursor+=4
                                    assert list(uv)==corner["uvRaw"] and [x/256 for x in uv]==corner["uv"]
                                else:assert corner["uvRaw"] is None
                            expected=[]
                            for k in range(len(ids)-2):
                                face=[k,k+1,k+2]
                                if (k%2==1)!=(n<0):face[0],face[1]=face[1],face[0]
                                if len({ids[j] for j in face})==3:expected.append(face)
                            assert expected==strip["triangles"]
                            triangles+=len(expected);corners+=len(ids)
                    poly+=2*len(words)
                assert read("B",poly)==(255,)
            if sibling:stack.append((sibling,parent))
            if child:stack.append((child,len(seen)-1))
        assert not stack
        print(f"{name}: {len(rows)} matching nodes, {vertices} matching vertices, {batches} material batches, {corners} corners, {triangles} triangles")
        if mode==0:
            assert report["motion"] is None and all("motion" not in row for row in rows)
            continue
        m=offset(export("motion_tornado_3rd")-base)
        tracks,frames,flags,interp=read("IIHH",m)
        assert (frames,flags,interp)==tuple(report["motion"][k] for k in ("frames","type","interpolation_word"))
        keys=0
        for i,row in enumerate(rows):
            channel_row=offset(tracks-base)+i*24
            for j,(name,fmt) in enumerate((("position","3f"),("angles","3i"),("scale","3f"))):
                ptr,=read("I",channel_row+j*4);n,=read("I",channel_row+12+j*4)
                channel=row["motion"][name]
                assert len(channel)==n,(i,name)
                for k,entry in enumerate(channel):
                    key=offset(ptr-base)+16*k
                    assert struct.pack("<I"+fmt,entry["frame"],*entry["value"])==dll[key:key+16],(i,name,k)
                    keys+=1
        print(f"{len(rows)*3} channels, {keys} matching position/angle/scale keys")
