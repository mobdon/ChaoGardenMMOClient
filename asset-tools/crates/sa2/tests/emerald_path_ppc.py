"""Run original path stepper with explicit distance and heading service observations."""
import math, json, os, struct, subprocess, tempfile
from pathlib import Path
import wild_canyon_ppc as native

u, R, base = native.u, native.R, native.base
# Matching City Escape source names both player-slot calls njDistanceP2P.
# Resolve both installed modules' import tables rather than assuming DOL addresses.
city=(Path(os.environ["SA2_GC_REL_DIR"])/"stg13D.rel").read_bytes()
w=lambda i:struct.unpack_from(">I",city,i)[0]
board_imports={}
for module,at in struct.iter_unpack(">II",city[w(40):w(40)+w(44)]):
    offset=0
    while True:
        inc,kind,section,target=struct.unpack_from(">HBBI",city,at);at+=8
        if kind==203:break
        if kind==202:destination=section;offset=0;continue
        offset+=inc
        if module==0 and destination==1 and kind==10:board_imports[offset]=target
assert board_imports[0x9ac]==board_imports[0xa40]==native.engines[0x21914]==native.engines[0x21b08]==0x8011b65c
paths = native.emerald_paths()
scratch = native.stage + 1200
def fp(x):
    return int.from_bytes(struct.pack(">d", x), "big")
def observe(cpu, pc, size, _):
    site = pc - base[1]
    if site in (0x21844, 0x218ac, 0x21a50, 0x21aa0, 0x21c50):
        cpu.reg_write(R.UC_PPC_REG_3, path_index)
    elif site in (0x21914, 0x21b08, 0x217d8):
        idx, rem = divmod(native.reg(3) - start, 12)
        assert rem == 0 and native.reg(4) == start + 12*((idx+1)%len(points))
        if site in (0x21914, 0x21b08):
            cpu.reg_write(R.UC_PPC_REG_FPR1, fp(lengths[idx]))
        else:
            inputs = [cpu.reg_read(r) for r in (R.UC_PPC_REG_FPR1, R.UC_PPC_REG_FPR2)]
            calls.append([idx, inputs])
            cpu.reg_write(R.UC_PPC_REG_FPR1, fp(angles[idx]))
    cpu.reg_write(R.UC_PPC_REG_PC, pc+4)
for site in (0x21820,0x21c94,0x21844,0x218ac,0x21a50,0x21aa0,0x21c50,
             0x21914,0x21b08,0x217d8):
    u.hook_add(native.UC_HOOK_CODE, observe, begin=base[1]+site, end=base[1]+site)
cases=[]
for path_index,path in enumerate(paths):
    points=path["points"];start=base[5]+path["pointsOffset"]
    lengths=[8.+(i%5) for i in range(len(points))]
    angles=[(3.141,-3.141,-0.1234)[i%3] for i in range(len(points))]
    for seg in (0,len(points)-1):
        for dist in (0., 0.25, lengths[seg]/2, 33., -0.25,-lengths[seg]/2,-33.):
            calls=[]
            native.put(scratch,"If",seg,0.5)
            native.put(native.work+32,"f",float(path_index))
            for i,v in enumerate((native.work,scratch,scratch+4,scratch+8,scratch+20),3):
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(i)),v)
            u.reg_write(R.UC_PPC_REG_FPR1,fp(dist))
            u.reg_write(R.UC_PPC_REG_1,native.stage+4000)
            u.reg_write(R.UC_PPC_REG_LR,native.stop)
            u.emu_start(base[1]+0x21808,native.stop,count=10000)
            assert u.reg_read(R.UC_PPC_REG_PC)==native.stop
            idx,f,x,y,z,h=native.get(scratch,"Iffffi")
            assert len(calls)==2
            cases.append([points,lengths,angles,calls,seg,dist,[idx,f,[x,y,z],h,bool(native.reg(3))]])
# Run a persistent cursor for each alternate SET placement (init 0x220ac).
series=[]
f32=lambda v:struct.unpack(">f",struct.pack(">f",v))[0]
for name in ("set0016_s.bin","set0016_u.bin"):
    data=(native.game/"resource/gd_PC"/name).read_bytes()
    table=(native.get(base[1]+0x1fcd6,"H")[0]<<16)+native.get(base[1]+0x1fcda,"h")[0]
    for idx in range(struct.unpack_from(">I",data)[0]):
        packed,rx,ry,rz,*values=struct.unpack_from(">4H6f",data,32+32*idx)
        obj=base[5]+0x7c6f8+(packed&4095)*16
        if native.text(native.get(obj+12,"I")[0]) not in ("EMERALD","EMERALD_F"):continue
        if native.get(table+36*((rx>>8)%11)+8,"I")[0]!=base[1]+0x220ac:continue
        assert (ry,rz)==(0,0)
        path_index=int(values[3])%len(paths);path=paths[path_index];points=path["points"]
        start=base[5]+path["pointsOffset"]
        lengths=[]
        for j,p in enumerate(points):
            sq=[f32(f32(a-b)**2) for a,b in zip(p,points[(j+1)%len(points)])]
            lengths.append(f32(math.sqrt(f32(f32(sq[0]+sq[1])+sq[2]))))
        angles=[(3.141,-3.141,-0.1234)[i%3] for i in range(len(points))]
        native.put(scratch,"If",255,0.0);native.put(native.work+32,"f",float(path_index))
        runs=[]
        constant_addr=((native.get(base[1]+0x22372,"H")[0]<<16)+native.get(base[1]+0x22376,"h")[0])&0xffffffff
        dist=f32(native.get(constant_addr,"f")[0]+values[5])
        for frame in range(64):
            calls=[]
            for i,v in enumerate((native.work,scratch,scratch+4,scratch+8,scratch+20),3):
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(i)),v)
            u.reg_write(R.UC_PPC_REG_FPR1,fp(dist));u.reg_write(R.UC_PPC_REG_1,native.stage+4000)
            u.reg_write(R.UC_PPC_REG_LR,native.stop);u.emu_start(base[1]+0x21808,native.stop,count=10000)
            assert u.reg_read(R.UC_PPC_REG_PC)==native.stop
            seg,f,x,y,z,h=native.get(scratch,"Iffffi")
            world=[f32(v+f32(origin-center)) for v,origin,center in zip((x,y,z),values,path["center"])]
            runs.append([seg,f,[x,y,z],h,bool(native.reg(3)),world])
        series.append([points,angles,values[:3],path["center"],dist,runs])
assert len(series)==8
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    file=Path(tmp)/"oracle.json";file.write_text(json.dumps(cases))
    placed=Path(tmp)/"placed.json";placed.write_text(json.dumps(series))
    subprocess.run(["cargo","test","--locked","--manifest-path","asset-tools/crates/sa2/Cargo.toml",
                    "--test","emerald_path","--","--ignored"],check=True,
                   env=os.environ|{"SA2_PATH_ORACLE":str(file.resolve()),"SA2_PLACED_PATHS":str(placed.resolve())})
print(len(cases),"path cursor, position, heading and wrap results matched")
