"""Execute original kart result atlas setup, display and sprite wrapper offline."""
from pathlib import Path
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,atlas,tmp=map(Path,sys.argv[1:])
data=rel.read_bytes()
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x82000000,5:0x83000000,6:0x84000000}
ram=0x85000000
u.mem_map(ram,0x10000)
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
        if t!=1 or k not in (4,6) or not 0xd2dc<=l<0xd954:continue
        if mod==80:p=base[s]+a
        elif a==0x801cbd88:p=ram
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(addr,fmt,*v):u.mem_write(addr,struct.pack(">"+fmt,*v))
def get(addr,fmt):return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
def reg(n):return u.reg_read(getattr(R,"UC_PPC_REG_"+str(n)))
def hook(machine,addr,*args):
    pc=addr-base[1]
    if pc==0xd334:
        p=reg(3);idx=reg(4)
        tab,anim=get(p+24,"2I")
        assert tab==base[5]+0x2ee0b0
        sx,sy,cx,cy,*tail=get(anim+20*idx,"10h")
        depth=struct.unpack(">d",struct.pack(">Q",reg("FPR1")))[0]
        draws.append(dict(frame=idx,xy=list(get(p,"2f")),scale=list(get(p+12,"2f")),
                          angle=get(p+20,"i")[0],priority=depth,draw_flags=reg(5),size=[sx,sy],
                          center=[cx,cy],uv=tail[:4],texture_id=tail[4],attribute=tail[5]))
    else:viewport.append(pc)
    u.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0xd334,0xd788,0xd78c,0xd930):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
u.reg_write(R.UC_PPC_REG_1,ram+0xf000);u.reg_write(R.UC_PPC_REG_MSR,8192)
u.emu_start(base[1]+0xd348,base[1]+0xd49c)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(mode,sel,p0,p1,flag) in enumerate([(0,1,1,2,True),(0,1,2,1,False),
        (0,0,2,2,True),(1,0,2,1,False),(1,3,1,2,True),(2,0,1,2,True)]):
        req=Path(td)/"req.json";out=Path(td)/f"{i}.json"
        req.write_text(json.dumps(dict(mode=mode,selection=sel,places=[p0,p1],viewport_flag=flag)))
        subprocess.run([cli,"kart-finish-display",rel,out,atlas,req],check=True)
        result=json.loads(out.read_text());draws=[];viewport=[]
        put(base[6]+0x21428,"II",ram+0x400,ram+0x500)
        for off,p in [(0x400,p0),(0x500,p1)]:
            put(ram+off+0x38,"I",ram+off+0x1000);put(ram+off+0x10c0,"i",p)
        put(ram+0x1e,"B",flag);put(ram+0x234,"I",ram+0x300)
        put(ram+0x300,"b",mode);put(ram+0x303,"B",sel)
        u.reg_write(R.UC_PPC_REG_3,ram+0x200);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0xd734,base[1]+0x200,count=500)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        assert result["draws"]==draws
        assert viewport==([0xd788,0xd78c,0xd930] if result["viewport_setup"] else [])
print("Matched all six original result display submissions and initialized atlas frames.")
