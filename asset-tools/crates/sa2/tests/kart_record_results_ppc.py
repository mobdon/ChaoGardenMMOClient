"""Compare CartD race results using private synthetic memory, never a save file."""
import json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
d=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,5:0x82000000,6:0x83000000};engine=0x84000000
for s,p in base.items():
    row=struct.unpack_from(">I",d,16)[0]+s*8
    off,size=struct.unpack_from(">II",d,row);off &= ~1
    u.mem_map(p,(size+4095)&~4095)
    if off:u.mem_write(p,d[off:off+size])
u.mem_map(engine,0x10000)
record=engine+0x4000; index_word=engine+0x1000
off,n=struct.unpack_from(">II",d,40)
for mod,at in struct.iter_unpack(">II",d[off:off+n]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t!=1 or not 0x832c<=l<0x861c or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cc168:p=engine
        elif a==0x801f1684:p=index_word
        elif a==0x801f2340:p=record
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
services={0x83bc:(0x80062c4c,0),0x840c:(0x8001ef9c,1),0x8540:(0x8005ad34,1),
          0x8554:(0x8005bcdc,0),0x85ac:(0x8005ab64,4),0x85b4:(0x8005ad34,1),0x85c8:(0x8005bcdc,0)}
def intercept(m,addr,*_):
    target,argc=services[addr-base[1]]
    vals=[m.reg_read(getattr(R,f"UC_PPC_REG_{i+3}")) for i in range(argc)]
    calls.append([f"engine_{target:x}",[v if v<2**31 else v-2**32 for v in vals]])
    if target==0x8005ad34:m.reg_write(R.UC_PPC_REG_3,ret)
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in services:u.hook_add(UC_HOOK_CODE,intercept,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(elapsed,mode,place,multi,block,menu,byte23,ret,time) in enumerate([
        (59,5,1,0,0,0,0,256,[0,30,9]),(59,5,1,0,0,0,0,255,[0,30,9]),
        (59,5,1,0,0,0,0,0,[0,30,10]),(59,5,1,0,1,0,0,0,[0,20,1]),
        (59,2,1,0,0,0,0,0,[1,-5,7]),(59,4,1,0,0,0,0,3,[0,10,0]),
        (59,0,1,0,0,0,0,0,[0,0,0]),(59,2,None,0,0,0,0,0,[0,0,0]),
        (59,2,2,0,0,0,0,0,[0,0,0]),(59,2,1,1,0,0,0,0,[0,0,0]),
        (199,2,1,0,0,1,0,0,[0,0,0]),(199,2,1,0,0,0,0,0,[0,0,0]),
        (299,2,1,0,0,0,0,0,[0,0,0]),(299,2,1,0,0,0,1,0,[0,0,0])]):
        ctx=dict(mode=mode,first_slot_place=place,multiplayer_byte=multi,engine_byte_23=byte23,
                 menu_active=bool(menu),engine_word_b4=block,time=time,record=[0,30,10],record_index=2,
                 completion_index=3,check_return=ret)
        q=dict(remove_task=False,pursuit_latch=0,race_finish_latch=1,elapsed=elapsed,remaining=55,
               engine_byte_09=0,engine_byte_42=0,flag_3ce24=7,engine_byte_16=0,race_results=ctx)
        req=Path(td)/"q.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-pursuit-clock",rel,out,req],check=True);doc=json.loads(out.read_text())
        mem=bytearray([19])*0x4000;mem[0x34a0:0x34a3]=bytes(ctx["record"]);u.mem_write(record,bytes(mem))
        for p,fmt,v in [(base[6]+0x24d74,"i",elapsed),(base[6]+0x3ce24,"i",7),
            (base[6]+0x3ce20,"i",menu),(base[6]+0x21428,"I",engine+512 if place is not None else 0),
            (engine+512+0x38,"I",engine+1024),(engine+1024+0xc0,"i",place or 0),
            (engine+0x1e,"b",multi),(engine+0x23,"b",byte23),(engine+0xb4,"i",block),
            (engine+0xb0,"i",3),(engine+0x47,"b",12),(engine+0x16,"b",0),
            (base[6]+0x1a0,"i",mode),(index_word+4,"i",2)]:put(p,fmt,v)
        u.mem_write(base[6]+0x36d88,struct.pack(">3b",*time))
        u.reg_write(R.UC_PPC_REG_31,engine+0x1e);calls=[]
        u.emu_start(base[1]+0x83bc,base[1]+0x861c,count=200)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x861c
        a=doc["race_results"]
        for start,v in a["writes"]:mem[start:start+len(v)]=struct.pack(f">{len(v)}b",*v)
        assert bytes(u.mem_read(record,len(mem)))==mem
        assert calls==[[doc["calls"][0][0],[]]]+a["calls"]
        assert get(engine+0x47,"b")== (12 if a["engine_byte_47_write"] is None else a["engine_byte_47_write"])
        assert [get(base[6]+0x24d74,"i"),get(base[6]+0x3ce24,"i"),get(engine+0x16,"b")]==[doc["elapsed"],doc["flag_3ce24"],doc["engine_byte_16"]]
print("Matched 14 race-result paths, call arguments and complete synthetic record memory.")
