"""Compare active profile loading and pre-dispatch velocity limiting with CartD."""
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
        if t==1 and k in (4,6) and (0x2284<=l<0x23b4 or 0x2a2c<=l<0x2b14 or 0x5b90<=l<0x5c38 or 0x979c<=l<0x9814 or 0x4eb4<=l<0x5090):
            if mod!=80 and a!=0x801e55d8:continue
            p=base[s]+a if mod==80 else 0x85004000
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
        return
    elif addr in (base[1]+0x97ec,base[1]+0x97fc):
        calls.append(struct.unpack(">i",struct.pack(">I",m.reg_read(R.UC_PPC_REG_3)))[0])
    else:
        calls.append([m.reg_read(getattr(R,f"UC_PPC_REG_{j}")) for j in range(3,7)])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x5be8,0x97ec,0x97fc,0x5c0c):u.hook_add(UC_HOOK_CODE,event_hook,begin=base[1]+pc,end=base[1]+pc)
put(parent+0x38,"I",wk)
def boost_hook(m,addr,*_):
    a,b,c,d=[m.reg_read(getattr(R,f"UC_PPC_REG_{n}")) for n in range(3,7)]
    if addr==base[1]+0x501c:
        assert a==wk+0x7c and list(get(a,"3f"))==got["velocity"]
        put(a,"3f",*boost["normalized_velocity"])
    elif addr==base[1]+0x4fb4:calls.append(dict(effect800bf6a4=dict(slot=a,kind=b,position=list(get(c,"3f")),velocity=list(get(d,"3f")))))
    else:calls.append(dict(engine8002f76c=[a,b,c,d]))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x501c,0x4fb4,0x5074):u.hook_add(UC_HOOK_CODE,boost_hook,begin=base[1]+pc,end=base[1]+pc)
put(parent+0x34,"I",tw);put(tw+20,"3f",3,4,5)
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
                q["event"]=dict(work_44=int(j!=0),selector=j%2,piece_id=[49,50,71,72,128,255][(i+j)%6])
                if j:q["normalize"]=dict(magnitude=limit+j-2,vector=[.25,-.5,.75])
                slot=-1 if j==0 else i%2
                boost=dict(work_70=slot,work_d8=[40,19,20,21][j],work_dc=[0,0,0,1 if i%2 else -1][j],
                           slot_flag=int(j==1 and i%2==0),buttons_pressed=(0x201 if i%3!=1 else 0x100),
                           normalized_velocity=[-.25,.5,.75],position=[3,4,5],work_44=int(i!=3),piece_id=q["event"]["piece_id"])
                q["boost"]=boost
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
                put(wk+0xbc,"i",3);put(base[6]+0x24d84+12,"I",5);put(base[6]+12,"I",ram+0x3000)
                put(ram+0x3005,"B",q["event"]["piece_id"])
                calls=[];put(wk+0x44,"i",q["event"]["work_44"])
                u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_4,j%2)
                u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                u.emu_start(base[1]+0x5b90,base[1]+0x200,count=100)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
                event=got["event"]
                assert u.reg_read(R.UC_PPC_REG_3)==event["return_word"]
                if event["call_cart_979c"]:
                    mode=event["course_mode"]
                    assert calls==[mode["engine_8006c97c_arg"],event["engine_args"]]
                    assert mode["piece_id"]==q["event"]["piece_id"]
                    u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                    u.emu_start(base[1]+0x979c,base[1]+0x200,count=40)
                    assert u.reg_read(R.UC_PPC_REG_3)==mode["return_word"]
                else:assert not calls and "course_mode" not in event
                flag=base[6]+0x22498+4*max(0,slot)
                put(flag,"i",boost["slot_flag"]);put(wk+0x44,"i",boost["work_44"]);put(wk+0x70,"i",slot)
                put(wk+0xd8,"ii",boost["work_d8"],boost["work_dc"]);put(ram+0x4000+4*max(0,slot),"I",ram+0x4100)
                put(ram+0x4110,"I",boost["buttons_pressed"])
                calls=[];u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                u.emu_start(base[1]+0x4eb4,base[1]+0x200,count=200)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
                b=got["boost"];assert list(get(wk+0xd8,"ii"))==[b["work_d8"],b["work_dc"]]
                assert get(flag,"i")[0]==b["slot_flag"]
                for k,at in (("acceleration",0),("speed_cap",12)):
                    assert struct.pack(">f",b[k])==bytes(u.mem_read(base[6]+0x21440+at,4))
                assert struct.pack(">3f",*b["velocity"])==bytes(u.mem_read(wk+0x7c,12))
                expected=[]
                for call in b["calls"]:
                    if "profile_event" in call:
                        e=call["profile_event"]
                        if e["call_cart_979c"]:expected.extend([e["course_mode"]["engine_8006c97c_arg"],e["engine_args"]])
                    else:expected.append(call)
                assert calls==expected,(i,j,calls,expected)
print("60 active profile, velocity-limit and boost paths matched CartD, including all six alternate physics replacements.")
