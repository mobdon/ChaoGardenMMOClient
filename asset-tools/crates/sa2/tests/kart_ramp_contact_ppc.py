"""Compare active jump-panel task branches with the installed PPC instructions."""
import hashlib,json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,private=map(Path,sys.argv[1:])
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
word=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,4:0x82000000,5:0x83000000,6:0x84000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    row=word(16)+8*sec;off=word(row)&~1;size=word(row+4)
    u.mem_map(addr,(size+4095)&~4095)
    if off:u.mem_write(addr,d[off:off+size])
for mod,at in struct.iter_unpack(">II",d[word(40):word(40)+word(44)]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=delta
        if target==1 and mod==80 and (0xbdf4<=loc<0xbf30 or 0x1714<=loc<0x17e8) and k in (4,6):
            ptr=base[s]+a
            if loc==0x1716:registry=ptr
            u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,0x10000);work=ram;coll=ram+256
get=lambda n:u.reg_read(getattr(R,f"UC_PPC_REG_{n}"))
def reg(n,v):u.reg_write(getattr(R,f"UC_PPC_REG_{n}"),v)
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def read(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))
u.reg_write(R.UC_PPC_REG_MSR,8192)
kart=ram+0x2000;kart2=ram+0x2200
def hook(u,addr,size,_):
    off=addr-base[1]
    if off not in (0xbe90,0xbe98,0xbea4,0xbeb4,0xbeb8,0xbec4,0xbee8,0xbf04):return
    i=get(26)
    if off==0xbea4:trace.append(dict(contact_index=i,task=get(25),yaw=struct.unpack(">i",struct.pack(">I",get(4)))[0],call_8002f8c4=None,called_5b90=False))
    if off==0xbeb4:trace[-1]["matrix_input"]=list(read(get(4),"3f"))
    if off==0xbec4:
        assert get(3)==trace[-1]["task"]
    if off==0xbee8:trace[-1]["call_8002f8c4"]=[get(n) for n in (3,4,5,6)]
    if off==0xbf04:
        assert get(3)==get(25) and get(4)==0
        trace[-1]["called_5b90"]=True;reg(3,obs[i]["helper_5b90_result"]&0xffffffff)
    u.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,hook)
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for case,(flags,timer,ids,results) in enumerate([
        (0,1,[kart],[1]),(1,0,[],[]),(1,0,[0,ram+0x2600],[0,0]),
        (1,0.25,[kart,kart2,kart2],[0,-1,0]),(3,3.5,[kart2],[1]),(1,1,[kart],[1])]):
        trace=[];obs=[]
        put(registry,"6I",kart,0,0,0,0,kart2);reg(31,work);reg(1,ram+0x8000)
        put(work+12,"i",-1234);put(work+32,"3f",11.5,-2.25,timer);put(work+44,"I",coll)
        put(coll+2,"hH",len(ids),flags)
        for i,task in enumerate(ids):
            put(coll+20+i*8,"I",task)
            enabled=int(task==kart2)
            if task:put(task+56,"I",task+128);put(task+128+68,"I",enabled);put(task+128+112,"I",37)
            obs.append(dict(task=task,post_response_work_44=enabled,post_response_work_70=37,helper_5b90_result=results[i]))
        u.emu_start(base[1]+0xbdf4,base[1]+0xbf30,count=3000)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xbf30
        req=Path(tmp)/"request.json";out=Path(tmp)/f"{case}.json"
        req.write_text(json.dumps(dict(cooldown=timer,collision_flags=flags,registered_tasks=[kart,0,0,0,0,kart2],yaw=-1234,work_20=11.5,work_24=-2.25,contacts=obs)))
        subprocess.run([cli,"kart-ramp-tick",rel,out,req],check=True)
        assert json.loads(out.read_text())==dict(cooldown=read(work+40,"f")[0],dispatches=trace)
print("Matched six native contact loops, registry filtering, ordered calls and timer writes.")
