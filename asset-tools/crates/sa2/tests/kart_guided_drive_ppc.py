"""Compare state 7 movement, speed limits, waypoint advance and rendered yaw with CartD."""
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
        if t==1 and k in (4,6) and (0x42c8<=l<0x46f8 or 0x5ce8<=l<0x5e78 or 0x12c<=l<0x140):
            p=base[s]+a if mod==80 else 0x85003000
            assert mod==80 or a==0x801cbd88
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))
def freg(i):
    return struct.unpack(">d",struct.pack(">Q",u.reg_read(getattr(R,f"UC_PPC_REG_FPR{i}"))))[0]
def hook(m,a,*_):
    pc=a-base[1]
    r3,r4,r5=[u.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5)]
    if pc==0x4368:
        a3,b3=get(r3,"3f"),get(r4,"3f");put(r5,"3f",*(x-y for x,y in zip(a3,b3)))
    elif pc==0x4374:
        assert [freg(1),freg(2)]==got["angle_input_zx"]
    elif pc==0x44b8:
        assert r3==(got["target_angle"]&0xffffffff) and r4==g["work_274"]
        assert freg(31)==struct.unpack(">f",struct.pack(">f",got["speed_cap"]))[0]
    elif pc==0x4530:assert (r3,r4,r5)==(g["work_274"],got["target_angle"]&0xffffffff,1092)
    elif pc in (0x453c,0x4558):assert r3==e["stepped_angle"]
    elif pc==0x45a4:assert list(get(r3,"3f"))==[got["angle_input_zx"][1],0,got["angle_input_zx"][0]]
    rets={0x4374:e["angle_radians"],0x453c:.25,0x4558:-.5,0x45a4:e["distance_return"]}
    if pc in rets:u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",rets[pc]))[0])
    if pc in (0x44b8,0x4530,0x45f8):u.reg_write(R.UC_PPC_REG_3,{0x44b8:e["angle_difference"],0x4530:e["stepped_angle"],0x45f8:e["random_return"]}[pc]&0xffffffff)
    m.reg_write(R.UC_PPC_REG_PC,a+4)
for site in (0x4368,0x4374,0x44b8,0x4530,0x453c,0x4558,0x45a4,0x45f8):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+site,end=base[1]+site)
put(parent+0x34,"II",tw,wk)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i in range(8):
        g=dict(work_254=4,work_258=i%4,work_25c=3,work_260=2,work_264=[0,99,2,5][i%4],work_274=222)
        e=dict(angle_radians=-.7,angle_difference=[363,20,364,700][i%4],stepped_angle=444,direction_xz=[.25,-.5],
               distance_return=[400,399,401,0][i%4],random_return=[0,10000,20000,0][i%4])
        q=dict(guidance=g,segment=4 if i>=4 else 1,segment_count=5 if i%2 else 9,quad_segment=4,quad=[[4,8],[6,2],[5,-3],[7,11]],
               position=[-3,15,5],previous_ground_position=[1,2,3],velocity_y=3,work_74=i%2,place=[1,2,-1,4][i%4],
               player_slot=i%2,lap=i%3,acceleration=1,deceleration=-4,weight=.5,engine=e)
        mode=[0,2,3,4,5,5,1,6][i]
        previous=list(get(base[5]+0x36c0,"3f"))
        setup=dict(mode=mode,previous_tuning=previous)
        put(base[6]+0x1a0,"i",mode)
        before=bytes(u.mem_read(base[5]+0x380c,40))
        if mode==5:
            words=list(struct.unpack(">10I",struct.pack(">10f",*(j+.25 for j in range(10)))))
            setup["mode5"]=dict(engine_word_b4=i%2,profile_words=words,tuning_28=[.8,.2,.25],tuning_34=[.9,.3,.125])
            put(base[5]+0x13f8,"I",ram+0x3100);put(ram+0x3100,"I",ram+0x3200)
            put(ram+0x3200,"10I6f",*words,*setup["mode5"]["tuning_28"],*setup["mode5"]["tuning_34"])
            put(ram+0x30b4,"i",i%2)
        req=Path(td)/"setup.json";out=Path(td)/f"setup-{i}.json";req.write_text(json.dumps(setup))
        subprocess.run([cli,"kart-race-tuning",rel,out,req],check=True);configured=json.loads(out.read_text())
        u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0x5ce8,base[1]+0x200,count=180)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        assert struct.pack(">3f",*configured["tuning"])==bytes(u.mem_read(base[5]+0x36c0,12))
        assert bytes(u.mem_read(base[5]+0x380c,40))==(before if mode!=5 else struct.pack(">10I",*configured["profile8_write"]))
        q["tuning"]=configured["tuning"]
        if mode==5:
            req.write_text(json.dumps(dict(profile=8,alternate=False,profile8_write=configured["profile8_write"],velocity=[0,0,0])))
            prof=Path(td)/f"profile-{i}.json"
            subprocess.run([cli,"kart-active-profile",rel,prof,req],check=True)
            row=json.loads(prof.read_text())
            assert row["acceleration"]==.25 and row["velocity_limit"]==9.25
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-guided-drive",rel,out,req],check=True);got=json.loads(out.read_text())
        for key,v in g.items():put(wk+int(key[5:],16),"f" if key=="work_264" else "i",v)
        for key,off in (("segment",0xbc),("place",0xc0),("lap",0xc4),("player_slot",0x78),("work_74",0x74)):put(wk+off,"i",q[key])
        put(wk+0x80,"f",3);put(wk+0x268,"3f",1,2,3);put(tw+20,"3f",*q["position"])
        put(base[6]+0x24d80,"i",q["segment_count"]);put(base[6]+0x26d84+128,"8f",*(v for pair in q["quad"] for v in pair))
        put(base[6]+0x21440,"ff",1,-4);put(base[6]+0x21450,"f",.5)
        u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0x42c8,base[1]+0x200,count=500)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        for key,v in got["guidance"].items():
            fmt="f" if key=="work_264" else "i"
            assert struct.pack(">"+fmt,v)==bytes(u.mem_read(wk+int(key[5:],16),4))
        for addr,key in ((tw+20,"position"),(wk+0x268,"previous_ground_position")):
            assert struct.pack(">3f",*got[key])==bytes(u.mem_read(addr,12))
        assert got["yaw"]==get(tw+12,"i")[0]
print("Eight state 7 driving callbacks matched through return.")
