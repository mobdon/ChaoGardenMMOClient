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
count,functions,names,ordinals=read("4I",directory+24)
def export(name):
    for i in range(count):
        at=offset(read("I",offset(names)+i*4)[0])
        if dll[at:dll.index(0,at)]==name.encode():
            ordinal,=read("H",offset(ordinals)+i*2)
            return base+read("I",offset(functions)+ordinal*4)[0]
    raise ValueError(name)
with tempfile.TemporaryDirectory(dir=".local") as td:
    for mode,name in enumerate(("object_chara_limoall_limoall","object_tornado_3rd_tornado_3rd")):
        out=Path(td)/f"{mode}.json"
        subprocess.run([cli,"--source-root",game,"kart-target-model",gd/"CartD.rel",out,str(mode)],check=True)
        rows=json.loads(out.read_text())["nodes"]
        stack=[(export(name),None)];seen=set()
        for row in rows:
            va,parent=stack.pop();assert va not in seen;seen.add(va)
            at=offset(va-base)
            flags,attach=read("II",at);child,sibling=read("II",at+44)
            assert (parent,flags,bool(attach))==(row["parent"],row["flags"],row["attach"] is not None),(name,len(seen))
            for key,fmt,field in (("position","3f",8),("angles","3i",20),("scale","3f",32)):
                assert struct.pack("<"+fmt,*row[key])==dll[at+field:at+field+12],(name,len(seen),key)
            if sibling:stack.append((sibling,parent))
            if child:stack.append((child,len(seen)-1))
        assert not stack
        print(f"{name}: {len(rows)} matching nodes")
