"""Compare kart controller flags, stick history and steering input using original instructions."""
import math,json,struct,subprocess,sys,tempfile
from gsne8p import steam_rel
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
d=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,5:0x86000000,4:0x82000000,6:0x83000000};engine=0x84000000
for s,p in base.items():
    row=struct.unpack_from(">I",d,16)[0]+s*8
    off,size=struct.unpack_from(">II",d,row);off &= ~1
    u.mem_map(p,(size+4095)&~4095)
    if off:u.mem_write(p,d[off:off+size])
u.mem_map(engine,4096)
off,n=struct.unpack_from(">II",d,40)
for mod,at in struct.iter_unpack(">II",d[off:off+n]):
    while True:
        delta,k,s,a=steam_rel(struct.unpack_from(">HBBI", d, at), mod);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t!=1 or not (0x3034<=l<0x30dc or 0x31b4<=l<0x368c or 0x52b0<=l<0x5308 or 0x5360<=l<0x5498) or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cc168:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
ram=0x85000000;u.mem_map(ram,65536)
task,work=ram+256,ram+512;put(task+0x38,"I",work)
def reg(i,val):u.reg_write(getattr(R,f"UC_PPC_REG_{i}"),val)
def run(start,end):
    u.emu_start(base[1]+start,base[1]+end,count=4000)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+end
hist=base[6]+0x222ac;cur=base[6]+0x2248c
reg(1,ram+0xf000);u.reg_write(R.UC_PPC_REG_MSR,8192)
from kart_matrix_imports import install_inverse_rotation
install_inverse_rotation(u,base[1])
def vec_hook(m,addr,*args):
    pc=addr-base[1]
    a,b,dest=[m.reg_read(getattr(R,f"UC_PPC_REG_{n}")) for n in (3,4,5)]
    read=lambda p:struct.unpack(">3f",u.mem_read(p,12))
    if pc==0x352c:
        assert bytes(u.mem_read(b,12))==struct.pack(">3f",*dr["history_offset"])
        if "pitch_roll" in di:return
        vals=di["local_offset"]
    else:
        left,right=read(a),read(b)
        vals=[x+y if pc==0x34b8 else x-y for x,y in zip(left,right)]
    u.mem_write(dest,struct.pack(">3f",*vals))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for addr in (0x34b8,0x351c,0x352c):
    u.hook_add(UC_HOOK_CODE,vec_hook,begin=base[1]+addr,end=base[1]+addr)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for slot in (0,1):
        history=[n*17-420 for n in range(60)];cursor=57
        frames=[dict(buttons=(0,4,2,0x400,0x406,0xffffffff,0x8000)[n%7],
                     work_28c=(9,10,11,-1)[n%4],stick_x=(n*7937)%65536-32768) for n in range(67)]
        for n,f in enumerate(frames):
            f["drive"]=dict(heading=2147483647-n,work_a4=n-1,place=1+n%6,multiplayer=bool(slot),
                            velocity_xz=[0,0] if n%3==0 else [3,4],magnitude_return=None if n%3==0 else 5,
                            acceleration=0.125,brake_power=-0.75,deceleration=-0.0625,speed_cap=10)
            if n%4==2:f["drive"]["magnitude_return"]=None
            f["drive"]["position_step"]=dict(position=[3.125,4.5,-9],velocity_y=(-1,0,4.5)[n%3],
                weight=0.375,work_74=n%2,transformed_vector=[-1.25,0.75,2] if n%4==1 else None)
            f["drive"]["drift"]=dict(positions=[[n+i*.25,3-i*2,n-3*i] for i in range(30)],
                local_offset=[[-3,-2.5,1,2.5,3][n%5],0,-1.5],speed_threshold=(4,5,6)[n%3 if n%4 else 0],
                vehicle_state=1,work_10c=4,work_a8=99)
            if n%3==1:
                di=f["drive"]["drift"];del di["local_offset"];di["pitch_roll"]=[n*987,-n*888]
            if n in (4,11,25):
                f["stick_x"]=(20,-20,21)[(4,11,25).index(n)]
                f["drive"]["drift"]["speed_threshold"]=4
        q=dict(history=history,cursor=cursor,frames=frames)
        req=Path(td)/"q.json";out=Path(td)/f"{slot}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-drive-input",rel,out,req],check=True);doc=json.loads(out.read_text())
        put(work+0x78,"i",slot);put(cur+4*slot,"i",cursor)
        u.mem_write(hist+240*slot,struct.pack(">60i",*history))
        for n,f in enumerate(frames):
            bank=n%2;row=base[6]+0x224bc+bank*104+slot*52
            put(base[6]+0x224b8,"i",bank);put(row+8,"I",f["buttons"]);put(row+28,"h",f["stick_x"])
            put(work+0x28c,"i",f["work_28c"])
            for pc,arg in [(0x5360,slot),(0x3034,task),(0x5420,slot)]:
                reg(3,arg);reg(4,10);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
                run(pc,0x200)
            total=u.reg_read(R.UC_PPC_REG_3)
            got=doc["frames"][n]
            assert got["steering_sum"]==struct.unpack(">i",struct.pack(">I",total))[0]
            assert (got["work_110"],got["work_114"])==(get(work+0x110,"I"),get(work+0x114,"I"))
            drive=f["drive"];motion=got["motion"]
            tw=ram+0x1000;reg(30,work);reg(31,tw)
            put(engine+0x1e,"B",slot);put(work+0xc0,"i",drive["place"])
            put(work+0xa4,"i",drive["work_a4"]);put(tw+12,"i",drive["heading"])
            put(work+0x68,"i",99)
            put(work+0x7c,"f",drive["velocity_xz"][0]);put(work+0x84,"f",drive["velocity_xz"][1])
            for j,key in enumerate(["acceleration","brake_power","deceleration","speed_cap"]):
                put(base[6]+0x21440+4*j,"f",drive[key])
            if drive["velocity_xz"]==[0,0]:run(0x31b4,0x33a4)
            else:
                run(0x31b4,0x32a8)
                assert struct.unpack(">3f",u.mem_read(u.reg_read(R.UC_PPC_REG_3),12))==(3,0,4)
                u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",5))[0])
                run(0x32ac,0x33a4)
            scaled=struct.unpack(">d",struct.pack(">Q",u.reg_read(R.UC_PPC_REG_FPR29)))[0]
            assert struct.pack(">f",got["steering_input"])==struct.pack(">f",scaled)
            assert get(tw+12,"i")==get(base[5]+0x3bc4,"i")==motion["heading"]
            assert get(work+0xa4,"i")==motion["work_a4"] and get(work+0x68,"i")==motion["work_68"]
            assert bytes(u.mem_read(u.reg_read(R.UC_PPC_REG_4),12))==struct.pack(">3f",*motion["local_displacement"])
            # Inspect the local vector; provide a mathematical Y rotation or an explicit
            # observation at the matrix boundary, and emulate the imported vector add.
            pp=drive["position_step"];res=motion["position_step"];buf=u.reg_read(R.UC_PPC_REG_4)
            observed=pp["transformed_vector"]
            if observed is None:
                theta=math.radians((get(tw+12,"i")%65536)*360/65536)
                v=get(buf+8,"f");world=[math.sin(theta)*v,0,math.cos(theta)*v]
            else:world=observed
            world=struct.unpack(">3f",struct.pack(">3f",*world))
            assert struct.pack(">3f",*res["world_displacement"])==struct.pack(">3f",*world)
            assert res["transform_source"]==("observed" if observed else "mathematical_projection")
            assert motion["magnitude_source"]==("native_zero" if drive["velocity_xz"]==[0,0] else
                "observed" if drive["magnitude_return"] is not None else "mathematical_projection")
            u.mem_write(buf,struct.pack(">3f",*world))
            run(0x33a8,0x33b4)
            assert u.reg_read(R.UC_PPC_REG_3)==tw+0x14 and u.reg_read(R.UC_PPC_REG_5)==tw+0x14
            assert u.reg_read(R.UC_PPC_REG_4)==buf
            u.mem_write(tw+0x14,struct.pack(">3f",*(a+b for a,b in zip(pp["position"],world))))
            put(work+0x80,"f",pp["velocity_y"]);put(work+0x74,"i",pp["work_74"])
            put(base[6]+0x21450,"f",pp["weight"])
            run(0x33b8,0x3428)
            assert bytes(u.mem_read(tw+0x14,12))==struct.pack(">3f",*res["position"])
            di=drive["drift"];dr=motion["drift"]
            poscursor=(3+n)%60;put(base[6]+0x222a4+4*slot,"i",poscursor)
            for i,v in enumerate(di["positions"]):
                u.mem_write(base[6]+0x21764+720*slot+12*((poscursor-i-1)%60),struct.pack(">3f",*v))
            put(work+0x70,"i",slot);put(work+0x30,"i",di["vehicle_state"])
            put(work+0x10c,"I",di["work_10c"]);put(work+0xa8,"i",di["work_a8"])
            put(base[6]+0x2145c,"f",di["speed_threshold"])
            if "pitch_roll" in di:
                put(tw+8,"i",di["pitch_roll"][0]);put(tw+16,"i",di["pitch_roll"][1])
            run(0x346c,0x368c)
            for key,off in [("work_f0",0xf0),("work_f8",0xf8),("vehicle_state",0x30),
                            ("work_10c",0x10c),("work_a8",0xa8),("work_a4",0xa4)]:
                assert dr[key]==get(work+off,"i"),(key,n,slot,dr)
        assert get(cur+4*slot,"i")==doc["cursor"]
        assert bytes(u.mem_read(hist+240*slot,240))==struct.pack(">60i",*doc["history"])
print("Matched 134 native controller samples, including heading, local movement and drift-entry state writes.")
