"""Compare the kart story target spawn and chained ticks with original PowerPC."""
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
        if t==1 and mod==80 and k in (4,6) and 0xd968<=l<0xdbe0:
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))
def intercept(m,a,*_):
    if a==base[1]+0xd99c:m.reg_write(R.UC_PPC_REG_3,child)
    else:cues.append(m.reg_read(R.UC_PPC_REG_3))
    m.reg_write(R.UC_PPC_REG_PC,a+4)
for off in (0xd99c,0xdb78,0xdb84):
    u.hook_add(UC_HOOK_CODE,intercept,begin=base[1]+off,end=base[1]+off)
put(parent+0x34,"II",tw,wk);put(child+0x34,"I",cw);put(wk+0xbc,"I",5)
quad=[[1,2],[3,4],[-37.5,82],[7,8]]
put(base[6]+0x26d84+32*5,"8f",*(v for pair in quad for v in pair));put(tw+0x18,"f",-4.25)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    case=0
    for mode in (0,1,2):
        put(base[6]+0x1a0,"i",mode);put(base[6]+0x3ce2c,"i",0)
        for j,(active,speed) in enumerate([(1,0),(0,123),(1,-2500),(1,3907),(1,7),(1,8),(1,123)]):
            cues=[]
            if j==0:
                q=dict(operation="spawn",mode=mode,latch=0,parent_task_y=-4.25,segment_xz=quad)
                entry,arg=0xd968,parent
            else:
                q=dict(operation="tick",mode=mode,state=state,active_word=active,parent_work_x=12.5,parent_work_7c=speed)
                entry,arg=0xda80,child
                put(wk+0x14,"f",12.5);put(wk+0x7c,"f",speed);put(base[6]+0x36d94,"i",active)
            put(child+0x14,"II",0,0)
            req=Path(td)/"q.json";out=Path(td)/f"{case}.json";case+=1
            req.write_text(json.dumps(q));subprocess.run([cli,"kart-story-target",rel,out,req],check=True)
            doc=json.loads(out.read_text())
            u.reg_write(R.UC_PPC_REG_3,arg);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
            u.emu_start(base[1]+entry,base[1]+0x200,count=200)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
            state=doc["state"]
            assert bytes(u.mem_read(cw+0x14,16))==struct.pack(">4f",*state["position"],state["lead"])
            assert state["latch"]==get(base[6]+0x3ce2c,"i")[0]
            assert cues==([] if doc["cue_id"] is None else [doc["cue_id"]])
            assert get(child+0x14,"II")==((base[1]+0xdbe0,base[1]+0xdd00) if doc["installs_callbacks"] else (0,0))
print("Matched three target spawns and 18 chained ticks, including the exact catch threshold.")
