"""Compare native helper 0x5524 and its nested yaw update to Rust."""
import hashlib,json,struct,subprocess,sys,tempfile
from gsne8p import steam_rel
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
cli,rel,private=map(Path,sys.argv[1:])
d=rel.read_bytes();assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,4:0x82000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for s,b in base.items():
    row=w(16)+8*s;off=w(row)&~1;n=w(row+4)
    u.mem_map(b,(n+4095)&~4095);u.mem_write(b,d[off:off+n])
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    while True:
        inc,k,s,a=steam_rel(struct.unpack_from(">HBBI", d, at), mod);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=inc
        if target==1 and mod==80 and (0x30dc<=loc<0x3188 or 0x5524<=loc<0x560c) and k in (4,6):
            ptr=base[s]+a;u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x84000000;u.mem_map(ram,65536);task=ram+0x100;wk=ram+0x400;tw=ram+0x800;vec=ram+0xc00
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def read(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))
def fp(n):return struct.unpack(">d",struct.pack(">Q",u.reg_read(getattr(R,f"UC_PPC_REG_FPR{n}"))))[0]
def freturn(v):u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",v))[0])
def hook(u,addr,size,_):
    off=addr-base[1]
    if off==0x5588:assert read(u.reg_read(R.UC_PPC_REG_3),"3f")==tuple(vector);freturn(mag)
    elif off in (0x3134,0x5598):calls["angle_args"]=[fp(1),fp(2)];freturn(angle)
    elif off==0x55c0:
        vals=[u.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5)]
        calls["turn_args"]=[struct.unpack(">i",struct.pack(">I",v))[0] for v in vals]
        u.reg_write(R.UC_PPC_REG_3,turn&0xffffffff)
    else:return
    u.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,hook)
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for i,(flag,vector,angle) in enumerate([(0,[0,0,0],0.),(0,[1.5,-2.,3.],-1.234),(1,[0.,4.,-0.25],None),(1,[-0.5,2.,0.],-2.1),(7,[0.,0.,0.5],1.23),(1,[3.,0.,-6.],0.)]):
        mag=8.25;turn=-582;original=-4095;calls={"angle_args":None,"turn_args":None}
        u.mem_write(wk,b"\xcd"*0x300);put(task+0x34,"II",tw,wk);put(tw+12,"I",123456);put(wk+0x44,"I",flag);put(wk+0x274,"i",original);put(vec,"3f",*vector)
        u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0x8000);u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_4,vec)
        stop=base[1]+0x200;u.reg_write(R.UC_PPC_REG_LR,stop)
        u.emu_start(base[1]+0x5524,stop,count=500);assert u.reg_read(R.UC_PPC_REG_PC)==stop
        inp=dict(vector=vector,work_44=flag,work_274=original,magnitude_return=mag,angle_return=angle,turn_return=turn)
        req=Path(tmp)/"req.json";out=Path(tmp)/f"{i}.json";req.write_text(json.dumps(inp))
        subprocess.run([cli,"kart-ramp-response",rel,out,req],check=True)
        got=json.loads(out.read_text())
        assert {k:got[k] for k in calls}==calls
        for key,off,fmt in [("work_30",0x30,"I"),("work_68",0x68,"I"),("work_7c",0x7c,"3f"),("work_264",0x264,"f"),("work_274",0x274,"i")]:
            v=got[key]
            expected=(struct.pack(">"+fmt,*(v if isinstance(v,list) else [v])) if v is not None else struct.pack(">i",original) if key=="work_274" else b"\xcd"*struct.calcsize(">"+fmt))
            assert bytes(u.mem_read(wk+off,len(expected)))==expected,(i,key)
        assert read(tw+12,"I")[0]==(123456 if got["task_yaw"] is None else got["task_yaw"])
print("Matched six native vector responses, including both 0.5 thresholds and untouched fields.")
