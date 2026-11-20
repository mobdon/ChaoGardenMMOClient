"""Compare kart ring display commands, local geometry and decoded texture with CartD."""
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
        if t==1 and k in (4,6) and (0xaa64<=l<0xab00 or 0x17e8<=l<0x1854):
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
    vals=[struct.unpack(">d",struct.pack(">Q",m.reg_read(getattr(R,f"UC_PPC_REG_FPR{i}"))))[0] for i in (1,2,3)]
    if pc==0x1818:assert (a,b)==(tw+20,ram+0x4300+active*16)
    elif pc==0x1820:m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",metric))[0])
    elif pc==0xaaa0:calls.append(("texture",a-base[5]))
    elif pc==0xaaa4:calls.append("push")
    elif pc in (0xaab8,0xaad8):
        assert a==0
        calls.append(("position" if pc==0xaab8 else "scale",vals))
    elif pc==0xaac4:
        assert a==0
        calls.append(("yaw",struct.unpack(">i",struct.pack(">I",b))[0]))
    elif pc==0xaae4:calls.append(("model",a-base[5]))
    else:calls.append("pop")
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x1818,0x1820,0xaaa0,0xaaa4,0xaab8,0xaac4,0xaad8,0xaae4,0xaae8):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
limit=get(base[4]+0x1c0,"f")[0]
put(ram+0x4000,"II",ram+0x4300,ram+0x4310)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(mode,metric) in enumerate([(0,0),(1,0),(2,0),(2,limit-1),(2,limit),(2,limit+1),(3,0),(4,0),(5,0)]):
        active=i%2;put(ram+0x4100,"I",active)
        q=dict(mode=mode,position=[-10.5,7,42],scale_z=i*.125,yaw=-123456+i,current_view_metric=metric)
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-ring-display",rel,out,req],check=True);r=json.loads(out.read_text())["draw"]
        put(tw,"b",mode);put(tw+20,"3f",*q["position"]);put(tw+12,"i",q["yaw"]);put(tw+40,"f",q["scale_z"])
        calls=[];u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0xaa64,base[1]+0x200,count=200);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        assert calls==([] if r is None else [("texture",r["textureList"]),"push",("position",r["position"]),
                    ("yaw",r["yaw"]),("scale",r["scale"]),("model",r["attach"]),"pop"])
        if r is None:continue
        g=r["geometry"];v=g["chunks"][0]["vertices"];assert len(v)==32
        for j,row in enumerate(v):
            assert struct.pack(">6f",*row["position"],*row["normal"])==u.mem_read(base[5]+0x2ef340+j*24,24)
        poly=g["polygons"];assert [x["chunk"]["kind"] for x in poly]==[23,8,64]
        assert poly[-1]["material"]["textureId"]==0 and poly[-1]["material"]["diffuseRgba"]==[178,178,178,255]
        for cmd in poly:
            c=cmd["chunk"];words=c["rawWords"]
            assert bytes(u.mem_read(base[5]+c["offset"],c["length"]))==struct.pack(f">{len(words)}H",*words)
        t=r["texture"];bank=(rel.parent/"PRS"/"objtex_common.pak").read_bytes()
        dds=bank[t["offset"]:][:t["length"]];img=Image.open(io.BytesIO(dds)).convert("RGBA")
        assert t["entry"].lower()==r"objtex_common\sikake_05_64.dds"
        assert img.size==(64,64) and hashlib.sha256(img.tobytes()).hexdigest()==t["rgbaSha256"]
print("Nine display callbacks matched; checked 32 native vertices, polygon words and every RGBA pixel.")
