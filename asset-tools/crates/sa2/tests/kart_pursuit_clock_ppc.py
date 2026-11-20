"""Compare CartD branches from 0x832c to the next controller block."""
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
u.mem_map(engine,4096)
off,n=struct.unpack_from(">II",d,40)
for mod,at in struct.iter_unpack(">II",d[off:off+n]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t!=1 or not 0x832c<=l<0x861c or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cbd88:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
stops={0x861c:"shared_timing_861c",0x913c:"return",0x83bc:"race_results_83bc"}
def intercept(m,addr,*unused):
    pc=addr-base[1]
    if pc in stops:m.emu_stop();return
    if pc==0x8340:
        assert m.reg_read(R.UC_PPC_REG_3)==engine+0x100
        calls.append(["engine_80071888_task",None])
    elif pc==0x835c:calls.append(["engine_800629e4",None])
    else:calls.append(["engine_8005eb24",m.reg_read(R.UC_PPC_REG_3)])
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in [*stops,0x8340,0x835c,0x8384]:
    u.hook_add(UC_HOOK_CODE,intercept,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(remove,catch,race,elapsed,b9,b42) in enumerate([
        (True,1,1,299,0,0),(False,1,1,298,0,0),(False,1,1,299,0,0),
        (False,-1,0,300,0,0),(False,1,0,2147483647,0,0),(False,0,1,299,0,0),
        (False,0,0,42,0,0),(False,0,0,42,1,0),(False,0,0,42,1,-1)]):
        remaining=-2147483648 if i==8 else 321
        q=dict(remove_task=remove,pursuit_latch=catch,race_finish_latch=race,elapsed=elapsed,
               remaining=remaining,engine_byte_09=b9,engine_byte_42=b42,flag_3ce24=4,engine_byte_16=-2)
        req=Path(td)/"q.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-pursuit-clock",rel,out,req],check=True)
        for p,fmt,v in [(base[5]+3064344,"i",remove),(base[6]+0x3ce2c,"i",catch),
            (base[6]+0x24d60,"i",race),(base[6]+0x24d74,"i",elapsed),(base[6]+0x36d94,"i",remaining),
            (base[6]+0x3ce24,"i",4),(engine+9,"b",b9),(engine+0x42,"b",b42),(engine+0x16,"b",-2)]:
            put(p,fmt,v)
        u.reg_write(R.UC_PPC_REG_3,engine+0x100);calls=[]
        u.emu_start(base[1]+0x832c,base[1]+0x9400,count=100)
        got=dict(elapsed=get(base[6]+0x24d74,"i"),remaining=get(base[6]+0x36d94,"i"),
                 flag_3ce24=get(base[6]+0x3ce24,"i"),engine_byte_16=get(engine+0x16,"b"),
                 calls=calls,next=stops[u.reg_read(R.UC_PPC_REG_PC)-base[1]])
        assert got==json.loads(out.read_text()),got
print("Matched nine pursuit controller branches, writes and outgoing calls.")
