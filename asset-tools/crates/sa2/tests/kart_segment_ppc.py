"""Compare the crossing triangle and projected kart segment to CartD PowerPC."""
from pathlib import Path
import hashlib,json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
d=rel.read_bytes();assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x82000000,6:0x83000000}
for s,addr in base.items():
    row=w(16)+8*s;off=w(row)&~1;n=w(row+4)
    u.mem_map(addr,(n+4095)&~4095)
    if off:u.mem_write(addr,d[off:off+n])
calls={}
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=inc
        if target==1 and 0x93ac<=loc<=0x93d4 and k==10:
            calls[loc]=(mod,s,a)
        if target==1 and 0x9220<=loc<0x93ac and mod==80 and k in (4,6):
            val=base[s]+a
            u.mem_write(base[1]+loc,struct.pack(">H",(val if k==4 else (val+32768)>>16)&65535))
            if loc==0x9252:table=val
assert calls=={0x93ac:(0,2,0x8011b8e8),0x93bc:(0,2,0x8011b098),
              0x93c4:(0,2,0x801159dc),0x93d4:(0,2,0x800c2178)}
ram=0x85000000;u.mem_map(ram,65536);tw=ram+256;wk=ram+512;sp=ram+0x8000
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
with tempfile.TemporaryDirectory(dir=tmp) as root:
    for i,(start,end,quad) in enumerate([
        ([1,2,3],None,[[1,2]]*4),([1,2,3],[1,-100,3],[[1,2]]*4),
        ([2,4,7],[-1,99,4],[[2,0],[55,55],[1,3],[-4,1]]),
        ([-64.25,8,0],[0,8,-0.5],[[1.125,-4.5],[17,-32],[-4.5,5],[7.25,-1.125]]),
        ([4,0,8],[4,0,12],[[0,0]]*4)]):
        req=Path(root)/"q.json";out=Path(root)/f"{i}.json"
        req.write_text(json.dumps(dict(task_position=start,work_position=end,segment_xz=quad)))
        subprocess.run([cli,"kart-segment-geometry",rel,out,req],check=True);result=json.loads(out.read_text())
        put(ram+0x34,"II",tw,0 if end is None else wk);put(tw+20,"3f",*start)
        if end is not None:put(wk+20,"3f",*end)
        put(table+i*32,"8f",*(x for pair in quad for x in pair))
        u.reg_write(R.UC_PPC_REG_1,sp);u.reg_write(R.UC_PPC_REG_3,ram);u.reg_write(R.UC_PPC_REG_4,i)
        u.reg_write(R.UC_PPC_REG_MSR,8192);stop=base[1]+0x200;u.reg_write(R.UC_PPC_REG_LR,stop)
        # Stop at the imported geometry call or the function return.
        from unicorn import UC_HOOK_CODE
        h=u.hook_add(UC_HOOK_CODE,lambda uc,*_:uc.emu_stop(),begin=base[1]+0x93ac,end=base[1]+0x93ac)
        u.emu_start(base[1]+0x9220,stop,count=180);u.hook_del(h)
        pc=u.reg_read(R.UC_PPC_REG_PC)
        if pc==stop:
            assert u.reg_read(R.UC_PPC_REG_3)==0 and result=={"state":"miss"}
        else:
            assert pc==base[1]+0x93ac and result["state"]=="requires_intersection"
            stack=u.reg_read(R.UC_PPC_REG_1)
            for offset,vals in [(0x34,sum(result["triangle"],[])),(0x70,result["origin"]),(0x7c,result["direction"])]:
                assert bytes(u.mem_read(stack+offset,len(vals)*4))==struct.pack(">"+str(len(vals))+"f",*vals)
            # Resume at the instruction after the last imported call, with its
            # return register injected. Only the native epilogue is under test.
            for value in (0,1,0xffffffff):
                u.reg_write(R.UC_PPC_REG_1,stack);u.reg_write(R.UC_PPC_REG_3,value)
                u.emu_start(base[1]+0x93d8,stop,count=16)
                assert u.reg_read(R.UC_PPC_REG_PC)==stop and u.reg_read(R.UC_PPC_REG_3)==value
print("Matched five native segment probes, including early returns and all geometry operand bits.")
