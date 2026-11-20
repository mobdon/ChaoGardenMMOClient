"""Compare installed SET ring group expansion and child initialization with CartD."""
from pathlib import Path
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp,exe=map(Path,sys.argv[1:])
data=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x82000000,6:0x83000000,5:0x84000000}
for s,addr in base.items():
    row=struct.unpack_from(">I",data,16)[0]+s*8
    off,n=struct.unpack_from(">II",data,row);off &= ~1
    u.mem_map(addr,(n+4095)&~4095)
    if off:u.mem_write(addr,data[off:off+n])
o,n=struct.unpack_from(">II",data,40)
for mod,at in struct.iter_unpack(">II",data[o:o+n]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",data,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t==1 and k in (4,6) and (0xa7b8<=l<0xa824 or 0xbab0<=l<0xbb70):
            if mod!=80 and a not in (0x80174f98,0x803ad6d4):continue
            p=base[s]+a if mod==80 else (0x85004000 if a==0x80174f98 else 0x85004100)
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
ring,tw,col=ram+0x100,ram+0x200,ram+0x400
put(ring+0x34,"I",tw);put(tw+0x2c,"I",col)

def hook(m,a,*_):
    pc=a-base[1];x,y,z,w=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in range(3,7)]
    if pc==0xa7e4:
        assert [x,y]==doc["child"]["loadArgs"] and z-base[1]==doc["child"]["callback"] and w-base[5]==doc["child"]["nameOffset"]
        put(cw,"b",7);m.reg_write(R.UC_PPC_REG_3,child)
    elif pc==0xa810:
        assert get(cw,"b")[0]==doc["child"]["mode"]
        children.append(bytes(u.mem_read(cw+20,12)))
        return
    elif pc==0xbb1c:
        assert x==z and y==tw+32
        put(z,"3f",*(a+b for a,b in zip(get(x,"3f"),get(y,"3f"))))
    else:
        assert x==ring and pc==0xbb40 and doc["parentRelease"]==0x80065148
        releases.append(pc)
    m.reg_write(R.UC_PPC_REG_PC,a+4)
for pc in (0xa7e4,0xa810,0xbb1c,0xbb40,0xbb4c):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
put(child+0x34,"I",cw);put(ring+0x30,"I",ram+0x5000)
totals={};group_count=0
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for path in sorted(rel.parent.glob("setCart*.bin")):
        out=Path(td)/(path.stem+".json")
        subprocess.run([cli,"kart-ring-groups",rel,out,exe,path],check=True)
        doc=json.loads(out.read_text());raw=path.read_bytes()
        placements=list(struct.iter_unpack(">H3h6f",raw[32:]))
        indices=[j for j,row in enumerate(placements) if (row[0]&4095)==12]
        assert indices==[g["placement"] for g in doc["groups"]]
        total=0
        for idx,g in zip(indices,doc["groups"]):
            row=placements[idx];put(tw,"B",0);put(tw+8,"3i",*row[1:4])
            put(tw+20,"6f",*row[4:]);assert g["setOffset"]==32+32*idx
            children=[];releases=[];u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
            u.emu_start(base[1]+doc["groupCallback"],base[1]+0x200,count=2000)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200 and releases==[0xbb40]
            assert children==[struct.pack(">3f",*xyz) for xyz in g["positions"]]
            total+=len(children);group_count+=1
        assert total==doc["ringCount"];totals[path.name]=total
assert len(totals)==8 and sum(totals.values())==928
print(group_count,"SET groups,",sum(totals.values()),"native child poses matched across eight files:",totals)
