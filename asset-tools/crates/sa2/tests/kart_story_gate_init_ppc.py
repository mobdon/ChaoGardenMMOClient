"""Compare installed story gate SET poses and initialized task model words with CartD."""
from pathlib import Path
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,exe,tmp=map(Path,sys.argv[1:])
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
        if t==5 and mod==80 and k==1 and s in base:u.mem_write(base[5]+l,struct.pack(">I",base[s]+a))
        if t==1 and k in (4,6) and (0xb600<=l<0xb748):
            if mod!=80 and a not in (0x80175378,0x803adad0):continue
            p=base[s]+a if mod==80 else (0x85004000 if a==0x80175378 else 0x85004100)
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
ring,tw,col=ram+0x100,ram+0x200,ram+0x400
put(ring+0x34,"I",tw);put(tw+0x2c,"I",col)

def hook(m,addr,*_):
    a,b,c,e=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5,6)]
    if addr==base[1]+0xb670:assert (a,b-base[5],c,e)==(ring,r["collisionDescriptor"],r["collisionCount"],r["collisionList"])
    elif addr==base[1]+0xb67c:
        assert (a,b)==(1,56)
        m.reg_write(R.UC_PPC_REG_3,wk)
    else:assert (a,b)==(r["registrationFlags"],ring)
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0xb670,0xb67c,0xb744):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
count=0
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for path in sorted(rel.parent.glob("setCart*.bin")):
        out=Path(td)/(path.stem+".json")
        subprocess.run([cli,"kart-story-gates",rel,out,exe,path],check=True)
        r=json.loads(out.read_text())
        raw=[(i,v) for i,v in enumerate(struct.iter_unpack(">H3h6f",path.read_bytes()[32:])) if v[0]&4095==4]
        assert len(raw)==len(r["gates"])
        for (i,v),g in zip(raw,r["gates"]):
            assert i==g["placement"] and int(v[8])*60==g["timerTicksOnFirstPass"]
            u.mem_write(tw,b"\0"*64);put(tw+8,"3i6f",*v[1:])
            u.reg_write(R.UC_PPC_REG_30,ring);u.reg_write(R.UC_PPC_REG_31,tw)
            u.emu_start(base[1]+0xb5f8,base[1]+0xb7f0,count=300)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xb7f0
            assert (get(tw,"B")[0],get(tw+3,"B")[0],get(tw+6,"H")[0])==(g["mode"],g["initialByte3"],g["latch"])
            assert list(get(ring+20,"II"))==[base[1]+r["display"],base[1]+r["cleanup"]]
            words=list(get(wk,"14I"))
            for j in (1,11,12):
                if words[j]:words[j]-=base[5]
            assert words==g["nodeWords"]
            count+=1
assert count==8
print("All eight story gate instances matched native initialization across four separate layouts.")