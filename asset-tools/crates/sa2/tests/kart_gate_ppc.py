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
            # Identical one-tick clock, different order of admitted callbacks.
            check=dict(vehicle_countdown={})
            advance=dict(controller=dict(remove_task=False,pursuit_latch=0,race_finish_latch=0,engine_byte_09=0,engine_byte_42=0))
            credit=dict(placement=a,collision_flags=1,first_contact=primary,final_flags=0)
            for early in (True,False):
                timeline=[advance,credit,check] if early else [advance,check,check,credit,check]+[dict(timeout_tick=dict(engine_halfword_64=0))]*181
                q=dict(timer_word=1,primary_kart_counter=0,sequence=0,registered_tasks=registry,vehicle_state=0,updates=timeline)
                req.write_text(json.dumps(q));out=req.with_name(f"{path.stem}-{early}.json")
                subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
                result=json.loads(out.read_text())
                assert result["timerWord"]==int(rows[a][8])*60
                assert result["vehicleState"]==(0 if early else 10)
                assert result["timeoutTimer"]==(None if early else 181)
                if not early:
                    results=result["updates"]
                    assert results[1]["vehicleCountdown"]["cue_id"]==(0x706 if "Tails" in path.name else 0x70d)
                    assert results[2]["vehicleCountdown"]["timer"] is None
                    assert results[4]["vehicleCountdown"]["next"]=="vehicle_272c"
                    assert results[-2]["timeoutTick"]["action"] is None and results[-1]["timeoutTick"]["action"]==8
            # The target owns the catch latch. Do not inject it into the controller.
            signals=dict(remove_task=False,race_finish_latch=0,engine_byte_09=0,engine_byte_42=0)
            spawn=dict(target_spawn=dict(segment_xz=[[0,0],[10,10],[20,20],[30,30]],parent_task_y=40,latch=0))
            catch=dict(target_tick=dict(active_word=1,parent_work_x=80,parent_work_7c=2500))
            q.update(timer_word=2,controller_state=dict(elapsed=299,flag_3ce24=0,engine_byte_16=0),
                     updates=[spawn,dict(controller=signals),catch,dict(controller=signals),dict(controller=signals)])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-catch.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());assert doc["timerWord"]==1 and doc["target"]["latch"]==1
            assert doc["updates"][2]["target"]["cue_id"]==(0x701 if "Tails" in path.name else 0x708)
            first,second=[d["controller"] for d in doc["updates"][-2:]]
            assert first["elapsed"]==300 and ["engine_8005eb24",0] in first["calls"]
            assert second["elapsed"]==301 and ["engine_8005eb24",0] not in second["calls"]
            # Exactly 20 segments from the end does not spawn. One segment
            # closer allocates the target; the same handle suppresses a duplicate.
            allocation=dict(task_handle=0x1234,segment=75,segment_xz=[[0,0],[10,10],[20,20],[30,30]],latch=0)
            frame=dict(cooldown=0,work_288=0,work_108=0,engine_byte_23=0,engine_halfword_68=0,
                       global_24d60=0,segment=80,segment_count=100,position=[17,40,7],
                       previous_position=[6,5,4],velocity=[3,4,12],angles=[0,0,0],work_ec=0)
            q["updates"]=[dict(vehicle_post_move=frame),
                          dict(vehicle_post_move=dict(frame,segment=81,limousine_allocation=allocation)),
                          dict(vehicle_post_move=dict(frame,segment=81,work_ec=0x1234,velocity=[2500,4,12])),dict(target_tick={}),dict(controller=signals)]
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-route.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());before,spawned,existing=[v["vehiclePostMove"] for v in doc["updates"][:3]]
            assert "limousine" not in before and "limousine" not in existing
            assert spawned["frame"]["segment"]==75 and spawned["limousine"]["task_handle"]==doc["targetHandle"]==0x1234
            assert doc["target"]["latch"]==1 and doc["controllerState"]["elapsed"]==300 and doc["timerWord"]==2
            # The vehicle task pose and the vehicle work position differ here.
            # A latched tick follows the work position, not the shifted task X.
            q["updates"].append(dict(target_tick={}))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-follow.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            follow=json.loads(out.read_text())["target"]
            assert follow["position"][0]==6+100
            # Both the controller and vehicle consume the actual catch. A
            # finish flag takes priority and clamps the SAME credited counter.
            dispatch=dict(work_c8=12,work_44=0,work_68=98)
            q.update(primary_kart_counter=4)
            q["updates"] += [credit,dict(vehicle_post_move=dict(frame,work_ec=0x1234,dispatch=dispatch)),
                             dict(vehicle_post_move=dict(frame,engine_byte_23=1,engine_halfword_68=100,work_ec=0x1234,dispatch=dispatch))]
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-vehicle-catch.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());caught,finished=[v["vehiclePostMove"] for v in doc["updates"][-2:]]
            assert caught["vehicle_state"]==8 and caught["frame"]["dispatch"]["work_c4"]==5
            assert finished["frame"]["dispatch"]["work_c8"]==doc["primaryKartCounter"]==3
            # Vehicle writes the finish flag at the native engine threshold;
            # the controller must see that write without a second observation.
            ctx=dict(mode=int("Rouge" in path.name),first_slot_place=1,multiplayer_byte=0,engine_byte_23=0,
                     menu_active=False,engine_word_b4=0,time=[1,2,3],record=[2,0,0],
                     record_index=0,completion_index=0,check_return=None)
            finish_signals=dict(remove_task=False,engine_byte_09=0,engine_byte_42=0,race_results=ctx)
            post=dict(vehicle_post_move=dict(frame,engine_byte_23=1,engine_halfword_68=100))
            for already_caught in (0,1):
                q.update(updates=[dict(target_spawn=dict(spawn["target_spawn"],latch=already_caught)),post,dict(controller=finish_signals)])
                req.write_text(json.dumps(q));out=req.with_name(f"{path.stem}-finish-{already_caught}.json")
                subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
                doc=json.loads(out.read_text());r=doc["updates"][-1]["controller"]
                assert doc["finishFlag"]==1 and doc["vehicleState"]==10 and r["remaining"]==2 and r["elapsed"]==300
                if already_caught:assert "race_results" not in r and ["engine_8005eb24",0] in r["calls"]
                else:assert r["race_results"]["calls"]==[["engine_8001ef18",[8]]] and not r["race_results"]["writes"]
            # A later frame below threshold inherits the finish instead of
            # replaying the threshold write or clearing the controller input.
            later={k:v for k,v in frame.items() if k!="global_24d60"}
            q.update(updates=[post,dict(vehicle_post_move=later),dict(controller=dict(finish_signals,pursuit_latch=0))])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-retain-finish.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text())
            assert doc["updates"][1]["vehiclePostMove"]["frame"]["global_24d60"]==doc["finishFlag"]==1
            assert doc["updates"][2]["controller"]["race_results"]["calls"]==[["engine_8001ef18",[8]]]
            later["global_24d60"]=0
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-finish.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"vehicle finish flag conflicts" in rejected.stderr
            # Zero remaining time bypasses parent reads and the catch.
            q.update(timer_word=1,updates=[spawn,dict(controller=signals),dict(target_tick={})])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-inactive.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text())
            assert doc["target"]==doc["updates"][0]["target"]["state"] and not doc["updates"][-1]["target"]["installs_callbacks"]
            # The shared timing pass selects a split using the gate's actual
            # primary kart counter. Inhibiting lap updates does not pause countdown.
            timing=dict(inhibit=False,cumulative=True,first_highest=0)
            ctl=dict(controller=dict(signals,pursuit_latch=0),timing=timing)
            blank=[[0,0,0] for _ in range(3)]
            q.update(timer_word=400,primary_kart_counter=0,lap_clock=dict(ticks=59,display=[0,0,98],first=blank,second=blank),
                     updates=[ctl,credit,ctl,dict(ctl,timing=dict(timing,inhibit=True)),post,
                              dict(controller=dict(finish_signals,pursuit_latch=0),timing=timing)])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-lap.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());lap=doc["lapClock"]
            assert lap["ticks"]==61 and lap["first"]==[[0,1,0],[0,1,1],[0,0,0]]
            assert doc["timerWord"]==397+int(rows[a][8])*60
            assert doc["updates"][2]["controller"]["lap_clock"]==doc["updates"][3]["controller"]["lap_clock"]==lap
            # A gate credit reaches the next non-cumulative split via the
            # primary slot progress pass; no independent split index is given.
            first=dict(vehicle_post_move=dict(frame,segment=5))
            test=dict(slot_progress=dict(work_44=0,debug_enabled=False,highest=0,segment_tests=[False,True]))
            q.update(primary_kart_counter=0,updates=[first,credit,test,
                dict(slot_progress=dict(work_44=0,debug_enabled=False,segment_tests=[True])),
                dict(controller=dict(signals,pursuit_latch=0),timing=dict(inhibit=False,cumulative=False))])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-progress.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text())
            assert doc["primaryRoute"]==[8,100] and doc["highestCheckpoint"]==doc["primaryKartCounter"]==1
            assert doc["updates"][2]["slotProgress"]["tested_indices"]==[6,7]
            assert doc["lapClock"]["first"]==[[0,0,0],[0,1,0],[0,0,0]]
            continuation={k:v for k,v in frame.items() if k not in ("segment","segment_count")}
            continuation["dispatch"]=dict(work_44=0,work_68=98,pursuit_latch=0)
            q["updates"].append(dict(vehicle_post_move=continuation))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-progress-dispatch.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            dispatched=json.loads(out.read_text())["updates"][-1]["vehiclePostMove"]["frame"]
            assert (dispatched["segment"],dispatched["segment_count"])==(8,100)
            assert dispatched["dispatch"]["work_c8"]==1
            continuation["dispatch"]["work_c8"]=0
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-highest.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"vehicle highest checkpoint conflicts" in rejected.stderr
            del continuation["dispatch"]["work_c8"]
            continuation["segment"]=5
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-dispatch.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"vehicle route conflicts" in rejected.stderr
            stationary=dict(vehicle_post_move=dict(frame,segment=5,position=[6,999,4],previous_position=[6,5,4]))
            probe=dict(work_44=1,debug_enabled=True,highest=0)
            q.update(updates=[stationary,dict(slot_progress=probe)])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stationary.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());step=doc["updates"][1]["slotProgress"]
            assert step["tested_indices"]==[6,7,8,9,10,11,12,13,14,4,3]
            assert step["debug_indices"]==[6,7,5,4,3] and step["piece"]==5
            assert doc["primaryKartCounter"]==0
            for label,move,extra in [("false-hit",stationary,dict(segment_tests=[True])),("missing-hit",first,{})]:
                q.update(updates=[move,dict(slot_progress=dict(probe,**extra))])
                req.write_text(json.dumps(q));out=req.with_name(path.stem+"-"+label+".json")
                rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
                assert rejected.returncode!=0 and not out.exists()
            # State 7 clears work+0x44. The stationary progress scan must now
            # use three forward probes and skip the reverse/debug branches.
            guided=dict(stationary["vehicle_post_move"],global_24d60=1,velocity=[0,0,0],
                        dispatch=dict(work_c8=0,work_44=1,work_68=98,target=dict(
                            segment=7,quad=[[1,2],[37,45],[5,6],[7,8]],angle_return=0.0)))
            scan=dict(debug_enabled=True)
            q.update(updates=[dict(vehicle_post_move=guided),dict(slot_progress=scan)])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-guided-scan.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());step=doc["updates"][1]["slotProgress"]
            assert doc["vehicleState"]==7 and step["tested_indices"]==[6,7,8] and not step["debug_indices"]
            scan["work_44"]=1
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-scan.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"course scan flag conflicts" in rejected.stderr
            # A new observed effect can replace the two effect words while
            # the independent cooldown still inherits its previous value.
            initial=dict(frame,cooldown=3,work_288=2,work_108=17)
            tick={k:v for k,v in frame.items() if k not in ("cooldown","work_288","work_108")}
            q.update(updates=[dict(vehicle_post_move=f) for f in
                     (initial,tick,dict(tick,work_288=3,work_108=9),tick)])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-timer-continuity.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            frames=[v["vehiclePostMove"]["frame"] for v in json.loads(out.read_text())["updates"]]
            assert [[f[k] for k in ("cooldown","work_288","work_108")] for f in frames]==[
                [2,1,17],[1,0,0],[0,2,9],[0,1,9]]
            indices=[i for i,r in enumerate(rows) if r[0]&4095==9]
            pickups=[dict(balloon_reward=dict(placement=i,contact_task=primary,driver=0,work_44=1,clock=1)) for i in indices]
            q.update(primary_charge=4,updates=pickups)
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-balloon-charge.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());charge=4
            for i,event in zip(indices,doc["updates"],strict=True):
                amount=[5,10,20][rows[i][1]-1];charge+=amount
                assert event["balloonReward"]==dict(placement=i,charge=charge,display=[10,amount])
            assert doc["primaryCharge"]==charge==(54 if "Rouge" in path.name else 94)
            # Feed the same earned charge into readiness, a later press, and
            # the following active frame. Only initialize duration/readiness once.
            boost_inputs=dict(work_70=0,piece_id=0,position=[1,2,3],work_44=1)
            def boost(**changes):
                return dict(boost_tick=dict(velocity=[0,0,0],
                            boost=dict(boost_inputs,**changes)))
            q.update(primary_charge=0,updates=[pickups[0],boost(work_dc=0,slot_flag=0,buttons_pressed=0),
                boost(buttons_pressed=0x200,normalized_velocity=[0.6,0.8,0]),boost()])
            driving=dict(audio_work_284=0,buttons=4,work_28c=0,heading=17,work_a4=2,stick_x=0,steering_sum=0)
            driving["contact"]=dict(previous_position=[7,4,5],result=dict(return_word=1,resolved_position=[8,5,7]))
            driving["position_step"]=dict(work_74=0,transformed_vector=[-1.25,0.75,2])
            for event in q["updates"][1:]:
                event["boost_tick"]["drive"]=dict(driving)
            q["updates"][2]["boost_tick"]["drive"]["magnitude_return"]=12.0
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-earned-boost.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text())
            ready,start,active=[u["boostTick"]["boost"] for u in doc["updates"][1:]]
            assert doc["updates"][0]["balloonReward"]["charge"]==20 and doc["primaryCharge"]==0
            assert (ready["slot_flag"],ready["work_dc"],ready["work_d8"])==(1,0,0)
            assert (start["slot_flag"],start["work_dc"],active["work_dc"])==(0,120,119)
            assert ready["speed_cap"]==(12 if "Rouge" in path.name else 10)
            assert active["acceleration"]>ready["acceleration"] and active["calls"]
            normal,activated,powered=[u["boostTick"]["drive"] for u in doc["updates"][1:]]
            assert normal["magnitude_source"]==powered["magnitude_source"]=="native_zero"
            assert activated["magnitude_source"]=="observed"
            assert powered["heading"]==17 and powered["work_a4"]==1
            assert abs(powered["local_displacement"][2])>abs(normal["local_displacement"][2])
            for event,vy in zip(doc["updates"][1:],(0,16,0),strict=True):
                profile=event["boostTick"];pos=profile["drive"]["position_step"]
                assert pos["transform_source"]=="observed" and pos["world_displacement"]==[-1.25,0.75,2]
                assert pos["position"][::2]==[-0.25,5]
                assert abs(pos["position"][1]-(2.75+vy-profile["weight"]))<0.000002
                collision=profile["drive"]["contact"]
                assert collision["request"]["from"]==[7,4,5] and collision["request"]["to"]==pos["position"]
                assert collision["response"]["position"]==[8,5,7] and collision["response"]["velocity"]==[1,1,2]
                calls=profile["drive"]["audio"]["calls"]
                assert calls[1]["play"]["cue"]==profile["cue_words"][0]
                assert calls[1]["play"]["position"]==collision["request"]["to"]
                assert calls[-1]["pitch"]["value"]==(3072 if vy else 0)
            # A supplied below-course result reaches impact and recovery in
            # their dispatch order, without a second contact or position input.
            last=q["updates"][-1]["boost_tick"]["drive"]
            last["contact"]=dict(previous_position=[7,-100000,5],
                result=dict(return_word=0,resolved_position=[7,-100000,5]),
                wall=dict(normal=[0,1,0],impulse=[0,0,0],work_70=0,cooldown=0,mode=int("Rouge" in path.name),
                          vehicle_state=0,engine_byte_23=0),
                tail=dict(post_alignment_angles=[0,0,0],observed_raw_magnitude=0,work_d4=0,observed_local_velocity=[0,0,0],
                          ring=dict(slot=0,cursor=0,velocities=[[1,2,3]]*60,rotations=[[0,0,0]]*60)),
                recovery=dict(previous_velocity_xz=[20,0],work_b8=7,task_mode=0,saved_angles=[0,0,0],
                    grounded_position=[0,0,0],work_68=0,work_fc_104=[0,0,0],segment=0,lap=0,
                    work_cc=0,work_254=0,work_25c=0,timer=0,work_260=0,engine_halfword_64=0))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-boost-fall.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());suffix=doc["updates"][-1]["boostTick"]["drive"]["contact"]
            assert suffix["impact"]["state"]==4 and suffix["recovery"]["engine_call_8001ef18"]==8
            assert suffix["recovery"]["vehicle"]["state"]==doc["vehicleState"]==10
            observed=last["contact"]["recovery"].pop("previous_velocity_xz")
            origin=last["contact"].pop("previous_position")
            q["updates"].insert(-1,dict(primary_update_velocity=[0,0,0],primary_update_position=origin))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-saved-speed.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            saved=json.loads(out.read_text())["updates"][-1]["boostTick"]["drive"]["contact"]
            assert saved["impact"]["state"]==0 and saved["recovery"]["vehicle"]["state"]==10
            assert saved["request"]["from"]==origin
            last["contact"]["previous_position"]=[1,2,3]
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-contact-origin.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"contact origin conflicts" in rejected.stderr
            last["contact"]["previous_position"]=origin
            fields=last["contact"]["recovery"]
            initial_angles=fields.pop("saved_angles")
            q["updates"][-2]["primary_update_angles"]=[-12345,65537,23456]
            fields.update(timer=2,engine_halfword_64=2)
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-restore-snapshot.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            recovered=json.loads(out.read_text())["updates"][-1]["boostTick"]["drive"]["contact"]["recovery"]["vehicle"]
            assert recovered["position"]==origin and recovered["angles"]==[-12345,65537,23456]
            fields["saved_angles"]=initial_angles
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-saved-angles.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"recovery angles conflict" in rejected.stderr
            q["updates"].pop(-2)
            last["contact"]["recovery"]["previous_velocity_xz"]=observed
            # A later recovery tick rewinds the course. Its two saved poses
            # differ only vertically, so the next scan needs no injected hits.
            recovery=last["contact"]["recovery"]
            recovery.update(segment=3,timer=181,engine_halfword_64=2)
            del recovery["lap"]
            q["updates"].append(dict(slot_progress=dict(work_44=1,debug_enabled=False,highest=0)))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-recovered-scan.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());r=doc["updates"][-2]["boostTick"]["drive"]["contact"]["recovery"]
            assert r["engine_call_8001ef18"]==4 and r["vehicle"]["segment"]==doc["primaryRoute"][0]==1
            assert doc["primaryRoute"][1]>100 and doc["updates"][-1]["slotProgress"]["tested_indices"][0]==2
            after={k:v for k,v in frame.items() if k not in
                   ("position","previous_position","velocity","angles","segment","segment_count")}
            q["updates"].append(dict(vehicle_post_move=after))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-recovered-vehicle.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            carried=json.loads(out.read_text())["updates"][-1]["vehiclePostMove"]["frame"]
            for name,source in [("position","position"),("previous_position","saved_position"),
                                ("velocity","velocity"),("angles","angles")]:
                assert carried[name]==r["vehicle"][source]
            after["previous_position"]=[7,-100000,5]
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-recovery-pose.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"vehicle pose conflicts" in rejected.stderr
            # Start the next full update from the post-move frame. The saved
            # origin is the live task position, not the prior saved position.
            del after["previous_position"]
            next_boost=dict(work_70=0,work_44=1)
            resume=dict(boost_tick=dict(boost=next_boost))
            q["updates"].extend([dict(continue_vehicle_update={}),resume])
            out=req.with_name(path.stem+"-contiguous-update.json")
            speed=sum(x*x for x in carried["velocity"])**0.5
            if speed:resume["boost_tick"]["normalize"]=dict(magnitude=speed,vector=[x/speed for x in carried["velocity"]])
            req.write_text(json.dumps(q))
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            seq=json.loads(out.read_text())["updates"]
            assert seq[-2]["savedPosition"]==carried["position"]!=carried["previous_position"]
            assert seq[-2]["savedAngles"]==carried["angles"] and seq[-1]["boostTick"]["velocity"]==carried["velocity"]
            next_boost["position"]=[999,999,999]
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-conflicting-continuation.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"preceding vehicle frame" in rejected.stderr
            # Native history writes at the cursor but reads starting at the
            # NEXT cursor. Work across wrap and replace a contradictory sum.
            initial=[0]*60;initial[0]=100
            sampled=boost(work_dc=0,slot_flag=0)
            sampled["boost_tick"]["drive"]=dict(buttons=4,work_28c=0,heading=17,work_a4=2)
            q.update(primary_charge=0,controller_history=dict(history=initial,cursor=59),
                     updates=[dict(primary_stick_x=-25),sampled,dict(primary_stick_x=7),sampled])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-steering-history.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());steps=doc["updates"]
            assert [steps[0]["steeringSum"],steps[2]["steeringSum"]]==[75,-18]
            assert doc["controllerHistory"]["cursor"]==1 and doc["controllerHistory"]["history"][59]==-25
            assert steps[1]["boostTick"]["drive"]["heading"]!=steps[3]["boostTick"]["drive"]["heading"]
            sampled["boost_tick"]["drive"]["steering_sum"]=0
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-steering.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"drive steering conflicts" in rejected.stderr
            # Holding boost without a press leaves it ready; the press word
            # then activates it while the held brake selects the braking branch.
            go=boost(work_dc=0,slot_flag=0)
            stop=boost(normalized_velocity=[1,0,0])
            for event in (go,stop):
                event["boost_tick"]["drive"]=dict(work_28c=0,heading=0,work_a4=2,magnitude_return=20)
            q.update(primary_charge=20,updates=[dict(primary_stick_x=0,buttons_held=0x204,buttons_pressed=0),go,
                dict(primary_stick_x=0,buttons_held=0x202,buttons_pressed=0x200),stop])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-controller-buttons.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());first,last=[doc["updates"][i]["boostTick"] for i in (1,3)]
            assert (first["boost"]["slot_flag"],first["boost"]["work_dc"],first["drive"]["work_a4"])==(1,0,1)
            assert (last["boost"]["slot_flag"],last["boost"]["work_dc"],last["drive"]["work_a4"])==(0,120,10)
            stop["boost_tick"]["boost"]["buttons_pressed"]=0
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-stale-press.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"sampled press word" in rejected.stderr
            for stick,expected in [(20,0),(-21,2)]:
                entry=boost(work_dc=0,slot_flag=0,normalized_velocity=[1,0,0])
                entry["boost_tick"]["drive"]=dict(work_28c=0,heading=0,work_a4=2,magnitude_return=20,
                    drift=dict(motion_history=[[1,0,2]]*30,local_offset=[0,0,0],work_10c=4,work_a8=9),
                    audio_work_284=0,position_step=dict(work_74=0,transformed_vector=[0,0,0]))
                q.update(primary_charge=20,updates=[dict(primary_stick_x=stick,buttons_held=4,buttons_pressed=0x200),entry])
                req.write_text(json.dumps(q));out=req.with_name(path.stem+f"-drift-{stick}.json")
                subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
                doc=json.loads(out.read_text());drive=doc["updates"][1]["boostTick"]["drive"];drift_result=drive["drift"]
                assert drift_result["vehicle_state"]==doc["vehicleState"]==expected and drift_result["work_10c"]==5
                assert (drift_result["work_a4"],drift_result["work_a8"])==((0,-21) if expected==2 else (1,9))
                assert ("play" in drive["audio"]["calls"][-1])==(expected==2)
            winding=dict(drift_tick=dict(heading=0,position=[1,2,3],velocity=[0,0,0],work_74=0))
            for _ in range(5):
                q["updates"].extend([dict(primary_stick_x=99,buttons_held=4),winding])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-windup.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());wind=[u["driftTick"] for u in doc["updates"] if "driftTick" in u]
            assert [v["work_a4"] for v in wind]==[1,2,3,4,0]
            assert [v["vehicle_state"] for v in wind]==[2,2,2,2,3] and doc["vehicleState"]==3
            assert wind[-1]["work_68"]>0 and wind[-1]["math_source"]=="mathematical_projection"
            moving=dict(winding["drift_tick"],velocity=[0,0,20],work_fc=5)
            q["updates"].extend([dict(primary_stick_x=10,buttons_held=4),dict(drift_tick=moving),
                                 dict(primary_stick_x=0,buttons_held=0),winding])
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-sustained-exit.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            doc=json.loads(out.read_text());active,exit_tick=[doc["updates"][i]["driftTick"] for i in (-3,-1)]
            assert (active["vehicle_state"],active["work_fc"])==(3,96)
            assert (exit_tick["vehicle_state"],exit_tick["work_fc"],doc["vehicleState"])==(0,187,0)
            # Run contact on a sustained tick, before the separate exit tick.
            # The native request ends at the drift task position; effects use
            # world velocity, distinct from the rotated local drift vector.
            moving["contact"]=dict(work_28c=31,work_44=1,query=dict(previous_position=[9,4,3],
                result=dict(return_word=1,resolved_position=[8,5,7])))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-drift-contact.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            event=json.loads(out.read_text())["updates"][-3];collision=event["driftContact"]
            assert collision["request"]["to"]==event["driftTick"]["position"] and collision["request"]["from"]==[9,4,3]
            assert collision["response"]["velocity"]==[-1,1,4]
            assert collision["response"]["calls"][0]["Effect10860"]["velocity"]==[0,0,20]
            # Full vehicle updates clear the flag word before sampling. The
            # dispatch selected on entry sets bit 2 even when this tick exits.
            del entry["boost_tick"]["drive"]["drift"]["work_10c"]
            q["updates"].insert(0,dict(primary_update_velocity=[0,0,0]))
            q["updates"].insert(-1,dict(primary_update_velocity=[0,0,0]))
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-frame-flags.json")
            subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],check=True)
            seq=json.loads(out.read_text())["updates"]
            assert seq[0]["work_10c"]==0 and seq[2]["boostTick"]["drive"]["drift"]["work_10c"]==1
            assert seq[-1]["work_10c"]==4 and seq[-1]["driftTick"]["vehicle_state"]==0
            q["updates"].pop(0)
            entry["boost_tick"]["drive"]["drift"]["work_10c"]=4
            q["updates"]=q["updates"][:2]
            q["vehicle_state"]=1
            req.write_text(json.dumps(q));out=req.with_name(path.stem+"-wrong-drift-routine.json")
            rejected=subprocess.run([cli,"kart-story-gate-sequence",rel,out,exe,path,req],capture_output=True)
            assert rejected.returncode!=0 and b"normal driving state" in rejected.stderr
    print("Matched 24 gate and 24 interleaved countdown updates across four installed story layouts.")
