"""Run original path stepper with explicit distance and heading service observations."""
import json, os, struct, subprocess, tempfile
from pathlib import Path
import wild_canyon_ppc as native

u, R, base = native.u, native.R, native.base
paths = native.emerald_paths()
scratch = native.stage + 1200
def fp(x):
    return int.from_bytes(struct.pack(">d", x), "big")
def observe(cpu, pc, size, _):
    site = pc - base[1]
    if site in (0x21844, 0x218ac, 0x21a50, 0x21aa0, 0x21c50):
        cpu.reg_write(R.UC_PPC_REG_3, path_index)
    elif site in (0x21914, 0x21b08, 0x219d8, 0x219ec, 0x21bd0, 0x21be4):
        idx, rem = divmod(native.reg(3) - start, 12)
        assert rem == 0 and native.reg(4) == start + 12*((idx+1)%len(points))
        if site in (0x21914, 0x21b08):
            cpu.reg_write(R.UC_PPC_REG_FPR1, fp(lengths[idx]))
        else:
            cpu.reg_write(R.UC_PPC_REG_3, headings[idx] & 65535)
    cpu.reg_write(R.UC_PPC_REG_PC, pc+4)
for site in (0x21820,0x21c94,0x21844,0x218ac,0x21a50,0x21aa0,0x21c50,
             0x21914,0x21b08,0x219d8,0x219ec,0x21bd0,0x21be4):
    u.hook_add(native.UC_HOOK_CODE, observe, begin=base[1]+site, end=base[1]+site)
cases=[]
for path_index,path in enumerate(paths):
    points=path["points"];start=base[5]+path["pointsOffset"]
    lengths=[8.+(i%5) for i in range(len(points))]
    headings=[(32760,-32760,-1234)[i%3] for i in range(len(points))]
    for seg in (0,len(points)-1):
        for dist in (0., 0.25, lengths[seg]/2, 33., -0.25,-lengths[seg]/2,-33.):
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
            cases.append([points,lengths,headings,seg,dist,[idx,f,[x,y,z],h,bool(native.reg(3))]])
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    file=Path(tmp)/"oracle.json";file.write_text(json.dumps(cases))
    subprocess.run(["cargo","test","--locked","--manifest-path","asset-tools/crates/sa2/Cargo.toml",
                    "--test","emerald_path","--","--ignored"],check=True,
                   env=os.environ|{"SA2_PATH_ORACLE":str(file.resolve())})
print(len(cases),"path cursor, position, heading and wrap results matched")
