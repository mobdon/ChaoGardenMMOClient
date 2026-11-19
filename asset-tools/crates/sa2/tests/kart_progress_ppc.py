"""Run one occupied slot of the native course progress loop; supply segment test results."""
import hashlib,json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
cli,rel,private=map(Path,sys.argv[1:])
d=rel.read_bytes();assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
row=w(16)+8;off=w(row)&~1;size=w(row+4)
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base=0x81000000;ram=0x84000000
u.mem_map(base,(size+4095)&~4095);u.mem_write(base,d[off:off+size]);u.mem_map(ram,0x10000)
def reg(n,v):u.reg_write(getattr(R,f"UC_PPC_REG_{n}"),v)
def put(p,v):u.mem_write(p,struct.pack(">I",v&0xffffffff))
def signed(v):return struct.unpack(">i",struct.pack(">I",v))[0]
def read(p):return struct.unpack(">i",u.mem_read(p,4))[0]
def hook(u,addr,size,_):
    loc=addr-base
    if loc not in (0x9220,0x93f0):return
    idx=signed(u.reg_read(R.UC_PPC_REG_4))
    if loc==0x9220:
        assert u.reg_read(R.UC_PPC_REG_3)==task
        tested.append(idx);reg(3,int(next(results)))
    else:debug.append(idx)
    u.reg_write(R.UC_PPC_REG_PC,u.reg_read(R.UC_PPC_REG_LR))
u.hook_add(UC_HOOK_CODE,hook)
task=ram+1024;tw=ram+2048;wk=ram+4096
with tempfile.TemporaryDirectory(dir=private) as tmp:
    cases=[(87,1,2,[False]*k+[True]) for k in range(9)]
    cases += [(x,1,mode,[False]*9+[False]*k+[True]) for x in (0,5) for mode in (0,1,2) for k in (0,1)]
    cases += [(5,0,0,[False]*3),(5,1,0,[False]*11),(86,0,2,[False,True])]
    for i,(piece,extended,mode,tests) in enumerate(cases):
        tested=[];debug=[];results=iter(tests);counter=(2**31-1 if i==0 else 2);highest=3
        reg(23,ram);reg(24,ram+0x2000);reg(25,ram+0x3000);reg(29,ram+0x4000)
        put(ram,task);put(task+0x34,tw);put(task+0x38,wk);put(wk+0xbc,piece)
        put(wk+0xc4,counter);put(wk+0xc8,highest);put(wk+0x44,extended)
        put(ram+0x219c,mode);put(ram+0x3000,88);put(ram+0x4000,i%2)
        u.reg_write(R.UC_PPC_REG_MSR,8192)
        u.emu_start(base+0x885c,base+0x8d60,count=1000)
        assert u.reg_read(R.UC_PPC_REG_PC)==base+0x8d60 and next(results,None) is None
        inp=dict(piece=piece,counter=counter,highest=highest,course_length=88,work_44=extended,mode=mode,debug_enabled=bool(i%2),segment_tests=tests)
        req=Path(tmp)/"req.json";out=Path(tmp)/f"{i}.json";req.write_text(json.dumps(inp))
        subprocess.run([cli,"kart-progress-tick",rel,out,req],check=True)
        assert json.loads(out.read_text())==dict(piece=read(wk+0xbc),counter=read(wk+0xc4),highest=read(wk+0xc8),tested_indices=tested,debug_indices=debug),(i,inp)
print(f"Matched {len(cases)} course updates, including every forward and reverse branch.")
