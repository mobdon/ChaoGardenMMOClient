"""Compare the vehicle expiry branch, original task spawn, and scheduled timeout update."""
import json,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

cli,rel,tmp=map(Path,sys.argv[1:])
d=rel.read_bytes();u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x86000000,5:0x82000000,6:0x83000000};engine=0x84000000
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
        if t!=1 or not (0x2648<=l<0x2814 or 0xd968<=l<0xda80 or 0x9814<=l<0x9948) or k not in (4,6):continue
        if mod==80:p=base[s]+a
        elif a==0x801cbd88:p=engine
        else:continue
        u.mem_write(base[1]+l,struct.pack(">H",(p if k==4 else (p+32768)>>16)&65535))
def put(p,fmt,v):u.mem_write(p,struct.pack(">"+fmt,v))
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))[0]
ram=0x85000000;u.mem_map(ram,0x10000)
task,tw,vehicle=ram+256,ram+512,ram+1024
put(task+0x34,"I",tw)
stops={0x272c:"vehicle_272c",0x2814:"vehicle_2814",0x200:"return"}
def hook(m,addr,*args):
    pc=addr-base[1]
    if pc==0x272c and op=="vehicle_post_move":return
    if pc==0x27a4:
        assert m.reg_read(R.UC_PPC_REG_3)==vehicle+0x7c
        assert struct.unpack(">3f",u.mem_read(vehicle+0x7c,12))==(3.,4.,12.)
        m.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",13.))[0])
        m.reg_write(R.UC_PPC_REG_PC,addr+4);return
    if pc==0xd99c:
        assert [m.reg_read(getattr(R,f"UC_PPC_REG_{j}")) for j in range(3,7)]==[2,2,base[1]+0xda80,base[5]+0x2ee178]
        m.reg_write(R.UC_PPC_REG_3,child)
        m.reg_write(R.UC_PPC_REG_PC,addr+4);return
    if pc==0x280c:
        if alloc:return
        assert m.reg_read(R.UC_PPC_REG_3)==task
        m.reg_write(R.UC_PPC_REG_3,65432)
        m.reg_write(R.UC_PPC_REG_PC,addr+4);return
    if pc in stops:m.emu_stop();return
    arg=m.reg_read(R.UC_PPC_REG_3)
    if pc==0x98ac:
        assert arg==2 and m.reg_read(R.UC_PPC_REG_4)==2 and m.reg_read(R.UC_PPC_REG_5)==base[1]+0x9814
        events.append(("spawn",None));m.reg_write(R.UC_PPC_REG_3,task)
    else:events.append(("cue" if pc in (0x9908,0x9914) else "action",arg))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in [*stops,0x27a4,0x280c,0xd99c,0x98ac,0x9908,0x9914,0x985c,0x9868]:
    u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    cases=[dict(operation="vehicle_countdown",remaining=n,vehicle_state=v,mode=m)
           for n,v,m in [(1,2,0),(0,2,0),(-1,2,1),(0,10,1),(0,0,4)]]
    cases += [dict(operation="spawn",remaining=1,mode=0)]
    cases += [dict(operation="tick",timer=t,engine_halfword_64=h)
              for t,h in [(119,1),(179,0),(180,0),(181,-1),(180,1),(65535,1)]]
    for j,(mode,byte,half,flag,remaining,segment,handle) in enumerate([
        (0,1,99,0,1,80,0),(0,1,99,0,1,81,0),(1,0,0,0,1,81,0),
        (1,0,0,0,1,81,123),(2,1,100,0,0,81,0),(0,1,100,0,0,81,0),
        (1,1,100,1,0,81,0),(1,-1,0,0,0,80,0)]):
        cases.append(dict(operation="vehicle_post_move",remaining=remaining,vehicle_state=2,mode=mode,
            frame=dict(cooldown=[180,1,0,-1,-2147483648,6,0,7][j],work_288=[1,0,-1,2,5,1,0,1][j],
                work_108=17,engine_byte_23=byte,engine_halfword_68=half,global_24d60=flag,segment=segment,
                segment_count=100,position=[17,8,7],previous_position=[6,5,4],velocity=[3,4,12],
                angles=[-9,55,88],work_ec=handle,magnitude_return=13,d968_return=65432)))
    child,child_tw=ram+0x4000,ram+0x5000
    for mode in (0,1):
        q=json.loads(json.dumps(cases[14]));q["mode"]=mode
        del q["frame"]["d968_return"]
        q["frame"]["limousine_allocation"]=dict(task_handle=child,segment=75,
            segment_xz=[[1,2],[3,4],[-32.5,95.25],[7,8]],latch=mode)
        cases.append(q)
    for i,q in enumerate(cases):
        alloc=q.get("frame",{}).get("limousine_allocation")
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json";req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-timeout",rel,out,req],check=True);doc=json.loads(out.read_text())
        events=[];op=q["operation"];u.reg_write(R.UC_PPC_REG_1,ram+0xf000)
        u.reg_write(R.UC_PPC_REG_LR,base[1]+0x200);u.reg_write(R.UC_PPC_REG_30,vehicle)
        u.reg_write(R.UC_PPC_REG_3,task);put(tw+6,"H",q.get("timer",333))
        put(vehicle+0x30,"i",q.get("vehicle_state",-99))
        put(base[6]+0x36d94,"i",q.get("remaining",0));put(base[6]+0x1a0,"i",q.get("mode",0))
        put(engine+0x64,"h",q.get("engine_halfword_64",0))
        if op=="vehicle_post_move":
            f=q["frame"]; pose=ram+0x2000
            u.reg_write(R.UC_PPC_REG_MSR,8192)
            u.reg_write(R.UC_PPC_REG_29,task);u.reg_write(R.UC_PPC_REG_31,pose)
            if alloc:
                put(task+0x34,"I",pose);put(task+0x38,"I",vehicle);put(child+0x34,"I",child_tw)
                put(base[6]+0x3ce2c,"i",alloc["latch"])
                for idx,pair in enumerate(alloc["segment_xz"]):
                    u.mem_write(base[6]+0x26d84+32*alloc["segment"]+8*idx,struct.pack(">2f",*pair))
            mapping=dict(cooldown=(vehicle+0x290,"i"),work_288=(vehicle+0x288,"i"),work_108=(vehicle+0x108,"i"),
                engine_byte_23=(engine+0x23,"b"),engine_halfword_68=(engine+0x68,"h"),
                global_24d60=(base[6]+0x24d60,"i"),segment=(vehicle+0xbc,"i"),segment_count=(base[6]+0x24d80,"i"),
                position=(pose+20,"3f"),previous_position=(vehicle+20,"3f"),velocity=(vehicle+0x7c,"3f"),
                angles=(pose+8,"3i"),work_ec=(vehicle+0xec,"I"))
            for key,(addr,fmt) in mapping.items():
                v=f[key];u.mem_write(addr,struct.pack(">"+fmt,*(v if isinstance(v,list) else [v])))
        entry={"vehicle_post_move":0x2648,"vehicle_countdown":0x26fc,"spawn":0x9884,"tick":0x9814}[op]
        u.emu_start(base[1]+entry,base[1]+0x9990,count=200)
        assert doc["next"]==stops[u.reg_read(R.UC_PPC_REG_PC)-base[1]]
        assert doc["vehicle_state"]==(get(vehicle+0x30,"i") if op in ("vehicle_countdown","vehicle_post_move") else None)
        if op=="vehicle_post_move":
            for key,(addr,fmt) in mapping.items():
                v=struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
                assert doc["frame"][key]==(list(v) if len(v)>1 else v[0]),(i,key,v,doc)
        if alloc:
            limo=doc["limousine"];state=limo["target"]["state"]
            assert limo["task_handle"]==get(vehicle+0xec,"I")==child
            assert [limo[k]+base[1] for k in ("display_offset","delete_offset")]==[get(child+j,"I") for j in (20,24)]
            assert limo["exec_offset"]==0xda80
            assert [get(child+j,"I") for j in (0x38,0x40)]==[pose,vehicle]
            assert state["position"]==list(struct.unpack(">3f",u.mem_read(child_tw+20,12)))
            assert state["lead"]==get(child_tw+32,"f") and state["latch"]==get(base[6]+0x3ce2c,"i")
        else:assert "limousine" not in doc
        spawned=("spawn",None) in events
        assert doc["timer"]==(get(tw+6,"H") if spawned or op=="tick" else None)
        assert [v for k,v in events if k=="cue"]==([] if doc["cue_id"] is None else [doc["cue_id"]])
        assert [v for k,v in events if k=="action"]==([] if doc["action"] is None else [doc["action"]])
print("Matched 22 vehicle and limousine initialization paths, including vehicle post-movement, route boundary, timeout and delayed-action paths.")
