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
