"""Compare the native lap clock and split-subtraction helper without running a game."""
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
        if t!=1 or not 0x861c<=l<0x8824 or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cc168:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
clock=base[6]+0x36d88
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for j,(ticks,l1,l2,cumulative,inhibit,finished) in enumerate([
        (0,0,None,False,False,False),(58,0,1,False,False,False),
        (59,1,2,False,False,False),(3599,2,0,False,False,False),
        (7201,2,1,True,False,False),(700,3,3,False,False,False),
        (0x10000000,2,None,False,False,False),(2147483647,1,0,False,False,False),
        (3599,0,1,False,True,False),(3599,0,1,False,False,True)]):
        first=[[1,5,57],[1,15,65],[-1,0,0]];second=[[-1,8,4]]*3
        state=dict(ticks=ticks,display=[9,8,7],first=first,second=second)
        lap=dict(clock=state,inhibit=inhibit,cumulative=cumulative,first_counter=l1 if cumulative else 1,
                 first_highest=l1 if not cumulative else 0,second_highest=l2)
        q=dict(remove_task=False,pursuit_latch=1,race_finish_latch=int(finished),elapsed=0,remaining=123,
               engine_byte_09=0,engine_byte_42=0,flag_3ce24=0,engine_byte_16=0,lap_clock=lap)
        req=Path(td)/"q.json";out=Path(td)/f"{j}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-pursuit-clock",rel,out,req],check=True)
        doc=json.loads(out.read_text());assert doc["next"]=="slot_progress_8824"
        put(base[5]+0x2ec208,"i",inhibit);put(base[6]+0x24d60,"i",finished)
        put(clock+8,"i",ticks);put(base[6]+0x19c,"i",cumulative)
        for n,v in [(25,engine+256),(26,engine+512),(31,engine)]:
            u.reg_write(getattr(R,f"UC_PPC_REG_{n}"),v)
        put(engine,"b",l2 is not None);put(engine+256+0xc4,"i",lap["first_counter"])
        put(engine+256+0xc8,"i",lap["first_highest"]);put(engine+512+0xc8,"i",l2 or 0)
        u.mem_write(clock,bytes(state["display"]))
        for offset,rows in [(16,first),(32,second)]:
            for i,row in enumerate(rows):u.mem_write(clock+offset+i*3,struct.pack(">3b",*row))
        u.emu_start(base[1]+0x861c,base[1]+0x8824,count=350)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x8824
        triplet=lambda pos:list(struct.unpack(">3b",u.mem_read(clock+pos,3)))
        expected=dict(ticks=get(clock+8,"i"),display=triplet(0),
                      first=[triplet(16+i*3) for i in range(3)],second=[triplet(32+i*3) for i in range(3)])
        assert doc["lap_clock"]==expected,(j,doc["lap_clock"],expected)
print("Matched ten original lap-clock and split updates for both kart slots.")
