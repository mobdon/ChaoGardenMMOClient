"""Compare kart dash display commands, local geometry and decoded texture with CartD."""
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
        if t==1 and k in (4,6) and (0xae40<=l<0xaf34 or 0x17e8<=l<0x1854):
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
    elif pc==0xae64:calls.append("push")
    elif pc==0xae70:
        assert a==0
        calls.append(list(get(b,"3f")))
    elif pc in (0xae7c,0xae88,0xae94):
        assert a==0
        calls.append([{0xae7c:"z",0xae88:"x",0xae94:"y"}[pc],struct.unpack(">i",struct.pack(">I",b))[0]])
    elif pc in (0xaea0,0xaeac,0xaeb8):calls.append(a-base[5])
    else:calls.append("pop")
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x1818,0x1820,0xae64,0xae70,0xae7c,0xae88,0xae94,0xaea0,0xaeac,0xaeb8,0xaebc):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
limit=get(base[4]+0x1c0,"f")[0];put(ram+0x4000,"I",ram+0x4300)
def run(start):
    u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
    u.emu_start(base[1]+start,base[1]+0x200,count=3000);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(phase,metric) in enumerate(((0,0),(16383,limit-1),(60,0),(321,limit),(0,limit+1))):
        q=dict(position=[-.5,22,47],angles=[-1234,23456,77],current_view_metric=metric,last_uv_phase=phase)
        put(tw+6,"H",phase);put(tw+8,"3i",*q["angles"]);put(tw+20,"3f",*q["position"])
        run(0xaed8);calls=[];run(0xae40)
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-dash-display",rel,out,req],check=True);r=json.loads(out.read_text())["draw"]
        assert calls==([] if r is None else ["push",r["position"],*r["rotations"],r["textureList"],*[x["root"] for x in r["models"]],"pop"])
        if r is None:continue
        for model in r["models"]:
            root=base[5]+model["root"];att=get(root+4,"I")[0];assert att-base[5]==model["attach"]
            assert list(get(root+8,"3f"))==model["position"] and list(get(root+32,"3f"))==model["scale"]
            vp=get(att,"I")[0]+8
            for v in model["geometry"]["chunks"][0]["vertices"]:
                x,y,z,argb=get(vp,"3fI");vp+=16;a,red,g,b=argb.to_bytes(4,"big")
                assert v==dict(position=[x,y,z],diffuseRgba=[red,g,b,a])
            for cmd in model["geometry"]["polygons"]:
                c=cmd["chunk"];w=c["rawWords"]
                assert bytes(u.mem_read(base[5]+c["offset"],2*len(w)))==struct.pack(f">{len(w)}H",*w)
        bank=(rel.parent/"PRS"/"course.pak").read_bytes()
        assert set(r["textures"])=={"61","65","66"}
        for t in r["textures"].values():
            dds=bank[t["offset"]:][:t["length"]];rgba=Image.open(io.BytesIO(dds)).convert("RGBA")
            assert hashlib.sha256(rgba.tobytes()).hexdigest()==t["rgbaSha256"],t["entry"]
print("Five native displays matched; both models, animated UV words, vertex colors and three DDS images checked.")
