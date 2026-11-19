"""Compare vehicle initialization writes to the complete taskwork and vehicle buffers with CartD."""
from pathlib import Path
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
data=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x82000000,6:0x83000000,5:0x84000000}
for s,addr in base.items():
    row=struct.unpack_from(">I",data,16)[0]+s*8
    off,n=struct.unpack_from(">II",data,row);off &= ~1
    u.mem_map(addr,(n+4095)&~4095)
    if off:u.mem_write(addr,data[off:off+n])
o,n=struct.unpack_from(">II",data,40)
for mod,at in struct.iter_unpack(">II",data[o:o+n]):
    while True:
        delta,k,s,a=struct.unpack_from(">HBBI",data,at);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t==1 and mod==80 and k in (4,6) and 0x2898<=l<0x2a2c:
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))
put(parent+0x34,"II",tw,wk)
u.reg_write(R.UC_PPC_REG_MSR,8192)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(enabled,slot) in enumerate([(1,0),(1,1),(0,0),(0,5),(-1,1),(0,2147483647)]):
        q=dict(work_44=enabled,player_slot=slot,position=[4.125,-2,33],angles=[1,-2147483647+i,19],
               profile_words=[0x12345678+i,0x80000000,0xffffffff-i,0x01020304])
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-vehicle-init",rel,out,req],check=True);got=json.loads(out.read_text())
        u.mem_write(wk,b'\xa7'*0x300);u.mem_write(tw,b'\xa7'*0x40)
        put(wk+0x44,"iii",enabled,i,slot);put(wk+0x50,"3f3i",*q["position"],*q["angles"])
        for off,v in zip((0x14,0x18,0x1c,0x28),q["profile_words"]):
            put(base[6]+4+i*28+off,"I",v)
        expected=bytearray(u.mem_read(wk,0x300));pose=bytearray(u.mem_read(tw,0x40))
        struct.pack_into(">3i",pose,8,*got["angles"]);struct.pack_into(">3f",pose,20,*got["position"])
        for off in got["cleared_work_offsets"]:struct.pack_into(">I",expected,off,0)
        for off,fmt,vs in [(20,"3f",got["previous_position"]),(0x7c,"3f",got["velocity"]),
                           (0x30,"i",[got["vehicle_state"]]),(0x34,"4I",got["profile_words_34_to_40"]),
                           (0x70,"i",[got["work_70"]]),(0x78,"i",[got["player_slot"]])]:
            struct.pack_into(">"+fmt,expected,off,*vs)
        for k,v in (got["guidance"] or {}).items():
            struct.pack_into(">f" if k=="work_264" else ">i",expected,int(k[5:],16),v)
        u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
        u.emu_start(base[1]+0x2898,base[1]+0x200,count=180)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        assert bytes(expected)==bytes(u.mem_read(wk,len(expected))) and bytes(pose)==bytes(u.mem_read(tw,len(pose)))
print("Six vehicle initializations matched all bytes of both work buffers.")
