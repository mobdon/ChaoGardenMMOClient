"""Compare state 4 spin, heading reset, displacement and ground response with CartD."""
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
        if t==1 and mod==80 and k in (4,6) and (0x30dc<=l<0x3188 or 0x3ec4<=l<0x4070):
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))
def freg(i):
    return struct.unpack(">d",struct.pack(">Q",u.reg_read(getattr(R,f"UC_PPC_REG_FPR{i}"))))[0]
def hook(m,addr,*_):
    pc=addr-base[1];a,b,c=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5)]
    if pc==0x3134:
        assert (freg(1),freg(2))==(z,x)
        ret=q["angle_radians"]
    elif pc==0x3f90:
        assert get(a,"3f")==tuple([x,0,z])
        ret=q["magnitude_return"]
    elif pc==0x3fd8:
        assert a==base[5]+0x3bf8 and get(a+4,"i")[0]==got["heading"]
        assert bytes(u.mem_read(b,12))==struct.pack(">3f",*got["local_displacement"])
        put(b,"3f",*pp["transformed_vector"])
    elif pc==0x3fe8:
        assert a==c==tw+20
        put(c,"3f",*(x+y for x,y in zip(get(a,"3f"),get(b,"3f"))))
    if pc in (0x3134,0x3f90):
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",ret))[0])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x3134,0x3f90,0x3fd8,0x3fe8):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
put(parent+0x34,"II",tw,wk)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,step in enumerate([18,-18,19,-19,17,-17,0,1,-1]):
        x,z=[(3,4),(0,0),(-.5,.25),(.25,-.25)][i%4]
        pp=dict(position=[10,-5,8],velocity_y=[-1,0,2][i%3],weight=.5,work_74=i%2,transformed_vector=[3,.5,-2])
        q=dict(heading=-54321,work_68=2147483645,work_b8=step,velocity_xz=[x,z],
               angle_radians=-.7,magnitude_return=(x*x+z*z)**.5,position_step=pp)
        req=Path(td)/"input.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-spin-drive",rel,out,req],check=True);got=json.loads(out.read_text())
        put(wk+0x30,"i",4);put(wk+0x68,"i",q["work_68"]);put(wk+0xb8,"i",step);put(wk+0x74,"i",pp["work_74"])
        put(tw+12,"i",q["heading"]);put(tw+20,"3f",*pp["position"]);put(wk+0x7c,"3f",x,pp["velocity_y"],z)
        put(base[6]+0x21450,"f",pp["weight"])
        u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0x3ec4,base[1]+0x200,count=300)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        for addr,key in ((tw+12,"heading"),(wk+0x30,"vehicle_state"),(wk+0x68,"work_68"),(wk+0xb8,"work_b8")):
            assert got[key]==get(addr,"i")[0],(i,key)
        assert bytes(u.mem_read(tw+20,12))==struct.pack(">3f",*got["position_step"]["position"])
print("Nine state 4 spin callbacks matched through return.")
