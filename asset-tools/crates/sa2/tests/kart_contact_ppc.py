"""Compare original kart movement query, landing dispatch and response writes."""
import math,json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:4])
exe=Path(sys.argv[4]) if len(sys.argv)>4 else None
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
        if t!=1 or not (0x2ca4<=l<0x3024 or 0x5220<=l<0x52b0 or 0xc74<=l<0xd00 or 0x4a58<=l<0x4eb4 or 0x30dc<=l<0x3188) or k not in (4,6):continue
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
from kart_matrix_imports import install_inverse_rotation
install_inverse_rotation(u,base[1])
def vec(a):return list(struct.unpack(">3f",u.mem_read(a,12)))
def hook(m,addr,*_):
    pc=addr-base[1];args=[m.reg_read(getattr(R,f"UC_PPC_REG_{i}")) for i in range(3,7)]
    a,b,c,d=args
    if pc==0x4b50:
        if combined==2:assert get(work+0x30,"i")==result["impact"]["state"]
        return
    if pc==0x2d0c:
        assert a==work+0x118
        req=dict(from_=vec(a),to=vec(a+12),word_18=get(a+24,"I"),float_1c=get(a+28,"f"))
        req["from"]=req.pop("from_");assert req==doc["request"]
        if hit is None:m.emu_stop();return
        u.mem_write(a+32,struct.pack(">3f",*hit["resolved_position"]))
        if "returned_from" in hit:u.mem_write(a,struct.pack(">3f",*hit["returned_from"]))
        reg(3,hit["return_word"] & 0xffffffff)
    elif pc in (0x4afc,0x4b28,0x4b34):impact_calls.append(args if pc==0x4afc else a)
    elif pc in (0x4bf8,0x4c04):
        signals.append(a)
    elif pc==0x3134:
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",.25))[0])
    elif pc==0x2d50:
        scale=struct.unpack(">d",struct.pack(">Q",m.reg_read(R.UC_PPC_REG_FPR1)))[0]
        calls.append(dict(Effect10860=dict(position=vec(a),velocity=vec(b),scale=scale)))
    elif pc==0x2fac:
        assert (a,b,c)==(tw+8,work+0x7c,work+0x88)
        assert list(struct.unpack(">3i",u.mem_read(a,12)))==tail["post_alignment_angles"]
        if combined:return
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
for pc in (0x4b50,0x4afc,0x4b28,0x4b34,0x4bf8,0x4c04,0x3134,0x2d0c,0x2d50,0x2d70,0x2d84,0x2df0,0x2e90,0x2ea4,0x2ee8,0x2ef4,0x2f18,0x2f30,0x2f40,0x2f54,0x2f74,0x2fac,0x2fcc):
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    if exe:
        directory=Path(td)/"directory.json"
        subprocess.run([cli,"kart-crossing-directory",rel,directory,exe],check=True)
        route=json.loads(directory.read_text())["courses"][0]
    cases=[(None,0,0),(0,4,1),(0,2147483647,0),(1,30,1),(1,31,0),(-1,31,1),(3,-1,1)]+[(1,0,1)]*5+[(0,0,0)]
    runs=[(False,i) for i in range(len(cases))]
    if exe:runs.extend((True,i) for i in range(7,len(cases)))
    if exe:runs.extend((2,i) for i in range(7,len(cases)))
    for combined,i in runs:
        word,count,enabled=cases[i]
        hit=None if word is None else dict(return_word=word,resolved_position=[11,-3.125,19])
        q=dict(previous_position=[8,-2,30],proposed_position=[10,-4,20],velocity=[2,-2,-10],
               player_slot=1,work_28c=count,work_44=enabled,result=hit)
        if i>=7:
            j=i-7
            w=dict(normal=[1,[0,.125,0,.25,0,0][j],-1],impulse=[1,2,3],work_70=-1 if j==4 else j%2,
                   cooldown=5 if j==2 else 0,mode=j%2,vehicle_state=2,engine_byte_23=3)
            if combined==2:
                hit["resolved_position"][2]=34
                w["mode"]=[0,1,2,1,0,3][j]
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
        req=Path(td)/"q.json";out=Path(td)/f"{combined}-{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-contact-step",rel,out,req],check=True);doc=json.loads(out.read_text())
        for addr,key in [(work+20,"previous_position"),(tw+20,"proposed_position"),(work+0x7c,"velocity")]:
            u.mem_write(addr,struct.pack(">3f",*q[key]))
        put(work+0x28c,"i",count);put(work+0x44,"i",enabled);put(work+0x78,"i",1)
        if combined:
            # Same task and work addresses from first contact query to the final branch.
            del tail["observed_local_velocity"]
            saved=dict(task_mode=7,saved_angles=[12,13,14],grounded_position=[-6,7,8],
                       work_68=11,work_fc_104=[12,13,14],segment=3,lap=2,work_cc=1,
                       work_254=6,work_25c=8,timer=[1,180,181,182,181,5][j],work_260=2,
                       engine_halfword_64=(0 if j==3 else 2),angle_radians=.25)
            # Put the query's supplied contact below the course minimum in selected cases.
            height=min([100000]+[p["translation"][1] for p in route["projected_occurrence_poses"][1:]])
            if j:
                hit["returned_from"][1]=height-8;hit["resolved_position"][1]=height-10
            if combined==2:saved["work_68"]=[0,1,-1,3,0,-3][j]
            fields=dict(task_mode=(tw,"B"),saved_angles=(work+8,"3i"),grounded_position=(work+0x268,"3f"),
                work_68=(work+0x68,"i"),work_fc_104=(work+0xfc,"3i"),segment=(work+0xbc,"i"),lap=(work+0xc4,"i"),
                work_cc=(work+0xcc,"i"),work_254=(work+0x254,"i"),work_25c=(work+0x25c,"i"),timer=(work+0x294,"i"))
            for k,(addr,fmt) in fields.items():
                v=saved[k];u.mem_write(addr,struct.pack(">"+fmt,*(v if isinstance(v,list) else [v])))
            put(work+0x260,"i",2);put(engine+0x64,"h",saved["engine_halfword_64"])
            put(base[6]+0x24d68,"f",height);put(base[6]+0x24d80,"i",route["segment_count"])
            for k,quad in enumerate(route["projected_segment_xz"]):
                u.mem_write(base[6]+0x26d84+k*32,struct.pack(">8f",*(x for v in quad for x in v)))
            request=dict(contact=q,**saved)
            if combined==2:
                before=[7.5,7.500001,10,5,15,0][j]
                put(base[6]+0x21468+0x7c,"f",before);put(base[6]+0x21468+0x84,"f",0)
                put(work+0xb8,"i",77)
                request=dict(update=request,previous_velocity_xz=[before,0],work_b8=77)
            req.write_text(json.dumps(request));out=Path(td)/f"combined-{combined}-{i}.json"
            subprocess.run([cli,"kart-normal-contact" if combined==2 else "kart-contact-recovery",rel,out,exe,route["course"],req],check=True)
            result=json.loads(out.read_text());doc=result["contact"];signals=[];calls=[]
            impact_calls=[];reg(29,task)
            if combined==2:
                reg(30,work);run(0x23f8 if j%2==0 else 0x244c,0x2648)
                impact=result["impact"]
                assert get(work+0xb8,"i")==impact["work_b8"]
                assert impact_calls==[val for val in (impact["engine_8006b710"],impact["engine_80011714"]) if val is not None]
            else:run(0x2258,0x287c)
            assert struct.pack(">3f",*doc["response"]["tail"]["projected_local_velocity"])==bytes(u.mem_read(work+0x88,12))
            assert calls==doc["response"]["calls"]
            fields.update(position=(tw+20,"3f"),saved_position=(work+20,"3f"),velocity=(work+0x7c,"3f"),
                          angles=(tw+8,"3i"),state=(work+0x30,"i"))
            for k,(addr,fmt) in fields.items():
                val=result["recovery"]["vehicle"][k]
                assert bytes(u.mem_read(addr,struct.calcsize(">"+fmt)))==struct.pack(">"+fmt,*(val if isinstance(val,list) else [val])),(i,k,val)
            signal=result["recovery"]["engine_call_8001ef18"]
            assert signals==([] if signal is None else [signal])
            continue
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
if exe:print("Six shortened updates and six normal contact-to-recovery suffixes matched through the final branch.")
