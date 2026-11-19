"""Compare kart controller flags, stick history and steering input using original instructions."""
import json,struct,subprocess,sys,tempfile
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
        delta,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t!=1 or not (0x3034<=l<0x30dc or 0x31b4<=l<0x33a4 or 0x5360<=l<0x5498) or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cbd88:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
ram=0x85000000;u.mem_map(ram,65536)
task,work=ram+256,ram+512;put(task+0x38,"I",work)
def reg(i,val):u.reg_write(getattr(R,f"UC_PPC_REG_{i}"),val)
def run(start,end):
    u.emu_start(base[1]+start,base[1]+end,count=1000)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+end
hist=base[6]+0x222ac;cur=base[6]+0x2248c
reg(1,ram+0xf000);u.reg_write(R.UC_PPC_REG_MSR,8192)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for slot in (0,1):
        history=[n*17-420 for n in range(60)];cursor=57
        frames=[dict(buttons=(0,4,2,0x400,0x406,0xffffffff,0x8000)[n%7],
                     work_28c=(9,10,11,-1)[n%4],stick_x=(n*7937)%65536-32768) for n in range(67)]
        for n,f in enumerate(frames):
            f["drive"]=dict(heading=2147483647-n,work_a4=n-1,place=1+n%6,multiplayer=bool(slot),
                            velocity_xz=[0,0] if n%3==0 else [3,4],magnitude_return=None if n%3==0 else 5,
                            acceleration=0.125,brake_power=-0.75,deceleration=-0.0625,speed_cap=10)
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
            if drive["magnitude_return"] is None:run(0x31b4,0x33a4)
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
        assert get(cur+4*slot,"i")==doc["cursor"]
        assert bytes(u.mem_read(hist+240*slot,240))==struct.pack(">60i",*doc["history"])
print("Matched 134 native controller samples, including native heading, input priority and exact local displacement.")
