"""Compare four ring effect spawns and allocated particle writes with CartD."""
from pathlib import Path
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
        delta,k,s,a=struct.unpack_from(">HBBI",data,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t==1 and k in (4,6) and (0xab04<=l<0xac40 or 0xf5e8<=l<0xf918):
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

def setfloat(m,v):m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",v))[0])
def hook(m,addr,*_):
    pc=addr-base[1];a=m.reg_read(R.UC_PPC_REG_3)
    if pc==0xf83c:
        assert a==(current["phase"]&0xffffffff);setfloat(m,sine)
    elif pc in (0xab20,0xf5fc):
        regs=range(25 if pc==0xab20 else 27,32)
        stack.append([(i,m.reg_read(getattr(R,f"UC_PPC_REG_{i}"))) for i in regs])
    elif pc in (0xac30,0xf774):
        for i,v in stack.pop():m.reg_write(getattr(R,f"UC_PPC_REG_{i}"),v)
    elif pc in (0xab98,0xabe8):
        assert a==len(seen)*0x4000
        setfloat(m,inputs["trig_xz"][len(seen)][pc==0xabe8])
    elif pc==0xac08:
        idx=len(seen);seen.append(idx);v=got[idx]
        assert list(get(a,"3f"))==v["position"] and list(get(m.reg_read(R.UC_PPC_REG_4),"3f"))==v["velocity"]
        assert struct.unpack(">d",struct.pack(">Q",m.reg_read(R.UC_PPC_REG_FPR1)))[0]==v["scale"]
        assert v["angle"]==idx*0x4000
        return
    elif pc==0xf614:
        assert a==base[5]+0x2ee6d0
        idx=seen[-1];m.reg_write(R.UC_PPC_REG_3,ram+0x5000+128*idx if allocations[idx] is not None else 0)
    else:
        idx=seen[-1];j=(0xf658,0xf6d0,0xf72c).index(pc)
        assert draws[idx]==j;draws[idx]+=1
        m.reg_write(R.UC_PPC_REG_3,allocations[idx][j])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for site in (0xf83c,0xab20,0xf5fc,0xac30,0xf774,0xab98,0xabe8,0xac08,0xf614,0xf658,0xf6d0,0xf72c):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for mask in range(16):
        allocations=[[i*8191,32767-i,mask*71] if mask&(1<<i) else None for i in range(4)]
        inputs=dict(position=[.125,-47,101.5],trig_xz=[[0,1],[1,0],[0,-1],[-1,0]],allocation_draws=allocations)
        q=dict(state=dict(mode=3,timer=19,yaw=0,scale_z=1),registry=[None]*6,collision_flags=0,contacts=[],
               first_range_metric=0,view_count=1,has_entry=False,collection=inputs)
        req=Path(td)/"input.json";out=Path(td)/f"{mask}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-ring-pickup",rel,out,req],check=True);got=json.loads(out.read_text())["collection"]
        put(tw+20,"3f",*inputs["position"]);u.mem_write(ram+0x5000,bytes([0xab])*512)
        seen=[];draws=[0]*4;stack=[]
        u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0xab04,base[1]+0x200,count=1200)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200 and seen==list(range(4)) and not stack
        for i,v in enumerate(got):
            expected=bytearray([0xab])*128;w=v["writes"]
            if allocations[i] is None:assert w is None and draws[i]==0
            else:
                assert draws[i]==3
                for off,fmt,vals in ((0,"if",[w["word_00"],v["scale"]]),(8,"3f",v["position"]),(0x20,"3f",v["velocity"]),
                                     (0x18,"f",[w["float_18"]]),(0x2c,"i",[w["word_2c"]]),(0x38,"I",[w["word_38"]])):
                    struct.pack_into(">"+fmt,expected,off,*vals)
            assert expected==u.mem_read(ram+0x5000+i*128,128),(mask,i)
print("16 allocation combinations matched all ring-effect constructor writes and RNG calls.")

with tempfile.TemporaryDirectory(dir=tmp) as td:
    ptr=ram+0x5000;desc=base[5]+0x2ee6d0
    layout=dict(angle=(0,"i"),scale=(4,"f"),position=(8,"3f"),color=(20,"I"),frame=(24,"f"),
                orientation=(28,"i"),velocity=(32,"3f"),spin=(44,"i"),phase=(56,"i"))
    put(ptr+20,"I",0xfbac3409);put(ptr+28,"i",0)
    for frame in range(256):
        current={}
        for key,(off,fmt) in layout.items():
            vals=get(ptr+off,fmt);current[key]=list(vals) if len(vals)>1 else vals[0]
        sine=(frame%7-3)/4
        req=Path(td)/"input.json";out=Path(td)/f"{frame}.json"
        req.write_text(json.dumps(dict(particle=current,phase_sine=sine)))
        subprocess.run([cli,"kart-ring-particle",rel,out,req],check=True);step=json.loads(out.read_text())
        u.reg_write(R.UC_PPC_REG_3,desc);u.reg_write(R.UC_PPC_REG_4,ptr);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0xf784,base[1]+0x200,count=140)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200 and step["alive"]==bool(u.reg_read(R.UC_PPC_REG_3))
        for key,(off,fmt) in layout.items():
            v=step["particle"][key]
            assert bytes(u.mem_read(ptr+off,struct.calcsize(">"+fmt)))==struct.pack(">"+fmt,*(v if isinstance(v,list) else [v])),(frame,key)
        assert get(ptr+20,"I")[0]&0xffffff==0xac3409
        if not step["alive"]:break
    else:raise AssertionError("particle never expired")
print(f"Matched {frame+1} consecutive native particle updates through expiry.")
