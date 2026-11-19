"""Compare stage20-splash reports to independent ArchiveTool -png conversions.
Pass CLI, game root, ArchiveTool.exe. All copies and generated files stay local.
"""
import hashlib,json,os,shutil,subprocess,sys,tempfile
from pathlib import Path
from PIL import Image
cli,game,tool=map(Path,sys.argv[1:])
gd=game/"resource/gd_PC"
env=dict(os.environ);env.setdefault("DOTNET_ROLL_FORWARD","Major")
alpha=0
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for query,stem in [(20,"bsplash"),(18,"bsplashdl"),(34,"bsplashlb1")]:
        copy=Path(tmp).resolve()/(stem+".prs");shutil.copyfile(gd/copy.name,copy)
        subprocess.run([tool,"-png",copy],env=env,check=True)
        output=copy.with_suffix(".json")
        subprocess.run([cli,"--source-root",game,"stage20-splash",gd/"stg20D.rel",output,str(query)],check=True)
        report=json.loads(output.read_text());folder=copy.with_suffix("")
        assert hashlib.sha256(copy.read_bytes()).hexdigest()==report["archiveSha256"]
        entries=(folder/"index.txt").read_text(encoding="utf-8-sig").splitlines()
        assert len(entries)==len(report["textures"])==10
        for i,(line,row) in enumerate(zip(entries,report["textures"])):
            gid,name,size=line.split(",")
            with Image.open(folder/name) as image:
                pixels=image.convert("RGBA").tobytes();w,h=image.size
            n=sum(a!=255 for a in pixels[3::4]);alpha+=n
            assert size==f"{w}x{h}" and row==dict(index=i,globalIndex=int(gid),name=Path(name).stem,
                width=w,height=h,rgbaSha256=hashlib.sha256(pixels).hexdigest(),nonopaquePixels=n)
print(f"30 exact RGBA matches, {alpha} nonopaque pixels")
