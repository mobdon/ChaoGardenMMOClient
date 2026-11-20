"""Run the installed jump-panel post-allocation block; observe both engine calls."""
from pathlib import Path
from gsne8p import steam_rel
import hashlib,json,struct,subprocess,sys,tempfile
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import ppc_const as R

cli,rel,private=map(Path,sys.argv[1:])
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
w=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,4:0x82000000,5:0x83000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    entry=w(16)+8*sec; offset=w(entry)&~1; size=w(entry+4)
    u.mem_map(addr,(size+4095)&~4095);u.mem_write(addr,d[offset:offset+size])
calls={}
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    while True:
        inc,k,s,a=steam_rel(struct.unpack_from(">HBBI", d, at), mod);at+=8
        if k==203:break
        if k==202:target=s;off=0;continue
        off+=inc
        if target==1 and 0xbd04<=off<0xbdf0:
            if mod==0 and k==10:calls[off]=a
            elif k in (4,6):
                val=base[s]+a
                u.mem_write(base[1]+off,struct.pack(">H",(val if k==4 else (val+32768)>>16)&65535))
        if target==5 and 0x2edb90<=off<0x2edbc8 and k==1:
            u.mem_write(base[5]+off,struct.pack(">I",base[s]+a))
task=0x85000000;work=task+256;node=task+512
u.mem_map(task,4096);u.reg_write(R.UC_PPC_REG_MSR,8192)
def run(a,b):
    u.emu_start(base[1]+a,base[1]+b,count=100)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+b
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for i,(pos,yaw) in enumerate([([0.,0.,0.],0),([200.5,-40.,-815.25],-32768),([-0.,3.125,0.],0x12345678)]):
        req=Path(tmp)/"pose.json";out=Path(tmp)/f"{i}.json";req.write_text(json.dumps(dict(position=pos,yaw=yaw)))
        subprocess.run([cli,"kart-ramp-init",rel,out,req],check=True)
        got=json.loads(out.read_text())
        u.mem_write(task+0x38,struct.pack(">I",node))
        u.mem_write(work+12,struct.pack(">i",yaw));u.mem_write(work+20,struct.pack(">3f",*pos))
        u.mem_write(work+0x28,b"\xAB"*4);u.mem_write(node,b"\xCD"*56)
        u.reg_write(R.UC_PPC_REG_30,task);u.reg_write(R.UC_PPC_REG_31,work)
        run(0xbd04,0xbdc4)
        words=list(struct.unpack(">14I",u.mem_read(node,56)))
        words[1]-=base[got["attachment_section"]]
        assert words==got["node_words"]
        assert (calls[0xbdc4],u.reg_read(R.UC_PPC_REG_3),u.reg_read(R.UC_PPC_REG_4))==(got["first_call_address"],got["first_call_flags"],task)
        run(0xbdc8,0xbddc)
        assert [u.reg_read(r) for r in (R.UC_PPC_REG_3,R.UC_PPC_REG_4,R.UC_PPC_REG_5,R.UC_PPC_REG_6)]==[task,base[got["collision_info_section"]]+got["collision_info_offset"],got["collision_info_count"],got["collision_info_kind"]]
        assert calls[0xbddc]==got["second_call_address"]
        run(0xbde0,0xbdf0)
        assert struct.unpack(">I",u.mem_read(work+0x28,4))[0]==got["task_word_28"]
print("Matched all 14 initialized words, both external call arguments and final work field for three task poses.")
