"""Compare vehicle gate state, stick history and callback order with CartD."""
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
        if t==1 and mod==80 and k in (4,6) and (0x2214<=l<0x2284 or 0x5360<=l<0x53dc):
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

def observe(m,a,*_):
    pc=a-base[1]
    if pc in (0x2284,0x287c):
        m.emu_stop()
    else:
        assert m.reg_read(R.UC_PPC_REG_3)==parent
        calls.append(pc)
        m.reg_write(R.UC_PPC_REG_PC,m.reg_read(R.UC_PPC_REG_LR))
for site in (0x2284,0x287c,0x2ca4,0x2da8,0x4b50,0x3030,0x302c):
    u.hook_add(UC_HOOK_CODE,observe,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for slot in (-1,0,1):
        for first,second in ((0,0),(0,-1),(1,0),(-2,1)):
            calls=[];hist=list(range(60));cursor=59 if slot==1 else 7
            lane=max(0,slot);g=base[6];ring=g+0x222ac+lane*240;idx=g+0x2248c+lane*4
            put(g+0x224b8,"I",1);put(g+0x224bc+0x68+lane*0x34+0x1c,"h",-321)
            put(ring,"60i",*hist);put(idx,"I",cursor)
            put(g+0x22494,"i",first);put(base[5]+0x2ec208,"i",second)
            put(wk+0x70,"i",slot);put(wk+0x30,"i",4)
            for r,v in ((R.UC_PPC_REG_29,parent),(R.UC_PPC_REG_30,wk)):u.reg_write(r,v)
            u.emu_start(base[1]+0x2214,base[1]+0x6000,count=100)
            q=dict(work_70=slot,state=4,global_22494=first,global_2ec208=second,
                   controller=dict(stick_x=-321,history=hist,cursor=cursor))
            req=Path(td)/"in.json";out=Path(td)/f"{slot}_{first}_{second}.json"
            req.write_text(json.dumps(q));subprocess.run([cli,"kart-update-gate",rel,out,req],check=True)
            got=json.loads(out.read_text())
            assert got["resume_offset"]==u.reg_read(R.UC_PPC_REG_PC)-base[1]
            assert got["controller"]["history"]==list(get(ring,"60i"))
            assert got["controller"]["cursor"]==get(idx,"I")[0]
            assert got["state"]==get(wk+0x30,"i")[0] and got["callbacks"]==calls
print("12 vehicle dispatch branches matched original CartD.")
