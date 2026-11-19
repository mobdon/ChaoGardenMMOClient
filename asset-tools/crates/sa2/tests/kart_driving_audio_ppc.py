"""Compare ordered vehicle sound dispatch and handle-relative arguments with CartD."""
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
        if t==1 and mod==80 and k in (4,6) and (0x190c<=l<0x1968 or 0x2a2c<=l<0x2b14 or 0x5774<=l<0x5b90 or 0x979c<=l<0x9814):
            p=base[s]+a
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))

def signed(x):return (x^0x80000000)-0x80000000
stops=(0x5850,0x5858,0x58a8,0x591c,0x5930,0x5948,0x595c,0x59c4,0x59d8,0x5a34,0x5aa8,0x5abc,0x5ad4,0x5ae8)
plays=(0x58a0,0x5904,0x5a20,0x5a90,0x5b38)
def hook(m,addr,*_):
    pc=addr-base[1];a,b,c,d,e,f=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in range(3,9)]
    if pc==0x57e8:
        assert get(a,"3f")==tuple((speed,0,0))
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",speed))[0])
    elif pc in (0x97ec,0x97fc):calls.append(dict(course_mode=signed(a)))
    elif pc in stops:calls.append(dict(stop=signed((a-parent)&0xffffffff)))
    elif pc in plays:
        assert f==tw+20
        calls.append(dict(play=dict(cue=signed(a),handle=signed((b-parent)&0xffffffff),args=[c,signed(d),e],position=list(get(f,"3f")))))
    elif pc==0x5b70:
        assert e==tw+20
        calls.append(dict(scripted=dict(cue=signed(a),handle=signed((b-parent)&0xffffffff),args=[c,signed(d)],position=list(get(e,"3f")))))
    else:calls.append(dict(pitch=dict(handle=signed((a-parent)&0xffffffff),channel=signed(b),value=signed(c))))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (*stops,*plays,0x57e8,0x97ec,0x97fc,0x598c,0x5b18,0x5b70):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
put(parent+0x34,"II",tw,wk);put(tw+20,"3f",7,-12,3.25)
put(wk+0xbc,"i",3);put(base[6]+0x24d84+12,"I",5);put(base[6]+12,"I",ram+0x3000)
threshold=get(base[4]+0x1f0,"f")[0]
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i in range(40):
        state=[0,1,2,3][i//8%4] if i<32 else [4,5,6,7,8,9,10,-1][i-32]
        j=i%8;piece=50 if j==0 else 49;flag=2 if j==7 else 0;speed=[0,5,threshold,threshold+.125,128.75,258,9,0][j]
        accel=int(j in (1,5));brake=int(j in (1,2,3,4));enabled=int(i!=30)
        profile=2 if i>=24 else i%9
        if i==24:
            u.reg_write(R.UC_PPC_REG_3,2);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
            u.emu_start(base[1]+0x2a2c,base[1]+0x200,count=100)
        q=dict(vehicle_state=state,work_44=enabled,velocity_xz=[speed,0],position=[7,-12,3.25],controls=dict(
            profile=profile,alternate=i>=24,work_284=flag,work_110=accel,work_114=brake,piece_id=piece,magnitude_return=speed or None))
        req=Path(td)/"input.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-driving-audio",rel,out,req],check=True);got=json.loads(out.read_text())
        put(wk+0x44,"i",enabled);put(wk+0x48,"i",profile);put(wk+0x30,"i",state);put(wk+0x7c,"3f",speed,30,0)
        put(wk+0x284,"i",flag);put(wk+0x110,"ii",accel,brake);put(ram+0x3005,"B",piece)
        u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200);calls=[]
        u.emu_start(base[1]+0x5774,base[1]+0x200,count=400)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
        assert got==dict(work_284=get(wk+0x284,"i")[0],calls=calls),(i,got,calls)
        # Execute the original caller through the return to contact. State 3
        # can follow windup movement; state 0 can follow sustained drift.
        for start in ([0x2498] if state==2 else [0x2498,0x24f8] if state==3 else [0x24f8] if state==0 else []):
            put(wk+0x284,"i",flag);calls=[]
            u.reg_write(R.UC_PPC_REG_29,parent);u.reg_write(R.UC_PPC_REG_3,0)
            u.emu_start(base[1]+start,base[1]+start+8,count=400)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+start+8
            assert got==dict(work_284=get(wk+0x284,"i")[0],calls=calls),(start,i)
print("40 vehicle sound dispatches and 32 drift caller sound traces matched through return.")
