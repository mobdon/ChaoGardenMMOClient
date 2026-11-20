"""Compare state 1 recovery, raw steering, displacement and ground response with CartD."""
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
        if t==1 and mod==80 and k in (4,6) and 0x36b4<=l<0x399c:
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
    if pc==0x36f8:
        assert (freg(1),freg(2))==(q["velocity_xz"][1],q["velocity_xz"][0])
    elif pc==0x372c:assert (a,b)==(q["work_68"]&0xffffffff,0)
    elif pc==0x3834:assert get(a,"3f")==tuple([q["velocity_xz"][0],0,q["velocity_xz"][1]])
    elif pc==0x38d0:
        assert a==base[5]+0x3bcc and get(a+4,"i")[0]==got["heading"]
        assert bytes(u.mem_read(b,12))==struct.pack(">3f",*got["local_displacement"])
        put(b,"3f",*pp["transformed_vector"])
    elif pc==0x38ec:assert (a,b,c)==(q["work_fc"]&0xffffffff,0,364)
    elif pc==0x3910:
        assert a==c==tw+20
        put(c,"3f",*(x+y for x,y in zip(get(a,"3f"),get(b,"3f"))))
    if pc in (0x36f8,0x3834):
        v=q["angle_radians" if pc==0x36f8 else "magnitude_return"]
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",v))[0])
    if pc in (0x372c,0x38ec):m.reg_write(R.UC_PPC_REG_3,q["angle_difference" if pc==0x372c else "ground_angle_return"]&0xffffffff)
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x36f8,0x372c,0x3834,0x38d0,0x38ec,0x3910):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
put(parent+0x34,"II",tw,wk)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i in range(8):
        x,z=(0,0) if i<2 else (3,4);slot=i%2
        pp=dict(position=[10,-5,8],velocity_y=[-1,0,2][i%3],weight=.5,work_74=slot,transformed_vector=[3,.5,-2])
        q=dict(heading=45,work_68=-1093 if slot else 1095,work_fc=2147483646,stick_x=i*9-30,
               velocity_xz=[x,z],angle_radians=-.7,angle_difference=[545,546,547][i%3],
               magnitude_return=5,ground_angle_return=-91,accelerate=i%3==0,brake=i%3!=1,speed_cap=12,
               acceleration=2,brake_power=-.5,deceleration=-.125,position_step=pp)
        req=Path(td)/"input.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-recovery-drive",rel,out,req],check=True);got=json.loads(out.read_text())
        for off,v in ((0x30,1),(0x68,q["work_68"]),(0xfc,q["work_fc"]),(0x74,slot),(0x78,slot),(0x110,int(q["accelerate"])),(0x114,int(q["brake"]))):
            put(wk+off,"i",v)
        put(tw+12,"i",45);put(tw+20,"3f",*pp["position"]);put(wk+0x7c,"3f",x,pp["velocity_y"],z)
        put(base[6]+0x224b8,"i",1);put(base[6]+0x224bc+104+52*slot+28,"h",q["stick_x"])
        put(base[6]+0x21440,"5f",2,-.5,-.125,12,.5)
        u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0x36b4,base[1]+0x200,count=300)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        for addr,key in ((tw+12,"heading"),(wk+0x30,"vehicle_state"),(wk+0x68,"work_68"),(wk+0xfc,"work_fc")):
            assert got[key]==get(addr,"i")[0],(i,key)
        assert bytes(u.mem_read(tw+20,12))==struct.pack(">3f",*got["position_step"]["position"])
print("Eight state 1 callbacks matched through return.")
