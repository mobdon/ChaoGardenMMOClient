"""Compare original kart movement query, landing dispatch and response writes."""
import math,json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
d=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,5:0x86000000,4:0x82000000,6:0x83000000};engine=0x84000000
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
        if t!=1 or not (0x2ca4<=l<0x2da8) or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cbd88:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
ram=0x85000000;u.mem_map(ram,65536)
task,work=ram+256,ram+512;put(task+0x38,"I",work)
def reg(i,val):u.reg_write(getattr(R,f"UC_PPC_REG_{i}"),val)
def run(start,end):
    u.emu_start(base[1]+start,base[1]+end,count=4000)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+end
tw=ram+0x1000;put(task+0x34,"I",tw)
reg(1,ram+0xf000);u.reg_write(R.UC_PPC_REG_MSR,8192)
def vec(a):return list(struct.unpack(">3f",u.mem_read(a,12)))
def hook(m,addr,*_):
    pc=addr-base[1];args=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in range(3,7)]
    a,b,c,d=args
    if pc==0x2d0c:
        assert a==work+0x118
        req=dict(from_=vec(a),to=vec(a+12),word_18=get(a+24,"I"),float_1c=get(a+28,"f"))
        req["from"]=req.pop("from_");assert req==doc["request"]
        if hit is None:m.emu_stop();return
        u.mem_write(a+32,struct.pack(">3f",*hit["resolved_position"]))
        reg(3,hit["return_word"] & 0xffffffff)
    elif pc==0x2d50:
        scale=struct.unpack(">d",struct.pack(">Q",m.reg_read(R.UC_PPC_REG_FPR1)))[0]
        calls.append(dict(Effect10860=dict(position=vec(a),velocity=vec(b),scale=scale)))
    elif pc==0x2d70:calls.append(dict(Engine8006b710=args))
    elif pc==0x2d84:calls.append(dict(Engine8002f8c4=args))
    else:
        assert c==work+0x7c
        u.mem_write(c,struct.pack(">3f",*(x-y for x,y in zip(vec(a),vec(b)))))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x2d0c,0x2d50,0x2d70,0x2d84,0x2df0):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(word,count,enabled) in enumerate([(None,0,0),(0,4,1),(0,2147483647,0),
                                            (1,30,1),(1,31,0),(-1,31,1),(3,-1,1)]):
        hit=None if word is None else dict(return_word=word,resolved_position=[11,-3.125,19])
        q=dict(previous_position=[8,-2,30],proposed_position=[10,-4,20],velocity=[2,-2,-10],
               player_slot=1,work_28c=count,work_44=enabled,result=hit)
        req=Path(td)/"q.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-contact-step",rel,out,req],check=True);doc=json.loads(out.read_text())
        for addr,key in [(work+20,"previous_position"),(tw+20,"proposed_position"),(work+0x7c,"velocity")]:
            u.mem_write(addr,struct.pack(">3f",*q[key]))
        put(work+0x28c,"i",count);put(work+0x44,"i",enabled);put(work+0x78,"i",1)
        reg(3,task);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200);calls=[]
        u.emu_start(base[1]+0x2ca4,base[1]+0x200,count=300)
        if hit is None:
            assert doc["response"] is None and u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x2d0c
            continue
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        reg(3,task);run(0x2da8,0x2e00 if word else 0x2f5c)
        expected=dict(work_74=get(work+0x74,"i"),work_28c=get(work+0x28c,"i"),
                      velocity=vec(work+0x7c),position=vec(tw+20),calls=calls,
                      next="contact_2e00" if word else "no_contact_2f5c")
        assert expected==doc["response"]
print("Matched seven contact request and response cases with supplied engine results.")
