"""Start two real offline Rust processes. Requires A2NE_ROM and A2NE_MEMORY_SESSION_INPUTS.
Usage: python tests/garden_cartridge_roundtrip.py /path/to/a2ne-garden .local/proof
Only a new temporary subdirectory below the supplied private directory is written.
"""
import hashlib,itertools,json,os,struct,subprocess,sys,tempfile
from pathlib import Path

def check(exe,root):
    root=Path(root).resolve()
    if ".local" not in root.parts:raise ValueError("proof output must be private .local")
    root.mkdir(parents=True,exist_ok=True)
    rom=Path(os.environ["A2NE_ROM"])
    trace=Path(os.environ["A2NE_MEMORY_SESSION_INPUTS"]).read_bytes()
    assert hashlib.sha256(trace).hexdigest()=="2d579d408af0b18e73681ae48400060300b13f01826084a89f14106780dd8a42"
    seed,frames=struct.unpack_from("<II",trace)
    assert (seed,frames)==(0x3039,1775)
    def run(input_flash,commands):
        p=subprocess.run([exe,str(rom),str(input_flash),"0"],input="\n".join(commands+["quit",""]),capture_output=True,text=True,timeout=30,check=True)
        return [json.loads(v) for v in p.stdout.splitlines()]
    with tempfile.TemporaryDirectory(dir=root) as tmp:
        target=Path(tmp)/"round save.sav"
        controls=[f"pad {k} {len(list(group))}" for k,group in itertools.groupby(trace[8:8+frames*16:16])]
        states=run("-",["memory",*controls,"hatch","shop","buy fruit 0","feed 0",f"save {target}",f"save {target}","status"])
        assert all("error" not in s for s in states[:-2]) and "error" in states[-2]
        assert states[-1]==states[-3], "refused overwrite changed live cartridge state"
        won=next(s for s in states if s.get("lastResult"))
        assert (won["scene"],won["lastResult"],won["wallet"])==("garden",["memory",True,90],90)
        save=states[-3]
        assert save["wallet"]==60 and save["chao"] and save["growth"] and save["fruitSlots"]==[None]*8
        before=target.read_bytes()
        restored,active,refused=run(target,["bounce",f"save {Path(tmp)/'blocked.sav'}"])
        assert active["scene"]=="bounce" and "error" in refused and not (Path(tmp)/"blocked.sav").exists()
        for key in ("chao","fruitSlots","wallet","generation","growth"):assert restored[key]==save[key]
        assert target.read_bytes()==before and len(before)==65536
    print("Cartridge runner: Memory +90, hatch, fruit -30, feed, save. Fresh process: same Chao fields and 60 rings.")
if __name__=="__main__":check(sys.argv[1],sys.argv[2])
