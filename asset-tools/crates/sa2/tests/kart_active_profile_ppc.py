"""Compare active profile loading and pre-dispatch velocity limiting with CartD."""
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
        if t==1 and mod==80 and k in (4,6) and (0x2284<=l<0x23b4 or 0x2a2c<=l<0x2b14 or 0x5b90<=l<0x5c38):
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))
def normalize(m,addr,*_):
    p=m.reg_read(R.UC_PPC_REG_3)
    assert list(get(p,"3f"))==q["velocity"]
    put(p,"3f",*q["normalize"]["vector"])
    m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",q["normalize"]["magnitude"]))[0])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,normalize,begin=base[1]+0x233c,end=base[1]+0x233c)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
u.reg_write(R.UC_PPC_REG_30,wk)
keys="acceleration brake_power deceleration speed_cap weight field_14 drift_handling speed_threshold field_20 velocity_limit".split()
def event_hook(m,addr,*_):
    if addr==base[1]+0x5be8:
        assert m.reg_read(R.UC_PPC_REG_3)==parent
        calls.append("cart")
    else:
        calls.append([m.reg_read(getattr(R,f"UC_PPC_REG_{j}")) for j in range(3,7)])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x5be8,0x5c0c):u.hook_add(UC_HOOK_CODE,event_hook,begin=base[1]+pc,end=base[1]+pc)
put(parent+0x38,"I",wk)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for alt in (False,True):
        for i in (range(2,8) if alt else range(9)):
            if alt:
                u.reg_write(R.UC_PPC_REG_3,i);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                u.emu_start(base[1]+0x2a2c,base[1]+0x200,count=100)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
            limit=get(base[5]+0x36cc+i*40+36,"f")[0]
            for j in range(4):
                q=dict(profile=i,alternate=alt,velocity=[0,0,0] if j==0 else [4,-3,2])
                q["event"]=dict(work_44=int(j!=0),selector=j%2)
                if j:q["normalize"]=dict(magnitude=limit+j-2,vector=[.25,-.5,.75])
                req=Path(td)/"in.json";out=Path(td)/f"{int(alt)}-{i}-{j}.json";req.write_text(json.dumps(q))
                subprocess.run([cli,"kart-active-profile",rel,out,req],check=True);got=json.loads(out.read_text())
                put(wk+0x48,"i",i);put(wk+0x7c,"3f",*q["velocity"])
                shared=base[6]+0x21468+0x7c;put(shared,"3f",33,44,55)
                u.emu_start(base[1]+0x2284,base[1]+0x23b4,count=100)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x23b4
                assert bytes(u.mem_read(base[6]+0x21440,40))==struct.pack(">10f",*(got[k] for k in keys))
                assert bytes(u.mem_read(wk+0x7c,12))==struct.pack(">3f",*got["velocity"])
                assert bytes(u.mem_read(shared,12))==struct.pack(">3f",*(got["shared_velocity_write"] or [33,44,55]))
                assert list(get(base[5]+0x3924+i*24,"6i"))==got["cue_words"]
                assert (list(get(base[5]+0x3ad4+i*8,"2i")) if i<8 else None)==got["event_pair"]
                calls=[];put(wk+0x44,"i",q["event"]["work_44"])
                u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_4,j%2)
                u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                u.emu_start(base[1]+0x5b90,base[1]+0x200,count=100)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
                event=got["event"]
                assert u.reg_read(R.UC_PPC_REG_3)==event["return_word"]
                assert calls==(["cart",event["engine_args"]] if event["call_cart_979c"] else [])
print("60 active profile and velocity-limit paths matched CartD, including all six alternate physics replacements.")
