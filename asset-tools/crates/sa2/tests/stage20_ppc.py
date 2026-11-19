"""Compare stage 20 entry frames with the installed registered callback."""
import os,json,struct,tempfile,sys,subprocess
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_BIG_ENDIAN,UC_MODE_PPC32,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);path=game/"resource/gd_PC/stg20D.rel";d=path.read_bytes()
w=lambda off:struct.unpack_from(">I",d,off)[0]
u=Uc(UC_ARCH_PPC,UC_MODE_BIG_ENDIAN|UC_MODE_PPC32);base={1:0x81200000,4:0x81800000,5:0x81a00000}
for s,b in base.items():
    row=w(16)+8*s;at=w(row)&~1;size=w(row+4);u.mem_map(b,(size+4095)&~4095);u.mem_write(b,d[at:at+size])
def put(addr,fmt,*vals):u.mem_write(addr,struct.pack(">"+fmt,*vals))
def get(addr,fmt):return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
engines={}
for mod,i in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        n,k,s,v=struct.unpack_from(">HBBI",d,i);i+=8
        if k==203:break
        if k==202:t=s;off=0;continue
        off+=n
        if t not in base:continue
        if mod==19 and s in base:v+=base[s]
        elif mod!=0:continue
        if mod==0 and t==1 and k==10:engines[off]=v
        if k==1:put(base[t]+off,"I",v)
        if k in (4,6):put(base[t]+off,"H",(v if k==4 else (v+32768)>>16)&65535)
        if t==1 and off==0x1c2c and k==10:service=v
# Resolve the registered entry pointer from the stage descriptor.
entry=get(base[5]+88932+20,"I")[0]
u.mem_map(0x100000,4096);u.mem_map(0x801e7000,4096);u.reg_write(R.UC_PPC_REG_MSR,8192)
task=0x100100;work=0x100200;target=0x100300;stop=0x100400
def watch(cpu,pc,*_):
    p=cpu.reg_read(R.UC_PPC_REG_4);pos,f,g,n,a,b,h,j,m=get(p,"IffIBB2xffI")
    assert pos==work+20
    calls.append(dict(engine=service,slot=cpu.reg_read(R.UC_PPC_REG_3),positionAtCall=list(get(pos,"fff")),
                      float4=f,float8=g,word12=n,byte16=a,byte17=b,float20=h,float24=j,word28=m))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
u.hook_add(UC_HOOK_CODE,watch,begin=base[1]+0x1c2c,end=base[1]+0x1c2c)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for mode in (-128,-1,0,1,2,127):
        observed=[None,[4.,5.,6.],None,[-2.,-10.,30.]];position=[3.,7.,-9.]
        put(task+0x34,"I",work);put(work,"b",mode);put(work+20,"fff",*position)
        frames=[]
        for value in observed:
            calls=[];put(0x801e736c,"I",target if value else 0)
            if value:put(target+20,"fff",*value)
            u.reg_write(R.UC_PPC_REG_1,0x100f00);u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_LR,stop)
            u.emu_start(entry,stop,count=100);assert u.reg_read(R.UC_PPC_REG_PC)==stop and len(calls)<=1
            frames.append(dict(mode=get(work,"b")[0],position=list(get(work+20,"fff")),call=next(iter(calls),None)))
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=mode,position=position,targets=observed)))
        out=Path(tmp)/f"{mode}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        assert json.loads(out.read_text())==dict(module=19,entry=entry-base[1],frames=frames)
print("24 consecutive stage 20 task frame observations matched")

startup=get(base[5]+88932+12,"I")[0]
def startup_call(cpu,pc,*_):
    site=pc-base[1];r3=cpu.reg_read(R.UC_PPC_REG_3);r4=cpu.reg_read(R.UC_PPC_REG_4)
    if site==0x1744:
        assert r3==target;ops.append(dict(site=site,slot=1,workByte=2,value=cpu.reg_read(R.UC_PPC_REG_0)))
        return
    if site in (0x1750,0x175c,0x1768):
        displacement=get(pc,"I")[0]&0x03fffffc
        if displacement&0x02000000:displacement-=0x04000000
        ops.append(dict(site=site,local=site+displacement,args=[] if site==0x1750 else [r3,r4]))
    else:
        ops.append(dict(site=site,engine=engines[site],args=[r3]))
        if site in (0x16e8,0x1704):cpu.reg_write(R.UC_PPC_REG_3,next(answers)&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x16e8,0x16f8,0x1704,0x1714,0x1730,0x1744,0x174c,0x1750,0x175c,0x1768):
    u.hook_add(UC_HOOK_CODE,startup_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,queries in enumerate(([7],[6,6],[6,7],[0,6],[0,0],[-1,-1])):
        for present in (False,True):
            ops=[];answers=iter(queries);put(0x801e738c,"I",task if present else 0);put(0x801e736c,"I",target);put(target+2,"B",0xdd)
            u.reg_write(R.UC_PPC_REG_1,0x100f00);u.emu_start(startup,base[1]+0x176c,count=100)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x176c and get(target+2,"B")[0]==(3 if present else 0xdd)
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],startup=dict(character_queries=queries,slot_one_present=present))))
            out=Path(tmp)/f"{idx}-{present}.json"
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            assert json.loads(out.read_text())["startup"]==dict(entry=startup-base[1],continueAt=0x176c,operations=ops)
print("12 stage 20 opponent startup paths matched")
