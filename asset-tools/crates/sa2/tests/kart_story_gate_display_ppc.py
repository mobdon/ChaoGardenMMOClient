"""Compare native GATE2 draw commands, shared material mutation and cleanup."""
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
        if t==5 and mod==80 and k==1 and s in base:u.mem_write(base[5]+l,struct.pack(">I",base[s]+a))
        if t==1 and k in (4,6) and (0xb834<=l<0xb940 or 0x17e8<=l<0x1854):
            if mod!=80 and a not in (0x80174f98,0x803ad6d0):continue
            p=base[s]+a if mod==80 else (0x85004000 if a==0x80174f98 else 0x85004100)
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
ring,tw,col=ram+0x100,ram+0x200,ram+0x400
put(ring+0x34,"I",tw);put(tw+0x2c,"I",col)

from PIL import Image
import io,hashlib

def hook(m,addr,*_):
    pc=addr-base[1];a,b,c=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in (3,4,5)]
    if pc==0x1818:assert (a,b)==(tw+20,ram+0x4300)
    elif pc==0x1820:m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",metric))[0])
    elif pc in (0xb8b0,0xb8fc):calls.append(pc)
    elif pc==0xb8bc:assert a==0;calls.append(list(get(b,"3f")))
    elif pc in (0xb8c8,0xb8d4,0xb8e0):
        assert a==0
        calls.append([{0xb8c8:"z",0xb8d4:"x",0xb8e0:"y"}[pc],struct.unpack(">i",struct.pack(">I",b))[0]])
    elif pc==0xb92c:calls.append([a,b])
    else:calls.append(a-base[5])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x1818,0x1820,0xb8b0,0xb8bc,0xb8c8,0xb8d4,0xb8e0,0xb8ec,0xb8f8,0xb8fc,0xb92c):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
put(ram+0x4000,"I",ram+0x4300)
def run(start):
    u.reg_write(R.UC_PPC_REG_3,ring);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
    u.emu_start(base[1]+start,base[1]+0x200,count=3000)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
with tempfile.TemporaryDirectory(dir=tmp) as td:
    def rust(i,q):
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-story-gate-display",rel,out,req],check=True);return json.loads(out.read_text())
    at=0x489e;ptr=base[5]+at;put(ptr,"H",0xc049)
    for i,(flag,metric) in enumerate([(0,0),(1,0),(2,0),(0,4000000)]):
        before=get(ptr,"H")[0];q=dict(operation="display",position=[1,-2,3],angles=[-10,20,30],byte_3=flag,current_view_metric=metric,material_word=before)
        put(tw+3,"B",flag);put(tw+8,"3i3f",*q["angles"],*q["position"]);calls=[];run(0xb834)
        r=rust(i,q);draw=r["draw"]
        assert calls==([] if draw is None else [0xb8b0,draw["position"],*draw["rotations"],draw["textureList"],draw["model"],0xb8fc])
        assert r["writes"]==([dict(offset=at,before=before,after=get(ptr,"H")[0])] if flag<2 and draw else [])
    for i,work in enumerate([0,wk],10):
        put(ring+56,"I",work);calls=[];run(0xb914)
        call=rust(i,dict(operation="cleanup",task=ring,work_38=work))["call"]
        assert calls==([] if call is None else [call["args"]])
        if call:assert call["address"]==0x80022f84
print("Four gate displays and two cleanup paths matched native execution.")