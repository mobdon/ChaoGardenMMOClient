"""Compare kart ring collection, charge writes and respawn with CartD."""
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
        if t==1 and k in (4,6) and (0xa824<=l<0xaa64 or 0x1714<=l<0x17e8 or 0x1854<=l<0x190c):
            if mod!=80 and a not in (0x80175378,0x803adad4):continue
            p=base[s]+a if mod==80 else (0x85004000 if a==0x80175378 else 0x85004100)
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
ring,tw,col=ram+0x100,ram+0x200,ram+0x400
put(ring+0x34,"I",tw);put(tw+0x2c,"I",col)
def hook(m,a,*_):
    pc=a-base[1];r3,r4,r5,r6=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5,6)]
    if pc in (0x1884,0x18cc):
        assert r3==tw+20
        assert r4==ram+(0x4300 if pc==0x1884 else 0x4400)
    elif pc in (0x188c,0x18d4):
        metric=q["first_range_metric"] if pc==0x188c else q["second_range_metric"]
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",metric))[0])
    elif pc==0xa8c8:
        assert r3==ring
        calls.extend([dict(install_callbacks=[x-base[1] for x in get(ring+20,"II")]),
                      dict(init_collision=dict(section5_offset=r4-base[5],args=[r5,r6]))])
    elif pc==0xa994:calls.append(dict(engine80063584=[r3,r4]))
    elif pc==0xa9a8:calls.append(dict(engine8006b710=[r3,r4,r5,r6]))
    else:
        assert r3==ring
        calls.append({0xa9e0:"collection_effect_ab04",0xa8f4:"register_collision",0xa9d4:"register_collision",
                      0xaa2c:"engine80065148",0xaa38:"engine80071888"}[pc])
    m.reg_write(R.UC_PPC_REG_PC,a+4)
for pc in (0x1884,0x18cc,0x188c,0x18d4,0xa8c8,0xa8f4,0xa994,0xa9a8,0xa9d4,0xa9e0,0xaa2c,0xaa38):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
limit=get(base[4]+0x1c0,"f")[0]
put(ram+0x4000,"II",ram+0x4300,ram+0x4400)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,mode in enumerate((0,1,2,2,2,2,2,3,4,4,4,5,5,8,0,2)):
        state=dict(mode=mode,timer=0 if i==8 else 1 if i==9 else 320,yaw=2147483600,scale_z=.5 if i%2 else 4)
        registry=[None]*6
        for slot in (0,3,5):
            task=ram+0x1000+slot*256;work=ram+0x2000+slot*512
            registry[slot]=dict(task=task,work_44=int(slot!=3),work_70=int(slot==5),work_d8=19 if slot==0 else 2147483647)
            put(task+0x38,"I",work);put(work+0x44,"i",int(slot!=3));put(work+0x70,"i",int(slot==5))
            put(work+0xd8,"i",registry[slot]["work_d8"])
        put(base[6]+0x21428,"6I",*(k["task"] if k else 0 for k in registry))
        contacts=([0,1234] if i==2 else [registry[3]["task"]] if i==3 else
                  [registry[j]["task"] for j in (5,0,3,0)])
        q=dict(state=state,registry=registry,collision_flags=0 if i==4 else 1,contacts=contacts,
               first_range_metric=limit if i in (6,8,9,14,15) else 0,view_count=2 if i in (6,15) else 1,
               second_range_metric=limit if i==15 else 0,has_entry=bool(i%2))
        put(tw,"b",mode);put(tw+6,"H",state["timer"]);put(tw+12,"i",state["yaw"]);put(tw+40,"f",state["scale_z"])
        put(ring+0x30,"I",int(q["has_entry"]));put(ram+0x4100,"i",q["view_count"])
        put(col+2,"hH",len(contacts),q["collision_flags"])
        for j,t in enumerate(contacts):put(col+0x14+8*j,"I",t)
        req=Path(td)/"req.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-ring-pickup",rel,out,req],check=True);got=json.loads(out.read_text());calls=[]
        u.reg_write(R.UC_PPC_REG_3,ring)
        u.emu_start(base[1]+0xa838,base[1]+0xaa4c,count=500)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xaa4c
        assert calls==got["calls"],(i,calls,got)
        assert got["state"]==dict(mode=get(tw,"b")[0],timer=get(tw+6,"H")[0],yaw=get(tw+12,"i")[0],scale_z=get(tw+40,"f")[0])
        for j,k in enumerate(registry):
            if k:k["work_d8"]=get(ram+0x2000+j*512+0xd8,"i")[0]
        assert got["registry"]==registry
        if i==5:
            charge=got["registry"][0]["work_d8"];assert charge==21
            boost=dict(work_70=0,work_44=1,work_d8=charge,work_dc=0,slot_flag=0,buttons_pressed=0,position=[0,0,0])
            req.write_text(json.dumps(dict(profile=0,alternate=False,velocity=[0,0,0],boost=boost)))
            dest=Path(td)/"boost.json";subprocess.run([cli,"kart-active-profile",rel,dest,req],check=True)
            b=json.loads(dest.read_text())["boost"];assert (b["slot_flag"],b["work_d8"])==(1,0)
print("16 pickup task updates matched CartD; collected charge armed the boost.")
