"""Compare fall recovery pose, timers, route counters and engine calls with CartD."""
from pathlib import Path
from gsne8p import steam_rel
import json,struct,subprocess,sys,tempfile
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:4])
exe=Path(sys.argv[4]) if len(sys.argv)>4 else None
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
        delta,k,s,a=steam_rel(struct.unpack_from(">HBBI", data, at), mod);at+=8
        if k==203:break
        if k==202:t=s;l=0;continue
        l+=delta
        if t==1 and k in (4,6) and (0x4b50<=l<0x4eb4 or 0x30dc<=l<0x3188 or 0x7100<=l<0x711c):
            p=base[s]+a if mod==80 else 0x85003000
            u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,65536)
parent,tw,wk,child,cw=[ram+x for x in (256,512,1024,2048,2304)]
def put(a,fmt,*v):u.mem_write(a,struct.pack(">"+fmt,*v))
def get(a,fmt):return struct.unpack(">"+fmt,u.mem_read(a,struct.calcsize(">"+fmt)))


def hook(m,a,*_):
    if a==base[1]+0x3134:
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",.45))[0])
    else:calls.append(m.reg_read(R.UC_PPC_REG_3))
    m.reg_write(R.UC_PPC_REG_PC,a+4)
for pc in (0x4bf8,0x4c04,0x3134):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
u.reg_write(R.UC_PPC_REG_MSR,8192);u.reg_write(R.UC_PPC_REG_1,ram+0xe000)
put(parent+0x34,"II",tw,wk)
fields=dict(position=(tw+20,"3f"),angles=(tw+8,"3i"),task_mode=(tw,"B"),
    saved_position=(wk+20,"3f"),saved_angles=(wk+8,"3i"),grounded_position=(wk+0x268,"3f"),
    velocity=(wk+0x7c,"3f"),work_68=(wk+0x68,"i"),work_fc_104=(wk+0xfc,"3i"),
    state=(wk+48,"i"),segment=(wk+0xbc,"i"),lap=(wk+0xc4,"i"),work_cc=(wk+0xcc,"i"),
    work_254=(wk+0x254,"i"),work_25c=(wk+0x25c,"i"),timer=(wk+0x294,"i"))
with tempfile.TemporaryDirectory(dir=tmp) as td:
    courses=[None]
    if exe:
        directory=Path(td)/"directory.json"
        subprocess.run([cli,"kart-crossing-directory",rel,directory,exe],check=True)
        courses+=json.loads(directory.read_text())["courses"]
        assert [r["course"] for r in courses[1:]]==["101","280","beginner","standard","expert"]
    for n,course in enumerate(courses):
        count=course["segment_count"] if course else 20
        if course:
            u.emu_start(base[1]+0x7100,base[1]+0x7120,count=16)
            row=ram+0x4000;u.reg_write(R.UC_PPC_REG_28,row)
            for pose in course["projected_occurrence_poses"][1:]:
                put(row+8,"f",pose["translation"][1])
                u.emu_start(base[1]+0x736c,base[1]+0x7388,count=10)
            native_height=get(base[6]+0x24d68,"f")[0]
            bits=struct.unpack(">I",struct.pack(">f",native_height))[0]
            below=struct.unpack(">f",struct.pack(">I",0x80000001 if native_height==0 else bits+(1 if native_height<0 else -1)))[0]
        for i,(height,mode,life,other,timer,seg) in enumerate([
            (-5,0,0,0,0,2), (3,0,0,1,0,2), (3,1,1,1,1,2), (3,0,2,1,180,2),
            (3,0,2,1,181,2),(3,2,0,1,0,2),(3,3,0,1,0,8),(3,3,0,0,12,2),(3,0,1,0,11,2)]):
            v=dict(position=[1,-5,3],angles=[4,5,6],task_mode=7,saved_position=[20,21,22],saved_angles=[33,34,35],
                   grounded_position=[0,25,8],velocity=([0,8,0] if i==8 else [17,8,19]),work_68=12,work_fc_104=[2,3,4],
                   state=4,segment=seg,lap=2,work_cc=8,work_254=12,work_25c=14,timer=timer)
            if course:
                height=native_height
                v["position"][1]=height if i==0 else below
            for k,(addr,fmt) in fields.items():put(addr,fmt,*(v[k] if isinstance(v[k],list) else [v[k]]))
            g=base[6];put(wk+0x44,"i",other);put(wk+0x74,"i",i%2);put(wk+0x260,"i",3)
            put(g+0x24d68,"f",height);put(g+0x1a0,"i",mode);put(ram+0x3064,"h",life);put(g+0x24d80,"i",count)
            ix=(9 if not other else seg-5 if seg>=5 else 1 if mode in (0,1) else count+seg-5)
            corner=1 if other else 3
            if course:
                for j,quad in enumerate(course["projected_segment_xz"]):
                    put(g+0x26d84+32*j,"8f",*(n for pair in quad for n in pair))
            else:put(g+0x26d84+32*ix+8*corner,"2f",72,73)
            q=dict(vehicle=v,work_74=i%2,work_44=other,work_260=3,mode=mode,engine_halfword_64=life,
                   segment_count=count,fall_height=height,vertex=dict(segment=ix,corner=corner,xz=[72,73]),angle_radians=.45)
            if course:
                del q["segment_count"],q["vertex"],q["fall_height"]
            req=Path(td)/"request.json";out=Path(td)/f"{n}-{i}.json";req.write_text(json.dumps(q))
            subprocess.run(([cli,"kart-course-recovery",rel,out,exe,course["course"],req] if course else
                            [cli,"kart-fall-recovery",rel,out,req]),check=True)
            got=json.loads(out.read_text());calls=[]
            if not course:
                gate=dict(work_70=-1,state=4,global_22494=0,global_2ec208=1,post_contact_fall=q)
                req.write_text(json.dumps(gate));gout=Path(td)/f"gate-{i}.json"
                subprocess.run([cli,"kart-update-gate",rel,gout,req],check=True)
                assert json.loads(gout.read_text())["fall"]==got
            u.reg_write(R.UC_PPC_REG_3,parent);u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200)
            u.emu_start(base[1]+0x4b50,base[1]+0x200,count=300)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x200
            for k,(addr,fmt) in fields.items():
                val=got["vehicle"][k]
                assert struct.pack(">"+fmt,*(val if isinstance(val,list) else [val]))==bytes(u.mem_read(addr,struct.calcsize(">"+fmt))),(i,k,val,get(addr,fmt))
            assert calls==([] if got["engine_call_8001ef18"] is None else [got["engine_call_8001ef18"]])
print(f"{9*len(courses)} fall callbacks matched through return.")
