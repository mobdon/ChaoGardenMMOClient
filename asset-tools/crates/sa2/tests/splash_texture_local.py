"""Compare stage20-splash reports to independent ArchiveTool -png conversions.
Pass CLI, game root, ArchiveTool.exe and optional suite (splash or pursuit). All copies and generated files stay local.
"""
import hashlib,json,os,shutil,subprocess,sys,tempfile
from pathlib import Path
from PIL import Image
cli,game,tool=map(Path,sys.argv[1:4])
gd=game/"resource/gd_PC"
env=dict(os.environ);env.setdefault("DOTNET_ROLL_FORWARD","Major")
suite=sys.argv[4] if len(sys.argv)>4 else "splash"
command,module,banks={
    "splash":("stage20-splash","stg20D.rel",[(20,"bsplash"),(18,"bsplashdl"),(34,"bsplashlb1")]),
    "pursuit":("kart-target-textures","CartD.rel",[(0,"cartLim"),(1,"limtails")]),
}[suite]
alpha=total=0
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for query,stem in banks:
        copy=Path(tmp).resolve()/(stem+".prs");shutil.copyfile(gd/copy.name,copy)
        subprocess.run([tool,"-png",copy],env=env,check=True)
        output=copy.with_suffix(".json")
        subprocess.run([cli,"--source-root",game,command,gd/module,output,str(query)],check=True)
        report=json.loads(output.read_text());folder=copy.with_suffix("")
        assert hashlib.sha256(copy.read_bytes()).hexdigest()==report["archiveSha256"]
        entries=(folder/"index.txt").read_text(encoding="utf-8-sig").splitlines()
        assert len(entries)==len(report["textures"])>0
        if suite=="splash": assert len(entries)==10
        total+=len(entries)
        for i,(line,row) in enumerate(zip(entries,report["textures"])):
            gid,name,size=line.split(",")
            with Image.open(folder/name) as image:
                pixels=image.convert("RGBA").tobytes();w,h=image.size
            n=sum(a!=255 for a in pixels[3::4]);alpha+=n
            assert size==f"{w}x{h}" and row==dict(index=i,globalIndex=int(gid),name=Path(name).stem,
                width=w,height=h,rgbaSha256=hashlib.sha256(pixels).hexdigest(),nonopaquePixels=n)
print(f"{total} exact RGBA matches, {alpha} nonopaque pixels")
