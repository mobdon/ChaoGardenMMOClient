"""Compare active GATE writes and call order with installed PPC instructions."""
import hashlib,json,struct,subprocess,sys,tempfile
from gsne8p import steam_rel
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
cli,rel,private=map(Path,sys.argv[1:])
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,4:0x82000000,5:0x83000000,6:0x84000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    row=w(16)+8*sec;off=w(row)&~1;size=w(row+4)
    u.mem_map(addr,(size+4095)&~4095)
    if off:u.mem_write(addr,d[off:off+size])
u.mem_map(0x801cb000,0x2000)
variables={};callsites={}
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    while True:
        inc,k,s,a=steam_rel(struct.unpack_from(">HBBI", d, at), mod);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=inc
        if target==1 and (0xb0e4<=loc<0xb234 or 0x9164<=loc<0x9220):
            if k==10:callsites[loc]=a
            if k in (4,6):
                ptr=a if mod==0 else base[s]+a;variables[loc]=ptr
                u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536);task=ram+0x100;work=ram+0x200;coll=ram+0x400;primary=ram+0x800;kw=ram+0xc00;other=ram+0x1000
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
def hook(u,addr,size,_):
    off=addr-base[1]
    if off in (0xb14c,0xb164):
        assert callsites[off]==0x8006b978
        calls.append([u.reg_read(getattr(R,f"UC_PPC_REG_{n}")) for n in (3,4,5,6)])
        u.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,hook);u.reg_write(R.UC_PPC_REG_MSR,8192)
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for i,(phase,num,override,old,timer,prev,vals) in enumerate([
        (1,0,1,0,0,0,[0]*6),(1,2,0,1,15,-1,[-2]*6),(1,4,0,255,599,0,[1]*6),
        (1,2,0,0,0,0,[7]*6),(0,0,1,3,25,1,[99]*6),
        (0,0,0,2,1,2,[2]*6),(0,0,0,0,0.5,2,[1,0,-1,-2,0,2]),
        (0,0,0,0,-5,3,[0,0,3,0,0,0]),(0,0,0,0,0,1,[0,0,0,0,2,0]),
        (0,0,0,0,100,-1,[-1,-2,-3,-4,-5,-6])]):
        calls=[];put(work+6,"H",65535);put(work+1,"B",old);put(work+3,"B",9);put(work+40,"f",timer)
        put(variables[0xb0f2],"I",phase);put(variables[0xb106],"i",num)
        put(variables[0xb18e],"I",override);put(variables[0xb1fe],"i",prev)
        for j,v in enumerate(vals):
            kart=ram+0x1000+0x400*j
            put(variables[0x916a]+4*j,"I",kart);put(kart+0x38,"I",kart+0x100);put(kart+0x100+0xc4,"i",v)
        u.reg_write(R.UC_PPC_REG_31,work);u.emu_start(base[1]+0xb0e4,base[1]+0xb234,count=300)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xb234
        inp=dict(counter=65535,byte_1=old,byte_3=9,timer=timer,phase_active=bool(phase),phase_word=num,override_active=bool(override),previous_max=prev,kart_counters=vals)
        req=Path(tmp)/"req.json";out=Path(tmp)/f"{i}.json";req.write_text(json.dumps(inp))
        subprocess.run([cli,"kart-race-gate-tick",rel,out,req],check=True)
        assert len(calls)<=1
        assert json.loads(out.read_text())==dict(counter=get(work+6,"H"),byte_1=get(work+1,"B"),byte_3=get(work+3,"B"),timer=get(work+40,"f"),previous_max=get(variables[0xb1fe],"i"),call_8006b710=calls[0] if calls else None)
print("Matched ten racing gate updates and the six-kart maximum helper.")
