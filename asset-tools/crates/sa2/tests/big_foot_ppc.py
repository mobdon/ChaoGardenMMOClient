"""Compare the mounted Big Foot stage opening with original installed PPC instructions."""
import hashlib,json,math,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

game=Path(os.environ["SA2_GAME_PATH"])
d=(game/"resource/gd_PC/boss_bigfootD.rel").read_bytes()
assert hashlib.sha256(d).hexdigest()=="fc92758679726f1f6bf2d816c878e79f44ad0adce2f9b32dd9ea549f0fea0f9d"
w=lambda i:struct.unpack_from(">I",d,i)[0]
sec=lambda s:w(w(16)+8*s)&~1
B,D,T=0x81000000,0x82000000,0x100000
STAGE,BOSS,WORK,EXTRA,PRIVATE,CCL,PLAYER=[T+i*0x200 for i in range(7)]
ret=B+0xb000
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
mapped=set()
def page(p):
    if p>>12 not in mapped:u.mem_map(p&~4095,4096);mapped.add(p>>12)
def get(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def reg(i):return u.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
def setreg(i,v):u.reg_write(getattr(R,"UC_PPC_REG_"+str(i)),v)
ranges=[(0x1bcc,0x1e50),(0x3c8,0x6b8),(0x19b0,0x19c0),(0x1a80,0x1b18),(0x163c,0x19b4),(0x63a0,0x63a8),(0x306c,0x3074),(0x30b8,0x30c4),(0x36ac,0x36c0),(0x36e8,0x3700),(0x3eb8,0x3eec),(0x4298,0x42ac),(0x45b8,0x45ec),(0,0x35c),(0x557c,0x5810),(0x570ac,0x57104),(0x57178,0x571a0),(0x58948,0x589b0),(0x810,0xa58),(0xd90,0x1628),(0xb64,0xbbc),(0x5470c,0x547fc),(0x63a8,0x6434),(0x66bc,0x67f4)]
for lo,hi in ranges:
    for pc in range(lo,hi,4):page(B+pc)
    u.mem_write(B+lo,d[sec(1)+lo:sec(1)+hi])
page(T);page(T+0x2000);page(ret)
player_ptr=0
defeat_targets={}
bind_relocs=[]
for module,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;off=0;continue
        off+=inc
        if target==5 and 528<=off<748 and k==1:
            dst=D+5*0x400000+off;page(dst);bind_relocs.append((dst,D+s*0x400000+a))
            if off%20==528%20: # copy each binding's trailing XYZ floats
                u.mem_write(dst+8,d[sec(5)+off+8:sec(5)+off+20])
        if target!=1 or not any(lo<=off<hi for lo,hi in ranges):continue
        if target==1 and off in (0x3ed0,0x3ed8,0x45d0,0x45d8):
            assert module==0 and k==10
            defeat_targets[off]=a
        if k==10:continue # observed at the external call site
        val=(B+a if s==1 else D+s*0x400000+a) if module else a;page(val)
        if module and s in (4,5):u.mem_write(val,d[sec(s)+a:sec(s)+a+16])
        if off==0x673a:player_ptr=val
        assert k in (4,6)
        put(B+off,"H",(val if k==4 else (val+0x8000)>>16)&65535)
for dst,val in bind_relocs:put(dst,"I",val)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|8192)
def hook(uc,pc,size,_):
    off=pc-B
    if off in (0x10,0xda0,0x1024):saved[:]=[reg(i) for i in range(26,32)]
    if off in (0x250,0x1004,0x1618):
        for i,v in enumerate(saved,26):setreg(i,v)
    if off in (0x1e20,0x1e2c):
        assert reg(3)==BOSS
        if off==0x1e2c:assert reg(4)==0
        turn_helpers.append(off)
    if off==0x414:setreg(3,0x1234)
    if off==0x41c:
        assert reg(3)==0x1234 and reg(4)==old_yaw
        setreg(3,delta&0xffffffff)
    if off in (0x48c,0x4c4,0x1a84,0x1aa8,0x1d8c,0x1db0):
        if off in (0x48c,0x4c4):assert reg(3)==((-2*delta)&0xffffffff)
        else:assert reg(3)==old_yaw
        value={0x1a84:yaw_sin,0x1aa8:yaw_cos,0x1d8c:yaw_sin,0x1db0:yaw_cos}.get(off,turn_sin)
        u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",value))[0])
    if off in (0x1764,0x18d8,0x1990):flight_calls.append(off)
    if off==0x34:
        assert (reg(3),reg(4),u.mem_read(reg(6),16))==(2,2,b"BossBigFootExec\0")
        result["executor"]=reg(5)-B;setreg(3,BOSS)
    if off==0x44:setreg(3,EXTRA)
    if off==0x54:
        assert (reg(3),reg(4))==(1,200);setreg(3,PRIVATE if ok else 0)
    if off==0x68:result["freed"]=reg(3)==BOSS
    if off in (0x94,0x134):setreg(3,0)
    if off==0x180:result["collision"]=(reg(4)-(D+5*0x400000),reg(5),reg(6))
    if off==0x338:result["animationRequest"]=reg(4)
    if off==0xe64:
        arg=reg(4);intro_calls.append({"openingView":{"slot":reg(3),"args":[get(arg,"f")[0],get(arg+4,"i")[0],*get(arg+12,"ff"),get(arg+20,"I")[0]],"focus":list(get(get(arg+8,"I")[0],"fff"))}})
    if off==0xea0:
        intro_calls.append({"setPlayerPosition":{"slot":0,"position":list(get(PLAYER+20,"fff"))}})
        intro_calls.append({"setMotionLinkSync":reg(4)})
    if off in (0xeb8,0xeec,0xf08,0xf38,0xfb0,0xfcc):
        intro_calls.append({0xeb8:"createHelper53da8",0xeec:"createHelper59bc4",0xf08:"setup54b74",0xf38:"endOpeningView",0xfb0:"engineCall%08x"%2147626012,0xfcc:"freeHelper59bc4"}[off])
        if off==0xeec:setreg(3,T+0xd00 if created else 0)
    if off==0xfa8:
        arg=reg(4);intro_calls.append({"nextView":{"slot":reg(3),"focus":list(get(get(arg,"I")[0],"fff")),"args":[*get(arg+4,"ffI"),*get(arg+16,"BB"),*get(arg+20,"ffI")]}})
    if off==0x54764:setreg(3,query & 0xffffffff)
    if off==0x547dc:intro_calls.append({"local2e4b4":{"position":list(get(reg(3),"fff")),"slot":reg(5),"argument":reg(6),"kind":reg(7)}})
    if off in (0x1134,0x1248,0x12c0,0x145c):
        assert ready is not None;setreg(3,int(ready));motion_queries.append(off)
    if off==0x10b8:take_calls.append({"enableCollision":reg(4)})
    if off in (0x10c0,0x1150):take_calls.append({0x10c0:"helper58c78",0x1150:"helper58d30"}[off])
    if off in (0x10d8,0x115c,0x1268,0x12e0,0x149c):take_calls.append({"motion":reg(4)})
    if off in (0x112c,0x1234,0x12fc,0x14b4):take_calls.append({"helper6b8":list(get(reg(4),"fff")) if reg(4) else None})
    if off==0x12f0:take_calls.append({"turnArgs":[reg(4),reg(5)]})
    if off in (0x11a0,0x15a0,0x15c4):
        take_calls.append({"effectDescriptor":680 if off==0x11a0 else 704})
        if off==0x15c4:take_calls[-1].update(byte0=get(reg(4),"B")[0],float4=get(reg(4)+4,"f")[0])
    if off in (0x8d0,0x8dc,0x8e8,0x910,0x91c):
        dst=reg(3);a=get(dst,"fff");b=get(reg(4),"fff")
        put(dst,"fff",*(x+y for x,y in zip(a,b,strict=True)))
    if off in (0x8f8,0x92c):
        assert reg(3)==BOSS and reg(4)==len(centers)
        centers.append(list(get(reg(5),"fff")))
    if off in (0x978,0x9b8,0x9f0):
        j=(0x978,0x9b8,0x9f0).index(off);assert reg(3)==(71,83,97)[j]
        u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack(">Q",struct.pack(">d",sines[j]))[0])
    if off==0xa14:
        assert reg(3)==BOSS
        registration.update(position=list(get(WORK+20,"fff")),angles=list(get(WORK+8,"iii")))
    if off in (0x5598,0x580c):
        u.reg_write(R.UC_PPC_REG_PC,ret);return
    if off==0x55ac:
        assert reg(3)==child;setreg(3,int(hit));queries.append("child")
    if off==0x5740:
        assert reg(3)==BOSS;setreg(3,PLAYER if body else 0);queries.append("body")
    if off in (0x5644,0x57e8):
        impulses.append(dict(slot=reg(3),velocity=list(get(reg(4),"fff")),
            angles=list(get(reg(5),"iii")),duration=reg(6)))
    if off==0x6790:
        assert get(reg(3),"fff")==tuple(position)
        put(reg(3),"fff",*vector);calls.append(off)
    u.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x1e20,0x1e2c,0x1d8c,0x1db0,0x414,0x41c,0x48c,0x4c4,0x1a84,0x1aa8,0x1764,0x18d8,0x1990,0x10,0x34,0x44,0x54,0x68,0x80,0x94,0x134,0x168,0x180,0x1a4,0x1d4,0x1e0,0x208,0x238,0x250,0x338,0x55a4,0x5598,0x580c,0x55ac,0x5740,0x5644,0x57e8,0x824,0xa48,0x8d0,0x8dc,0x8e8,0x910,0x91c,0x8f8,0x92c,0x978,0x9b8,0x9f0,0xa14,0x6790,0xda0,0xe64,0xea0,0xeb8,0xeec,0xf08,0xf38,0xfa8,0xfb0,0xfcc,0x54764,0x547dc,0x1004,0x1024,0x1618,0x10b8,0x10c0,0x10d8,0x112c,0x1134,0x1150,0x115c,0x11a0,0x1234,0x1248,0x1268,0x12c0,0x12e0,0x12f0,0x12fc,0x145c,0x149c,0x14b4,0x15a0,0x15c4):
    u.hook_add(UC_HOOK_CODE,hook,begin=B+off,end=B+off)
def invoke(pc,task=STAGE):
    setreg(1,T+0x2ff0);setreg(3,task);u.reg_write(R.UC_PPC_REG_LR,ret)
    u.emu_start(B+pc,ret,count=1200);assert u.reg_read(R.UC_PPC_REG_PC)==ret
def defeat_clock(low):
    # Execute the original timer blocks; omit motion/effect code between them.
    end=0x402c if low else 0x4650
    calls=[];rows=[]
    def trace(uc,pc,size,_):
        off=pc-B
        if off in defeat_targets:
            calls.append(dict(address=f"{defeat_targets[off]:08x}",argument=reg(3)))
            uc.reg_write(R.UC_PPC_REG_PC,pc+4)
        elif off in jumps:
            uc.reg_write(R.UC_PPC_REG_PC,B+jumps[off])
    jumps={0x3074:0x30b8,0x30c4:0x36ac,0x36c0:0x36e8,0x42ac:0x45b8}
    handles=[u.hook_add(UC_HOOK_CODE,trace,begin=B+a,end=B+a) for a in [*jumps,*defeat_targets]]
    page(B+end);setreg(28 if low else 29,WORK)
    try:
        for _ in range(400):
            sm=get(WORK+1,"B")[0]
            pc=([0x306c,0x36ac,0x3eb8] if low else [0x4298,0x45b8])[sm]
            u.emu_start(B+pc,B+end,count=75)
            assert u.reg_read(R.UC_PPC_REG_PC)==B+end
            rows.append(dict(submode=get(WORK+1,"B")[0],timer=get(WORK+6,"H")[0]))
            if calls:return dict(frames=rows,handoffUpdate=len(rows),callsOnHandoff=calls)
        raise AssertionError("no defeat handoff")
    finally:
        for handle in handles:u.hook_del(handle)
rng=random.Random(60)
positions=[[0,7,0],[120,50,0],[72,-70,96],[120.00001,1,0],[119.99999,1,0]]
positions.extend([[rng.uniform(-180,180),rng.uniform(-30,30),rng.uniform(-180,180)] for _ in range(96)])
with tempfile.TemporaryDirectory(dir=Path.cwd()/".local") as tmp:
    tmp=Path(tmp)
    for ok in (True,False):
        u.mem_write(T,bytes(4096));put(BOSS+0x34,"I",WORK);put(BOSS+0x40,"I",EXTRA);put(WORK+0x2c,"I",CCL)
        saved=[];result={};calls=[];invoke(0x66bc)
        stage_exec=get(STAGE+16,"I")[0]-B
        frames=[];expected=[];put(player_ptr,"I",PLAYER)
        for raw in positions:
            put(PLAYER+20,"fff",*raw);position=list(get(PLAYER+20,"fff"))
            # Distinct supplied vector verifies which components the native caller consumes.
            vector=[0.75,-0.125,-0.5];calls.clear();invoke(stage_exec)
            f={"player_position":position}
            if calls:f["vector_result"]=vector
            frames.append(f);expected.append(list(get(PLAYER+20,"fff")))
        req=tmp/"request.json";req.write_text(json.dumps({"work_allocation_ok":ok,"frames":frames}))
        out=tmp/f"{ok}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
        j=json.loads(out.read_text())["replay"];b=j["boss"]
        assert j["stageExecutor"]==stage_exec and b["created"]==ok
        if ok:
            assert b["position"]==list(get(WORK+20,"fff")) and b["yaw"]==get(WORK+12,"i")[0]
            assert (b["mode"],b["taskByte2"],b["timer"])==(get(WORK,"B")[0],get(WORK+2,"B")[0],get(WORK+6,"H")[0])
            assert (b["executor"],b["animationRequest"])==(result["executor"],result["animationRequest"])
            assert tuple(b["collision"][k] for k in ("descriptorOffset","count","id"))==result["collision"]
        else:assert result["freed"] and b["calls"]==["freeBossTask"]
        for f,e in zip(j["frames"],expected,strict=True):
            assert struct.pack(">fff",*f["playerPosition"])==struct.pack(">fff",*e)
    for case,(created,query) in enumerate([(False,0),(True,1),(True,7)]):
        ok=True
        u.mem_write(T,bytes(4096));put(BOSS+0x34,"I",WORK);put(BOSS+0x40,"I",EXTRA);put(WORK+0x2c,"I",CCL)
        put(player_ptr,"I",PLAYER);saved=[];result={};invoke(0x66bc)
        observation=[11.,-4.,-21.]
        trace=[]
        for i in range(451):
            if i==450:put(PLAYER+20,"fff",*observation)
            intro_calls=[];invoke(0xd90,BOSS)
            trace.append((get(WORK,"BB"),get(WORK+6,"H")[0],get(WORK+4,"H")[0],intro_calls))
        intro={"updates":451,"helper_task_created":created,"exit":{"query_result":query}}
        if query>0:intro["exit"]["player_position"]=observation
        req.write_text(json.dumps({"work_allocation_ok":True,"frames":[],"intro":intro}))
        out=tmp/f"intro{case}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
        for row,expected in zip(json.loads(out.read_text())["replay"]["intro"]["frames"],trace,strict=True):
            assert ((row["mode"],row["submode"]),row["timer"],row["flags"],row["calls"])==expected,(case,row,expected)
        # Follow the exact mode-one handoff. The unported outer physics and animation
        # callbacks are represented by per-tick observations on both sides.
        motion=T+0xd00;anim=T+0xe00
        put(BOSS+0x38,"I",motion);put(EXTRA+12,"I",anim)
        take_inputs=[];take_native=[];visits=[0]*5
        for _ in range(160):
            sub=get(WORK+1,"B")[0];visits[sub]+=1;n=visits[sub]
            x=[90.,-90.,0.][case];y=0.;vy=0.;motion_id=0;ready=None
            if sub==1:ready=n>=33+case
            if sub==2:
                motion_id=1 if n<3 else 2;ready=n==2 if motion_id==1 else None
                vy=2. if n<5 else -0.25
            if sub==3:
                motion_id=1 if n==1 else 2;ready=True if motion_id==1 else None
                y=[3.,20.,10.,10.][min(n-1,3)];vy=1. if n<20 else 0.01
            if sub==4:ready=n==4
            put(WORK+20,"fff",x,y,0);put(motion+4,"f",vy);put(anim+4,"i",motion_id)
            entry={"position":[x,y,0],"velocity_y":vy,"motion_id":motion_id}
            if ready is not None:entry["motion_ready"]=ready
            take_inputs.append(entry);take_calls=[];motion_queries=[];invoke(0x1014,BOSS)
            assert bool(motion_queries)==(ready is not None)
            take_native.append({"mode":get(WORK,"B")[0],"submode":get(WORK+1,"B")[0],
                "flags":get(WORK+4,"H")[0],"timer":get(WORK+6,"H")[0],"position":list(get(WORK+20,"fff")),
                "velocityY":get(motion+4,"f")[0],"acceleration":[get(motion,"f")[0],get(motion+16,"f")[0]],
                "phase":list(get(motion+0x84,"iii")),"intensity":get(motion+0xb8,"f")[0],
                "burst":get(motion+0xbc,"f")[0],"calls":take_calls})
            if get(WORK,"B")[0]==3:break
        assert all(visits) and take_native[-1]["mode"]==3
        req.write_text(json.dumps({"work_allocation_ok":True,"frames":[],"intro":intro,"takeoff":take_inputs}))
        out=tmp/f"takeoff{case}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
        for i,(row,expected) in enumerate(zip(json.loads(out.read_text())["replay"]["takeoff"],take_native,strict=True)):
            assert row==expected,(case,i,row,expected)
        # Continue directly from the native takeoff with the first flight decision blocks.
        table=T+0x3500;page(table);put(CCL+12,"I",table);put(table+0x154,"I",0x80)
        original_counter=get(EXTRA+2,"B")[0];assert original_counter==3
        first=[];expected_flight=[];side=-1 if case==1 else 1
        steering=dict(angles=list(get(WORK+8,"iii")),rates=[0.25,-0.625,1.25],previous_speed_squared=0.375)
        put(motion+0x6c,"fff",*steering["rates"]);put(motion+0xb4,"f",steering["previous_speed_squared"])
        approach=[(60.,4.,0.,rng.uniform(-6,6)) for _ in range(96)]
        for px,pz,bx,vx in approach+[(10,130,0,2),(-10,-130,0,-2),(15,4,0,0),(9.999,88,0,1),
                             (0,0,side*120,side),(0,0,side*120.01,0 if side<0 else 2)]:
            first.append(dict(player_xz=[px,pz],boss_x=bx,velocity_x=vx))
            put(PLAYER+20,"fff",px,0,pz);put(WORK+20,"f",bx);put(motion,"f",vx)
            setreg(1,T+0x2f00);setreg(3,BOSS);flight_calls=[]
            i=len(first)-1
            delta=[0,8191,-8191,8192,-8192,12000][i%6]
            turn_sin=[0.,-0.9375,0.9375,None,None,None][i%6]
            old_yaw=get(WORK+12,"I")[0];yaw_sin,yaw_cos=[(.75,-.5),(-.875,.125),(.25,.75)][i%3]
            vz=0.25*(i-case);acc=[0.25*i,-0.5*(i+1),0.125*(i-2)]
            put(motion+8,"f",vz);put(motion+12,"fff",*acc)
            first[-1]["motion"]=dict(velocity_z=vz,acceleration=acc,yaw_sin_cos=[yaw_sin,yaw_cos],heading_difference=delta,turn_sine=turn_sin)
            u.emu_start(B+0x163c,B+0x1b14,count=500);assert u.reg_read(R.UC_PPC_REG_PC)==B+0x1b14
            assert get(table+0x154,"I")== (0x90,)
            expected_flight.append(dict(mode=get(WORK,"B")[0],submode=get(WORK+1,"B")[0],
                targetXZ=[get(motion+k,"f")[0] for k in (0x54,0x5c)],extraByte2=get(EXTRA+2,"B")[0],
                enableShape7Attribute16=0x1764 in flight_calls,callHelper55d78=any(a in flight_calls for a in (0x18d8,0x1990))))
            expected_flight[-1]["steering"]=dict(velocityXZ=[get(motion+k,"f")[0] for k in (0,8)],
                acceleration=list(get(motion+12,"fff")),angles=list(get(WORK+8,"iii")),rates=list(get(motion+0x6c,"fff")),
                speedSquared=get(motion+0xb4,"f")[0],phase=list(get(motion+0x84,"iii")))
        assert expected_flight[-1]["mode"]==4
        request=json.loads(req.read_text());request["first_flight"]=dict(ticks=first,steering=steering)
        # Continue without resetting the native motion or Rust steering state.
        turn=[];expected_turn=[]
        cases=[(side*166,side*2)]*18+[(side*166,0),(side*166,-side),(side*150,-side),(side*149.99,-side)]
        if case==2:cases=[(0,1),(150,1),(149,1)] # entry falls through and reverses immediately
        for i,(x,vx) in enumerate(cases):
            z=[64.,-64.,48.][case];obs=dict(boss_xz=[x,z],velocity_x=vx)
            if i==0:obs["player_xz"]=[19.,z*2]
            put(PLAYER+20,"fff",19.+i,0,z*2+i);put(WORK+20,"f",x);put(WORK+28,"f",z)
            old_yaw=get(WORK+12,"I")[0];delta=8192 if i%2 else -100;turn_sin=None if i%2 else 0.015625
            yaw_sin,yaw_cos=0.125,-0.9375;vz=-2.5+i/8;acc=[i/4,0.5,2.]
            put(motion,"f",vx);put(motion+8,"f",vz);put(motion+12,"fff",*acc)
            obs["motion"]=dict(velocity_z=vz,acceleration=acc,yaw_sin_cos=[yaw_sin,yaw_cos],heading_difference=delta,turn_sine=turn_sin)
            turn_helpers=[];invoke(0x1bcc,BOSS);assert turn_helpers==[0x1e20,0x1e2c]
            turn.append(obs);expected_turn.append(dict(mode=get(WORK,"B")[0],submode=get(WORK+1,"B")[0],flags=get(WORK+4,"H")[0],
                targetXZ=[get(motion+k,"f")[0] for k in (0x54,0x5c)],steering=dict(velocityXZ=[get(motion+k,"f")[0] for k in (0,8)],
                acceleration=list(get(motion+12,"fff")),angles=list(get(WORK+8,"iii")),rates=list(get(motion+0x6c,"fff")),
                speedSquared=get(motion+0xb4,"f")[0],phase=list(get(motion+0x84,"iii")))))
        assert expected_turn[-1]["mode"]==3 and get(EXTRA+2,"B")[0]==2
        request["first_flight"]["turnaround"]=turn
        req.write_text(json.dumps(request));out=tmp/f"flight{case}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
        for a,e in zip(json.loads(out.read_text())["replay"]["firstFlight"],expected_flight,strict=True):assert a==e,(a,e)
        for a,e in zip(json.loads(out.read_text())["replay"]["turnaround"],expected_turn,strict=True):assert a==e,(a,e)
        print("turnaround updates matched:",len(expected_turn),"next mode:",expected_turn[-1]["mode"])
        print("mode-two callbacks:",len(take_native),"; first-pass steering updates matched:",len(expected_flight))
    observations=[];expected=[]
    motion=T+0xd00;put(BOSS+0x34,"I",WORK);put(BOSS+0x38,"I",motion)
    for mode in (1,2,3,13):
        for y in (13.499,13.5,13.501):
            for flags in (0,1,0x1000,0x1001):
                for angles in ([2147483647,65535,-2147483648],[-12,0,24]):
                    loc=[3.,7.,-9.];anchors=[[float(rng.randrange(-50,50))/8 for _ in range(3)] for _ in range(3)]
                    sines=[1.,-0.875,0.25]
                    obs=dict(mode=mode,flags=flags,position=loc,angles=angles,reference_y=y,anchors=anchors)
                    if not flags&1:obs['sine_results']=sines
                    for i,a in enumerate(anchors):put(D+6*0x400000+180+i*12,"fff",*a)
                    put(D+6*0x400000+136,"f",y)
                    put(WORK,"B",mode);put(WORK+4,"H",flags);put(WORK+8,"iii",*angles)
                    put(WORK+20,"fff",*loc);put(motion+0x84,"iii",71,97,83)
                    centers=[];registration={};invoke(0x810,BOSS)
                    observations.append(obs);expected.append(dict(centers=centers,registration=registration,
                        afterRegistration=dict(position=list(get(WORK+20,"fff")),angles=list(get(WORK+8,"iii")))))
    req.write_text(json.dumps({"work_allocation_ok":True,"frames":[],"collision":observations}))
    out=tmp/"collision.json"
    subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
    c=json.loads(out.read_text())["replay"]["collision"]
    assert c["frames"]==expected
    for i,row in enumerate(c["descriptors"]):
        raw=sec(5)+i*48
        assert row["headerBytes"]==list(d[raw:raw+4]) and row["attributes"]==w(raw+4)
        assert row["originalCenter"]+row["parameters"]==list(struct.unpack_from(">7f",d,raw+8))
        assert row["angles"]==list(struct.unpack_from(">iii",d,raw+36))
    print(len(expected),"collision callbacks and",sum(len(f["centers"]) for f in expected),"center updates matched")
    child=T+0x4000;page(child);page(T+0x3000);childw=child+0x100;childc=child+0x200
    bd=child+0x300;cd=child+0x600;put(child+0x34,"I",childw);put(childw+0x2c,"I",childc)
    put(WORK+0x2c,"I",CCL);put(CCL+12,"I",bd);put(childc+12,"I",cd);put(CCL+6,"H",11);put(childc+6,"H",1)
    put(PRIVATE+0x54,"I",child);put(player_ptr,"I",PLAYER)
    for path,y in enumerate((4.999,5.,5.001)):
        observations=[];expected=[];put(EXTRA+0x28,"f",4.)
        cases=[(m,flags,h,b,pflag) for m in (3,9,10,13) for flags in (0,4,0x1000,0x1004)
               for h in (False,True) for b,pflag in [(False,0),(True,0),(True,256)] if not (m==3 and not flags and h)]
        cases.extend([(3,1,True,False,0),(3,0,True,False,0),(2,1,True,False,0),(3,0,True,False,0)])
        for i,(mode,flags,hit,body,pflag) in enumerate(cases):
            yaw=2147483640-i
            put(WORK,"BBBBHH",mode,2,6,(9,255,5)[i%3],flags,42);put(WORK+12,"i",yaw);put(WORK+24,"f",y)
            put(PLAYER+4,"H",pflag);put(PLAYER+12,"i",-yaw)
            bs=[rng.randrange(256) for _ in range(11)];cs=rng.randrange(256)
            cf=rng.randrange(65536);ccf=rng.randrange(65536)
            put(childw+4,"H",cf);put(childc+4,"H",ccf);put(cd+3,"B",cs)
            for j,v in enumerate(bs):put(bd+48*j+3,"B",v)
            reqframe=dict(mode=mode,flags=flags,height=y,yaw=yaw,submode=2,byte3=(9,255,5)[i%3],timer=42)
            for rr,v in [(26,BOSS),(27,PRIVATE),(30,EXTRA),(31,WORK)]:setreg(rr,v)
            queries=[];impulses=[];prior=get(EXTRA+0x28,"f")[0]
            invoke(0x557c)
            if "child" in queries:reqframe["child_hit"]=hit
            if "body" in queries:
                reqframe["body_hit"]=body
                if body:reqframe["player"]={"flags":pflag,"yaw":-yaw}
            hp=get(EXTRA+0x28,"f")[0];changes=None
            if hp!=prior:
                reqframe["damage_work"]=dict(boss_shape_bytes=bs,child_shape_byte=cs,child_flags=cf,child_collision_flags=ccf)
                changes=dict(bossShapeBytes=[get(bd+48*j+3,"B")[0] for j in range(11)],childShapeByte=get(cd+3,"B")[0],
                    childFlags=get(childw+4,"H")[0],childCollisionFlags=get(childc+4,"H")[0])
            expected.append(dict(mode=get(WORK,"B")[0],submode=get(WORK+1,"B")[0],byte3=get(WORK+3,"B")[0],timer=get(WORK+6,"H")[0],
                flags=get(WORK+4,"H")[0],health=hp,impulse=impulses[0] if impulses else None,damageWork=changes))
            observations.append(reqframe)
        assert expected[-1]["health"]==0 and expected[-1]["mode"]==(10 if path==0 else 13)
        req.write_text(json.dumps({"work_allocation_ok":True,"frames":[],"damage":observations}));out=tmp/f"damage{path}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-big-foot",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","2",str(req)],check=True)
        damage=json.loads(out.read_text())["replay"]["damage"]
        assert damage["defeatClock"]==defeat_clock(path==0)
        actual=damage["frames"]
        for i,(a,e) in enumerate(zip(actual,expected,strict=True)):assert a==e,(path,i,a,e)
    print(3*len(expected),"post-AI damage callbacks matched, including both lethal routes")
print("1,353 mode-one callbacks matched the native player placement, state, timer and helper calls")
print("Both Big Foot setup routes and 202 arena callbacks matched installed PPC")
