"""Compare active GATE2 writes and call order with installed PPC instructions."""
import hashlib,json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
cli,rel,private=map(Path,sys.argv[1:4])
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,6:0x84000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    row=w(16)+8*sec;off=w(row)&~1;size=w(row+4)
    u.mem_map(addr,(size+4095)&~4095)
    if off:u.mem_write(addr,d[off:off+size])
u.mem_map(0x801cb000,0x2000)
variables={};callsites={}
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=inc
        if target==1 and (0xb74c<=loc<0xb81c or 0x1714<=loc<0x17e8 or 0x85e0<=loc<0x861c):
            if k==10:callsites[loc]=a
            if k in (4,6):
                ptr=a if mod==0 else base[s]+a;variables[loc]=ptr
                u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536);task=ram+0x100;work=ram+0x200;coll=ram+0x400;primary=ram+0x800;kw=ram+0xc00;other=ram+0x1000
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
def hook(u,addr,size,_):
    off=addr-base[1]
    if off==0xb798:
        assert [u.reg_read(getattr(R,f"UC_PPC_REG_{n}")) for n in (3,4,5,6)]==[25,0,0,0]
        calls.append(f"main:{callsites[off]:x}(25,0,0,0)")
    elif off==0xb7ec:
        assert u.reg_read(R.UC_PPC_REG_3)==task
        calls.append(f"main:{callsites[off]:x}(gate_task)")
    elif off in (0xb7e0,0xb7e4):
        disp=w(232+off)&0x03fffffc
        calls.append(f"cart:1:{off+(disp if disp<0x2000000 else disp-0x4000000):x}")
    else:return
    u.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,hook);u.reg_write(R.UC_PPC_REG_MSR,8192)
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for i,(latch,flags,first,param,timer,counter,final) in enumerate([
        (0,1,other,12.75,20,0,4),(1,1,other,12.75,740,1,0),
        (0,0,primary,10,0,0,4),(0,1,0,5,0,1,1),(0,3,ram+4,5,0,1,0),
        (0,3,primary,-1.75,20,0xffffffff,5),(0,1,primary,2,0xfffffff0,4,0)]):
        calls=[];registry=[primary,0,0,0,0,other]
        put(variables[0x1716],"6I",*registry);put(work+6,"H",latch);put(work+0x24,"f",param)
        put(work+0x2c,"I",coll);put(coll+4,"H",flags);put(coll+20,"II",first,0);put(coll+28,"I",other)
        put(variables[0xb79e]+12,"I",timer);put(variables[0xb7c6],"I",primary);put(primary+56,"I",kw)
        put(kw+0xc4,"I",counter);put(variables[0xb7f2]+0x7c,"I",final)
        u.reg_write(R.UC_PPC_REG_1,ram+0xf000);u.reg_write(R.UC_PPC_REG_31,work);u.reg_write(R.UC_PPC_REG_30,task)
        u.emu_start(base[1]+0xb74c,base[1]+0xb81c,count=300)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xb81c
        inp=dict(latch=latch,collision_flags=flags,first_contact=first,registered_tasks=registry,work_24=param,timer_word=timer,primary_kart_counter=counter,final_flags=final)
        req=Path(tmp)/"req.json";out=Path(tmp)/f"{i}.json";req.write_text(json.dumps(inp))
        subprocess.run([cli,"kart-gate-tick",rel,out,req],check=True)
        assert json.loads(out.read_text())==dict(latch=get(work+6,"H"),timer_word=get(variables[0xb79e]+12,"I"),primary_kart_counter=get(kw+0xc4,"I"),work_byte_3=get(work+3,"B"),calls=calls)
print("Matched seven GATE2 traces including one-shot credit, first-contact filtering and timer wrap.")

# Optional installed executable enables coupled traces across both authored gates.
if len(sys.argv)>4:
    exe=Path(sys.argv[4])
    with tempfile.TemporaryDirectory(dir=private) as td:
        for path in sorted([*rel.parent.glob("setCartTails*.bin"),*rel.parent.glob("setCartRouge*.bin")]):
            rows=list(struct.iter_unpack(">4H6f",path.read_bytes()[32:]))
            a,b=[i for i,r in enumerate(rows) if r[0]&4095==4]
            registry=[primary,0,0,0,0,other]
            put(variables[0x1716],"6I",*registry);put(variables[0xb7c6],"I",primary);put(primary+56,"I",kw)
            put(variables[0xb79e]+12,"I",0xfffffff0);put(kw+0xc4,"I",4)
            slots={a:work,b:work+128}
            for p in slots.values():put(p+6,"H",0);put(p+0x2c,"I",coll)
            events=[];expected=[]
            for index,flags,first in [(b,1,0),(b,1,other),(a,0,primary),(b,1,primary),(a,1,primary),(a,1,other)]:
                # Run the countdown instructions on the very same emulated memory word.
                assert variables[0xb79e]==variables[0x860a]
                b9,b42=(1,0) if len(events)==2 else (0,0)
                put(variables[0x85e2]+9,"b",b9);put(variables[0x85e2]+0x42,"b",b42)
                u.emu_start(base[1]+0x85e0,base[1]+0x861c,count=32)
                events.append(dict(controller=dict(remove_task=False,pursuit_latch=0,race_finish_latch=0,engine_byte_09=b9,engine_byte_42=b42)))
                expected.append(get(variables[0xb79e]+12,"i"))
                p=slots[index];put(p+0x24,"f",rows[index][8]);put(coll+4,"H",flags);put(coll+20,"I",first)
                put(variables[0xb7f2]+0x7c,"I",4)
                u.reg_write(R.UC_PPC_REG_1,ram+0xf000);u.reg_write(R.UC_PPC_REG_31,p);u.reg_write(R.UC_PPC_REG_30,task)
                calls=[];u.emu_start(base[1]+0xb74c,base[1]+0xb81c,count=300)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xb81c
                expected.append((get(p+6,"H"),get(variables[0xb79e]+12,"I"),get(kw+0xc4,"I"),calls))
                events.append(dict(placement=index,collision_flags=flags,first_contact=first,final_flags=4))
            req=Path(td)/"contacts.json";out=req.with_name(path.stem+".json")
            req.write_text(json.dumps(dict(timer_word=0xfffffff0,primary_kart_counter=4,sequence=0,registered_tasks=registry,updates=events)))
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());assert doc["sequence"]==2
            for event,ref in zip(doc["updates"],expected,strict=True):
                if isinstance(ref,int):assert event["controller"]["remaining"]==ref;continue
                r=event["result"];assert (r["latch"],r["timer_word"],r["primary_kart_counter"],r["calls"])==ref
    print("Matched 24 gate and 24 interleaved countdown updates across four installed story layouts.")
