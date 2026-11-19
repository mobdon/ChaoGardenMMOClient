"""Compare saved-section CLI lookup to local SA Tools splits; no player save used.
Args: CLI, installed EXE, story split directory, SA2Enums.h, ignored temporary root.
"""
import configparser
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

cli, exe, splits, enums, tmp_root = sys.argv[1:]
header = Path(enums).read_text()
def enum(name):
    body = header.split("enum "+name,1)[1].split("};",1)[0]
    values, number = {}, -1
    for key,explicit in re.findall(name+r"_(\w+)(?:\s*=\s*(\d+))?",body):
        number = int(explicit) if explicit else number+1
        values[key] = number
    return values
chars, levels = enum("Characters"), enum("LevelIDs")
sections = actions = 0
with tempfile.TemporaryDirectory(dir=tmp_root) as tmp:
    root=Path(tmp)
    image=bytearray(0x349a)
    for seq,(name,rva) in [(1,("Hero",0x133a148)),(2,("Dark",0x133a370)),(3,("Last",0x133a580)),(12,("Hero Boss Rush",0x133a628)),(13,("Dark Boss Rush",0x133a698)),(14,("All Boss Rush",0x133a708))]:
        ini=configparser.ConfigParser(interpolation=None)
        with (Path(splits)/(name+(".ini" if seq>=12 else " Story.ini"))).open(encoding="utf-8-sig") as f: ini.read_file(f)
        expected=[]
        ordinal=0
        for idx in range(len(ini.sections())):
            row=ini[str(idx)]
            kind=row["Type"].lower()
            if kind=="event": items=[dict(kind=kind,event=int(e)) for e in row["Events"].split(",")]
            elif kind=="level": items=[dict(kind=kind,character=chars[row["Character"]],level=levels[row["Level"]])]
            else:
                assert kind=="credits"
                items=[dict(kind=kind)]
            expected.append(dict(index=idx,status="section",rva=rva+idx*12,actions=[dict(actionIndex=ordinal+i,action=a) for i,a in enumerate(items)]))
            ordinal+=len(items)
        sections+=len(expected); actions+=ordinal
        expected.append(dict(index=len(expected),status="terminator",rva=rva+len(expected)*12))
        for idx in range(len(expected)):
            nxt=idx+1 if idx+1<len(expected) else 255
            image[0x3254:0x3257]=bytes([nxt,idx,seq])
            snap=root/"fixture.bin"; snap.write_bytes(image)
            out=root/f"{seq}-{idx}.json"
            subprocess.run([cli,"story-sections",exe,str(out),str(snap)],check=True)
            got=json.loads(out.read_text())
            assert got["sequence"]==name and got["current"]==expected[idx]
            assert got["nextInSameTable"]==(expected[nxt] if nxt!=255 else dict(index=255,status="outsideTable"))
    print(f"Matched {sections} story sections, {actions} actions and all six terminators.")
