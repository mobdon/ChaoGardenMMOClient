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
        if t!=1 or not (0x2ca4<=l<0x3024 or 0x5220<=l<0x52b0) or k not in (4,6):continue
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
        if "returned_from" in hit:u.mem_write(a,struct.pack(">3f",*hit["returned_from"]))
        reg(3,hit["return_word"] & 0xffffffff)
    elif pc==0x2d50:
        scale=struct.unpack(">d",struct.pack(">Q",m.reg_read(R.UC_PPC_REG_FPR1)))[0]
        calls.append(dict(Effect10860=dict(position=vec(a),velocity=vec(b),scale=scale)))
    elif pc==0x2fac:
        assert (a,b,c)==(tw+8,work+0x7c,work+0x88)
        assert list(struct.unpack(">3i",u.mem_read(a,12)))==tail["post_alignment_angles"]
        u.mem_write(c,struct.pack(">3f",*tail["observed_local_velocity"]))
    elif pc==0x2fcc:
        assert a==work+0x7c
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",tail["observed_raw_magnitude"]))[0])
    elif pc==0x2f18:calls.append("Cart9884")
    elif pc in (0x2ee8,0x2ef4):calls.append(dict(Engine80011714=a))
    elif pc==0x2f30:
        assert vec(a)==w["normal"]
        put(b,"i",0);put(c,"i",0)
    elif pc in (0x2f40,0x2f54):
        assert c==182
        reg(3,0)
    elif pc==0x2f74:u.mem_write(c,struct.pack(">3f",*(x+y for x,y in zip(vec(a),vec(b)))))
    elif pc in (0x2d70,0x2e90):calls.append(dict(Engine8006b710=args))
    elif pc in (0x2d84,0x2ea4):calls.append(dict(Engine8002f8c4=args))
    else:
        assert c==work+0x7c
        u.mem_write(c,struct.pack(">3f",*(x-y for x,y in zip(vec(a),vec(b)))))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x2d0c,0x2d50,0x2d70,0x2d84,0x2df0,0x2e90,0x2ea4,0x2ee8,0x2ef4,0x2f18,0x2f30,0x2f40,0x2f54,0x2f74,0x2fac,0x2fcc):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(word,count,enabled) in enumerate([(None,0,0),(0,4,1),(0,2147483647,0),
                                            (1,30,1),(1,31,0),(-1,31,1),(3,-1,1)]+[(1,0,1)]*5+[(0,0,0)]):
        hit=None if word is None else dict(return_word=word,resolved_position=[11,-3.125,19])
        q=dict(previous_position=[8,-2,30],proposed_position=[10,-4,20],velocity=[2,-2,-10],
               player_slot=1,work_28c=count,work_44=enabled,result=hit)
        if i>=7:
            j=i-7
            w=dict(normal=[1,[0,.125,0,.25,0,0][j],-1],impulse=[1,2,3],work_70=-1 if j==4 else j%2,
                   cooldown=5 if j==2 else 0,mode=j%2,vehicle_state=2,engine_byte_23=3)
            q["wall"]=w;hit["returned_from"]=[1,2,3]
            for off,fmt,v in [(0x70,"i",w["work_70"]),(0x290,"i",w["cooldown"]),(0x30,"i",2)]:put(work+off,fmt,v)
            put(engine+0x23,"b",3);put(base[6]+0x1a0,"i",w["mode"])
            for off,key in [(0x190,"normal"),(0x98,"impulse")]:u.mem_write(work+off,struct.pack(">3f",*w[key]))
            slot=w["work_70"]
            angles=[0 if word else -7,123456,0 if word else -19]
            u.mem_write(tw+8,struct.pack(">3i",-7,123456,-19))
            ring=None
            if slot>=0:
                cursor=59 if j%2 else 8
                ring=dict(slot=slot,cursor=cursor,velocities=[[k,-k,2*k] for k in range(60)],rotations=[[k+7,k-9,k*3] for k in range(60)])
                cp=base[6]+0x222a4+4*slot
                vp=base[6]+0x21764+slot*720
                rp=base[6]+0x21d04+slot*720
                put(cp,"i",cursor)
                for k in range(60):
                    u.mem_write(vp+k*12,struct.pack(">3f",*ring["velocities"][k]))
                    u.mem_write(rp+k*12,struct.pack(">3i",*ring["rotations"][k]))
            tail=dict(observed_local_velocity=[-4,3,2],observed_raw_magnitude=11.5,post_alignment_angles=angles,work_d4=2147483645,ring=ring)
            q["tail"]=tail;put(work+0xd4,"i",tail["work_d4"])
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
        reg(3,task);run(0x2da8,0x200 if i>=7 else (0x2e00 if word else 0x2f5c))
        expected=dict(work_74=get(work+0x74,"i"),work_28c=get(work+0x28c,"i"),
                      velocity=vec(work+0x7c),position=vec(tw+20),calls=calls,
                      next="contact_2e00" if word else "no_contact_2f5c")
        if i>=7:
            expected["next"]="returned"
            hist=None
            if ring:
                ring["cursor"]=get(cp,"i")
                ring["velocities"]=[vec(vp+12*k) for k in range(60)]
                ring["rotations"]=[list(struct.unpack(">3i",u.mem_read(rp+12*k,12))) for k in range(60)]
                hist=[ring["velocities"][(ring["cursor"]-1-k)%60] for k in range(30)]
            expected["tail"]=dict(observed_local_velocity=vec(work+0x88),work_d4=get(work+0xd4,"i"),ring=ring,motion_history=hist)
            got=doc["response"]["wall"]
            wall=dict(wall_hit=any(isinstance(c,dict) and c.get("Engine8006b710")==[18,0,0,0] for c in calls),
                      impulse=vec(work+0x98),cooldown=get(work+0x290,"i"),vehicle_state=get(work+0x30,"i"),align_to_normal=bool(word))
            assert struct.pack(">3f",*wall["impulse"])==struct.pack(">3f",*got["impulse"])
            wall["impulse"]=got["impulse"];expected["wall"]=wall
        assert expected==doc["response"]
print("Matched thirteen movement contact and wall-response cases with supplied engine results.")
