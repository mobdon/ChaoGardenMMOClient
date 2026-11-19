"""Compare each timeExtExec update with the native instructions, without launching SA2."""
from pathlib import Path
import json, struct, subprocess, sys, tempfile
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,private=map(Path,sys.argv[1:])
data=rel.read_bytes()
w=lambda at:struct.unpack_from(">I",data,at)[0]
base={1:0x81000000,4:0x82000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    at=w(16)+sec*8; off=w(at)&~1; n=w(at+4)
    u.mem_map(addr,(n+4095)&~4095);u.mem_write(addr,data[off:off+n])
for mod,at in struct.iter_unpack(">II",data[w(40):w(40)+w(44)]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",data,at);at+=8
        if k==203:break
        if k==202:t=s;loc=0;continue
        loc+=delta
        if t==1 and 0xc858<=loc<0xca10 and mod==80 and k in (4,6):
            ptr=base[s]+a
            u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x83000000;u.mem_map(ram,65536);task=ram+256;work=ram+512
u.mem_write(task+52,struct.pack(">I",work))
def get(at,fmt):
    return struct.unpack(">"+fmt,u.mem_read(at,struct.calcsize(">"+fmt)))
deletions=[]
def drop(m,pc,*args):
    assert m.reg_read(R.UC_PPC_REG_3)==task
    deletions.append(frame);m.reg_write(R.UC_PPC_REG_PC,pc+4)
u.hook_add(UC_HOOK_CODE,drop,begin=base[1]+0xc9c0,end=base[1]+0xc9c0)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
with tempfile.TemporaryDirectory(dir=private) as td:
    out=Path(td)/"timeline.json";subprocess.run([cli,"kart-time-extension",rel,out],check=True)
    frames=json.loads(out.read_text())["frames"]
    for frame,r in enumerate(frames,1):
        stop=base[1]+0x200
        u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_LR,stop)
        u.emu_start(base[1]+0xc858,stop,count=200)
        assert u.reg_read(R.UC_PPC_REG_PC)==stop
        assert r==dict(delete=bool(deletions),state=dict(mode=get(work,"B")[0],phase=get(work+3,"B")[0],
                   timer=get(work+6,"H")[0],position=list(get(work+20,"2f")),display=get(task+20,"I")[0]-base[1])),frame
    assert deletions==[len(frames)]
print(f"Matched all {len(frames)} native time extension updates including deletion.")
