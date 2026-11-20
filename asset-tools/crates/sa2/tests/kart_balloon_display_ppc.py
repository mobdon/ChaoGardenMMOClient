"""Compare story balloon display commands, local geometry and decoded texture with CartD."""
from pathlib import Path
from gsne8p import steam_rel
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
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
        delta,k,s,a=steam_rel(struct.unpack_from(">HBBI", data, at), mod);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t==5 and mod==80 and k==1 and s in base:u.mem_write(base[5]+l,struct.pack(">I",base[s]+a))
        if t==1 and k in (4,6) and (0xa650<=l<0xa7b4 or 0x17e8<=l<0x1854):
            if mod!=80 and a not in (0x80175378,0x803adad0):continue
            p=base[s]+a if mod==80 else (0x85004000 if a==0x80175378 else 0x85004100)
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
ring,tw,col=ram+0x100,ram+0x200,ram+0x400
put(ring+0x34,"I",tw);put(tw+0x2c,"I",col)

from PIL import Image
import io,hashlib

def hook(m,addr,*_):
    pc=addr-base[1];a,b,c=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5)]
    if pc==0x1818:assert (a,b)==(tw+20,ram+0x4300)
    elif pc==0x1820:m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",metric))[0])
    elif pc in (0xa700,0xa73c):
        calls.append([struct.unpack(">d",struct.pack(">Q",m.reg_read(getattr(R,f"UC_PPC_REG_FPR{i}"))))[0] for i in range(1,5)])
    elif pc in (0xa6ac,0xa720):calls.append([struct.unpack(">i",struct.pack(">I",a))[0],b])
    elif pc in (0xa74c,0xa774):calls.append(a-base[5])
    elif pc in (0xa754,0xa768):calls.append(list(get(a,"3f")))
    elif pc==0xa760:calls.append(struct.unpack(">i",struct.pack(">I",b))[0])
    else: calls.append(pc)
    m.reg_write(R.UC_PPC_REG_PC,addr+4)

for pc in (0x1818,0x1820,0xa68c,0xa690,0xa698,0xa6a0,0xa6ac,0xa700,0xa70c,0xa714,
           0xa720,0xa73c,0xa740,0xa74c,0xa754,0xa760,0xa768,0xa774,0xa778,0xa78c,0xa790,0xa798,0xa79c):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
put(ram+0x4000,"I",ram+0x4300)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(mode,timer,metric) in enumerate([(2,0,0),(3,0,3999999),(4,0,0),(4,17,0),(4,60,0),(5,0,0),(3,0,4000000)]):
        q=dict(position=[-.5,22,47],yaw=-23456,scale=[.5,1.25,2],current_view_metric=metric,mode=mode,timer=timer)
        put(tw,"B",mode);put(tw+6,"H",timer);put(tw+12,"i",q["yaw"])
        put(tw+20,"3f",*q["position"]);put(tw+32,"3f",*q["scale"])
        calls=[];u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0xa650,base[1]+0x200,count=3000)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-balloon-display",rel,out,req],check=True);r=json.loads(out.read_text())["draw"]
        if r is None:assert not calls;continue
        setup=[0xa68c,0xa690,0xa698,0xa6a0] if mode==4 else [0xa70c,0xa714]
        restore=[0xa78c,0xa790] if mode==4 else [0xa798,0xa79c]
        # Compare at native f32 precision (JSON prints the shortest round-trip decimal).
        multiplier=list(struct.unpack(">4f",struct.pack(">4f",*r["materialMultiplier"])))
        assert calls==[*setup,r["controlMasks"],multiplier,0xa740,r["textureList"],r["position"],
                       r["yaw"],r["scale"],r["object"],0xa778,*restore],(i,calls)
        root=base[5]+r["object"];node=r["node"]
        assert get(root+44,"II")== (0,0) and get(root+4,"I")[0]==base[5]+r["attach"]
        assert (node["flags"],node["position"],node["angles"],node["scale"])==(get(root,"I")[0],list(get(root+8,"3f")),list(get(root+20,"3i")),list(get(root+32,"3f")))
        for cmd in r["geometry"]["polygons"]:
            c=cmd["chunk"];w=c["rawWords"]
            assert bytes(u.mem_read(base[5]+c["offset"],2*len(w)))==struct.pack(f">{len(w)}H",*w)
        t=r["texture"];bank=(rel.parent/"PRS"/"objtex_common.pak").read_bytes()
        rgba=Image.open(io.BytesIO(bank[t["offset"]:][:t["length"]])).convert("RGBA")
        assert hashlib.sha256(rgba.tobytes()).hexdigest()==t["rgbaSha256"]
print("Seven native balloon displays matched; object hierarchy, polygon words and original RGBA verified.")