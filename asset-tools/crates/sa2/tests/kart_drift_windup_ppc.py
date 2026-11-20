"""Compare state 2 with installed PPC, modeling the engine math imports."""
import math,json,struct,subprocess,sys,tempfile
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
        if t!=1 or not (0x21d8<=l<0x2214 or 0x30dc<=l<0x3188 or 0x399c<=l<0x3ec4) or k not in (4,6):continue
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
tw=ram+0x1000;put(task+0x34,"I",tw)
reg(1,ram+0xf000);u.reg_write(R.UC_PPC_REG_MSR,8192)
def fp(i):return struct.unpack(">d",struct.pack(">Q",u.reg_read(getattr(R,f"UC_PPC_REG_FPR{i}"))))[0]
def setfp(v):u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",v))[0])
def hook(m,addr,*args):
    pc=addr-base[1]
    arg=m.reg_read(R.UC_PPC_REG_3)
    read=lambda p:struct.unpack(">3f",u.mem_read(p,12))
    write=lambda p,v:u.mem_write(p,struct.pack(">3f",*v))
    if pc==0x3134:setfp(math.atan2(fp(1),fp(2)))
    elif pc in (0x3a30,0x3af8):
        angle=get(arg+4,"i")%65536
        p=m.reg_read(R.UC_PPC_REG_4);x,y,z=read(p)
        theta=math.radians(angle*360/65536);s,c=math.sin(theta),math.cos(theta)
        write(p,[c*x+s*z,y,-s*x+c*z])
    elif pc==0x3e1c:
        assert arg==q['sustained']['work_fc'] and m.reg_read(R.UC_PPC_REG_5)==364
        m.reg_write(R.UC_PPC_REG_3,q['sustained']['angle_service_return'])
    elif pc in (0x3dd4,0x3a68):setfp(struct.unpack(">f",struct.pack(">f",math.hypot(*read(arg))))[0])
    elif pc==0x3aa8:
        v=read(arg);n=math.hypot(*v)
        if n:write(arg,[x/n for x in v])
    elif pc==0x3b08:write(arg,[a+b for a,b in zip(read(arg),read(m.reg_read(R.UC_PPC_REG_4)))])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x3134,0x3a30,0x3af8,0x3a68,0x3aa8,0x3b08,0x3dd4,0x3e1c):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for side in (-1,1):
        q=dict(work_a4=0,heading=278,work_a8=side*35,work_68=0,position=[3,4,5],
               velocity=[0,0,0],accelerate=False,work_74=0,drift_handling=.75,weight=.5)
        for n in range(12):
            q.update(velocity=[[0,1,0],[0.25,2,-.25],[3,-1,4],[0,0,-4],[4,1,0]][n%5],
                     accelerate=n%2==0,work_74=n%2)
            if n>=5:
                if n==5:q["velocity"]=[4,0,0]
                if n==6:q["velocity"]=[0,0,0]
                if n==9:q.update(work_a4=19,heading=0,work_68=0,velocity=[0,0,-4])
                ctx=dict(stick_x=(0 if n==9 else side*60),speed_threshold=(40 if n==8 else 1),
                         work_fc=123,angle_service_return=33)
                q["sustained"]=ctx
                put(base[6]+0x224b8,"i",0);put(work+0x78,"i",0)
                put(base[6]+0x224bc+28,"h",ctx["stick_x"]);put(work+0xfc,"i",ctx["work_fc"])
                put(base[6]+0x2145c,"f",ctx["speed_threshold"])
            req=Path(td)/"q.json";out=Path(td)/f"{side}-{n}.json";req.write_text(json.dumps(q))
            subprocess.run([cli,"kart-drift-step",rel,out,req],check=True);doc=json.loads(out.read_text())
            for off,key in [(0x68,"work_68"),(0xa4,"work_a4"),(0xa8,"work_a8"),(0x110,"accelerate"),(0x74,"work_74")]:
                put(work+off,"i",q[key])
            put(work+0x30,"i",2 if n<5 else 3);put(tw+12,"i",q["heading"])
            for addr,key in [(work+0x7c,"velocity"),(tw+0x14,"position")]:u.mem_write(addr,struct.pack(">3f",*q[key]))
            put(base[6]+0x21458,"f",.75);put(base[6]+0x21450,"f",.5)
            reg(3,task);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
            run(0x3b30 if n<5 else 0x3c74,0x200)
            if n>=5:assert doc['work_fc']==get(work+0xfc,'i')
            for addr,key in [(work+0x30,"vehicle_state"),(work+0x68,"work_68"),(work+0xa4,"work_a4"),(tw+12,"heading")]:
                assert get(addr,"i")==doc[key]
            for addr,key in [(work+0xac,"local_velocity"),(tw+0x14,"position")]:
                assert bytes(u.mem_read(addr,12))==struct.pack(">3f",*doc[key]),(side,n,key)
            q.update({key:doc[key] for key in ("work_a4","work_68","position","heading")})
reg(30,work);put(work+0x10c,"I",0x80000001)
u.mem_write(work+0x7c,struct.pack(">3f",-20,7,0.125))
reg(29,task);pose=struct.pack(">3f",-0.0,11.25,-39.5)
u.mem_write(tw+0x14,pose);u.mem_write(work+0x14,bytes(12))
angles=struct.pack(">3i",-12345,65537,23456)
u.mem_write(tw+8,angles);u.mem_write(work+8,bytes(12))
run(0x2170,0x2214)
assert bytes(u.mem_read(work+8,12))==angles and bytes(u.mem_read(base[6]+0x21468+8,12))==angles
assert bytes(u.mem_read(work+0x14,12))==pose
assert bytes(u.mem_read(base[6]+0x21468+0x14,12))==pose
assert bytes(u.mem_read(base[6]+0x21468+0x7c,12))==struct.pack(">3f",-20,7,0.125)
assert get(base[6]+0x21468+0x10c,"I")==0x80000001 and get(work+0x10c,"I")==0
for at,mask in [(0x24c0,4),(0x2520,4),(0x2570,2)]:
    for word in [0,0x80000001]:
        put(work+0x10c,"I",word);run(at,at+12)
        assert get(work+0x10c,"I")==word|mask
print("Matched 10 windup and 14 sustained-drift updates with modeled engine math.")
