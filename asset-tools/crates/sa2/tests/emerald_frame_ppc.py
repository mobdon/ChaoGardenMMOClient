"""Compare the moving emerald frame coordinator with installed module 15."""
import itertools,json,os,subprocess,tempfile
from pathlib import Path
import wild_canyon_ppc as n
u,R=n.u,n.R
ccl=n.stage+1100;other=n.stage+1400;extra=n.stage+1800;shape=n.stage+2000
def address(site):
    return ((n.get(n.base[1]+site+2,"H")[0]<<16)+n.get(n.base[1]+site+6,"h")[0])&0xffffffff
table=address(0x22434);spin=n.get(address(0x226d0)+0x28,"I")[0]
def watch(cpu,pc,*_):
    site=pc-n.base[1];out=0
    if site==0x2233c:out=gate
    elif site==0x2238c:
        calls.append([site,0]);cpu.reg_write(R.UC_PPC_REG_PC,n.base[1]+0x223f4);return
    elif site==0x2244c:out=selection
    elif site==0x224c8:out=mode_query
    elif site in (0x22550,0x225ac):out=7 if site==0x22550 else 11
    elif site==0x22594:assert n.reg(4)==7;out=eligible
    elif site==0x225e8:assert n.reg(5)==11 and n.reg(6)==extra
    elif site==0x22678:assert n.get(shape+8,"fff")==world
    elif site==0x226c0:assert n.reg(5)==extra
    calls.append([site,out]);cpu.reg_write(R.UC_PPC_REG_3,out&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x2233c,0x2238c,0x2244c,0x224c8,0x22550,0x22594,0x225ac,0x225e8,0x225fc,0x22608,0x22678,0x226c0,0x226c8):
    u.hook_add(n.UC_HOOK_CODE,watch,begin=n.base[1]+site,end=n.base[1]+site)
# Relocate the one imported global load in this frame to controlled engine memory.
global_addr=n.stage+2400
n.put(n.base[1]+0x224d6,"H",(global_addr+32768)>>16);n.put(n.base[1]+0x224da,"H",global_addr&65535)
world=(7.,13.,-17.)
rows=[]
def run(mode,sub,allocated,gate_in,sel,flags,hits,gm,byte,owner,eligible_in,has_set,variant):
    global calls,gate,selection,mode_query,eligible
    calls=[];gate=gate_in;selection=sel;mode_query=gm;eligible=int(eligible_in)
    kind=n.get(table+36*variant,"I")[0]
    n.put(n.child+0x30,"II",int(has_set),n.work);n.put(n.child+0x40,"I",extra if allocated else 0)
    n.put(n.child+0x4c,"I",0xfffffff0);n.put(n.work,"bb",mode,sub);n.put(n.work+8,"I",variant<<8)
    n.put(n.work+0x2c,"I",ccl);n.put(ccl+4,"H",flags);n.put(ccl+12,"I",shape)
    n.put(ccl+0xa2,"hI",hits,other);n.put(other,"H",owner);n.put(other+0x9c,"I",n.child)
    n.put(global_addr+30,"b",byte);n.put(extra,"fff",*world)
    u.reg_write(R.UC_PPC_REG_1,n.stage+4000);u.reg_write(R.UC_PPC_REG_3,n.child);u.reg_write(R.UC_PPC_REG_LR,n.stop)
    u.emu_start(n.base[1]+0x22300,n.stop,count=400);assert u.reg_read(R.UC_PPC_REG_PC)==n.stop
    rows.append(dict(mode=mode,sub=sub,allocated=allocated,set=has_set,kind=kind,flags=flags,hits=hits,byte=byte,owner=owner,
                calls=calls,subAfter=n.get(n.work+1,"b")[0],spinAfter=n.get(n.child+0x4c,"I")[0],increment=spin))
for mode,sub,alloc,gate,sel in itertools.product((0,1,5),(-1,0,1,2,3,4),(False,True),(0,1),(-1,0,1,3,4)):
    run(mode,sub,alloc,gate,sel,0,0,0,0,0,False,True,0)
for variant in range(11):
 for hits,gm,byte,owner,elig,entry in itertools.product((0,1),(0,26),(0,1),(0,1,2),(False,True),(False,True)):
    run(1,2,True,0,1,1,hits,gm,byte,owner,elig,entry,variant)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    p=Path(tmp)/"frames.json";p.write_text(json.dumps(rows))
    subprocess.run(["cargo","test","--locked","--manifest-path","asset-tools/crates/sa2/Cargo.toml","--test","emerald_frame","--","--ignored"],
                   check=True,env=os.environ|{"SA2_EMERALD_FRAMES":str(p.resolve())})
print(len(rows),"moving emerald frame comparisons passed")
