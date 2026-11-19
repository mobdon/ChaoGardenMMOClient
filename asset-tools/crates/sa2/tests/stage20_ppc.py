"""Compare stage 20 entry frames with the installed registered callback."""
import os,json,struct,tempfile,sys,subprocess
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_BIG_ENDIAN,UC_MODE_PPC32,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);path=game/"resource/gd_PC/stg20D.rel";d=path.read_bytes()
w=lambda off:struct.unpack_from(">I",d,off)[0]
u=Uc(UC_ARCH_PPC,UC_MODE_BIG_ENDIAN|UC_MODE_PPC32);base={1:0x81200000,4:0x81800000,5:0x81a00000}
for s,b in base.items():
    row=w(16)+8*s;at=w(row)&~1;size=w(row+4);u.mem_map(b,(size+4095)&~4095);u.mem_write(b,d[at:at+size])
def put(addr,fmt,*vals):u.mem_write(addr,struct.pack(">"+fmt,*vals))
def get(addr,fmt):return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
engines={}
for mod,i in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        n,k,s,v=struct.unpack_from(">HBBI",d,i);i+=8
        if k==203:break
        if k==202:t=s;off=0;continue
        off+=n
        if t not in base:continue
        if mod==19 and s in base:v+=base[s]
        elif mod!=0:continue
        if mod==0 and t==1 and k==10:engines[off]=v
        if k==1:put(base[t]+off,"I",v)
        if k in (4,6):put(base[t]+off,"H",(v if k==4 else (v+32768)>>16)&65535)
        if t==1 and off==0x1c2c and k==10:service=v
# Resolve the registered entry pointer from the stage descriptor.
entry=get(base[5]+88932+20,"I")[0]
u.mem_map(0x100000,4096);u.mem_map(0x801e7000,4096);u.reg_write(R.UC_PPC_REG_MSR,8192)
task=0x100100;work=0x100200;target=0x100300;stop=0x100400
def watch(cpu,pc,*_):
    p=cpu.reg_read(R.UC_PPC_REG_4);pos,f,g,n,a,b,h,j,m=get(p,"IffIBB2xffI")
    assert pos==work+20
    calls.append(dict(engine=service,slot=cpu.reg_read(R.UC_PPC_REG_3),positionAtCall=list(get(pos,"fff")),
                      float4=f,float8=g,word12=n,byte16=a,byte17=b,float20=h,float24=j,word28=m))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
u.hook_add(UC_HOOK_CODE,watch,begin=base[1]+0x1c2c,end=base[1]+0x1c2c)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for mode in (-128,-1,0,1,2,127):
        observed=[None,[4.,5.,6.],None,[-2.,-10.,30.]];position=[3.,7.,-9.]
        put(task+0x34,"I",work);put(work,"b",mode);put(work+20,"fff",*position)
        frames=[]
        for value in observed:
            calls=[];put(0x801e736c,"I",target if value else 0)
            if value:put(target+20,"fff",*value)
            u.reg_write(R.UC_PPC_REG_1,0x100f00);u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_LR,stop)
            u.emu_start(entry,stop,count=100);assert u.reg_read(R.UC_PPC_REG_PC)==stop and len(calls)<=1
            frames.append(dict(mode=get(work,"b")[0],position=list(get(work+20,"fff")),call=next(iter(calls),None)))
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=mode,position=position,targets=observed)))
        out=Path(tmp)/f"{mode}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        assert json.loads(out.read_text())==dict(module=19,entry=entry-base[1],frames=frames)
print("24 consecutive stage 20 task frame observations matched")

startup=get(base[5]+88932+12,"I")[0]
def startup_call(cpu,pc,*_):
    site=pc-base[1];r3=cpu.reg_read(R.UC_PPC_REG_3);r4=cpu.reg_read(R.UC_PPC_REG_4)
    if site==0x1744:
        assert r3==target;ops.append(dict(site=site,slot=1,workByte=2,value=cpu.reg_read(R.UC_PPC_REG_0)))
        return
    if site in (0x1750,0x175c,0x1768):
        displacement=get(pc,"I")[0]&0x03fffffc
        if displacement&0x02000000:displacement-=0x04000000
        ops.append(dict(site=site,local=site+displacement,args=[] if site==0x1750 else [r3,r4]))
    else:
        ops.append(dict(site=site,engine=engines[site],args=[r3]))
        if site in (0x16e8,0x1704):cpu.reg_write(R.UC_PPC_REG_3,next(answers)&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x16e8,0x16f8,0x1704,0x1714,0x1730,0x1744,0x174c,0x1750,0x175c,0x1768):
    u.hook_add(UC_HOOK_CODE,startup_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,queries in enumerate(([7],[6,6],[6,7],[0,6],[0,0],[-1,-1])):
        for present in (False,True):
            ops=[];answers=iter(queries);put(0x801e738c,"I",task if present else 0);put(0x801e736c,"I",target);put(target+2,"B",0xdd)
            u.reg_write(R.UC_PPC_REG_1,0x100f00);u.emu_start(startup,base[1]+0x176c,count=100)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x176c and get(target+2,"B")[0]==(3 if present else 0xdd)
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],startup=dict(character_queries=queries,slot_one_present=present))))
            out=Path(tmp)/f"{idx}-{present}.json"
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            assert json.loads(out.read_text())["startup"]==dict(entry=startup-base[1],continueAt=0x176c,operations=ops)
print("12 stage 20 opponent startup paths matched")

def walker_call(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    label=lambda addr:bytes(cpu.mem_read(addr,32)).split(b"\0")[0].decode()
    if site==0x15b4:
        args=[reg(3),reg(4),reg(5)-base[1],label(reg(6))];val=task if alloc else 0
    elif site==0x15cc:args=[reg(3),reg(4)];val=work if mem else 0
    elif site==0x15e0:assert reg(3)==task;args=["task"];val=0
    elif site==0x161c:
        assert reg(3)==task and reg(4)==work+8 and get(task+64,"I")[0]==work
        ops.append(dict(taskWorkOffset=64,value="work"))
        ops.append(dict(local=0x3361c,args=["task",get(reg(4),"f")[0]]))
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4);return
    elif site==0x1628:
        ops.append(dict(workByte=0,value=get(work,"B")[0]));return
    else:args=[label(reg(3)),reg(4)-base[5]];val=0
    ops.append(dict(engine=engines[site],args=args))
    cpu.reg_write(R.UC_PPC_REG_3,val);cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x15b4,0x15cc,0x15e0,0x161c,0x1628,0x164c,0x1678):
    u.hook_add(UC_HOOK_CODE,walker_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,(alloc,mem,ch,v) in enumerate(((False,None,None,None),(True,False,None,None),
            (True,True,6,17.5),(True,True,7,-3.75),(True,True,-1,0.))):
        ops=[];put(0x801e732c,"I",target);put(task+24,"I",0);put(task+64,"I",0);put(work,"B",99)
        if mem:put(target+3,"b",ch);put(target+72,"f",v)
        u.reg_write(R.UC_PPC_REG_1,0x100f00);u.reg_write(R.UC_PPC_REG_LR,stop)
        u.emu_start(base[1]+0x1584,stop,count=180);assert u.reg_read(R.UC_PPC_REG_PC)==stop
        callback=get(task+24,"I")[0]
        if callback:ops.append(dict(taskCallbackOffset=24,local=callback-base[1]))
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],
            startup=dict(character_queries=[7],slot_one_present=True,walker=dict(task_allocated=alloc,work_allocated=mem,character=ch,slot_value=v)))))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        assert json.loads(out.read_text())["startup"]["walker"]==ops
print("5 walker controller initialization paths matched")

# Run the controller through dispatch and, for state zero, through its opening block.
u.mem_map(0x801cb000,8192)
player0,player1,pose0,pose1=(0x100500,0x100600,0x100700,0x100800)
put(0x801e7328,"II",player0,player1);put(0x801e7368,"II",pose0,pose1)
branches={get(base[5]+0x298+i*4,"I")[0]-base[1] for i in range(11)}
running_eight=False
def opening_call(cpu,pc,*_):
    if running_eight:
        if pc==base[1]+0x1424:cpu.emu_stop()
        return
    global dispatched
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site in branches|{0x1424,0x1518}:
        if dispatched is None:dispatched=site
        if site!=0x828:cpu.emu_stop();return
    if site==0x768:pass
    elif site==0x828:ops.append([site]);cpu.reg_write(R.UC_PPC_REG_3,query)
    elif site==0x838:ops.append([site,reg(3)])
    elif site==0x83c:ops.append([site])
    elif site in (0x86c,0x888):ops.append([site,reg(3),reg(4)-base[5]])
    elif site==0x91c:
        angle,yaw,ptr,x,y,last=get(reg(4),"fIIffI")
        assert ptr==work+12
        ops.append([site,reg(3),angle,yaw,list(get(ptr,"fff")),x,y,last])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in branches|{0x768,0x828,0x838,0x83c,0x86c,0x888,0x91c,0x1424,0x1518}:
    u.hook_add(UC_HOOK_CODE,opening_call,begin=base[1]+site,end=base[1]+site)
cases=[(m,[0,0,0],[0,0],16,6) for m in range(-1,12)]
cases+=[(0,b,f,q,ch) for b,f,q,ch in (([1,0,0],[0,0],16,7),([0,-2,0],[0,0],16,7),([0,0,1],[0,0],16,7),
          ([0,0,0],[0x4000,0],16,7),([0,0,0],[0,0x4000],16,7),([0,0,0],[1,0],15,7),([0,0,0],[0,0],16,7),([0,0,0],[0,0],16,-1))]
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,(mode,b,f,query,ch) in enumerate(cases):
        ops=[];dispatched=None;yaw=4;pos=[5.,91.,-3.]
        for off,v in zip((0x39,0x37,0x42),b):put(0x801cbd88+off,"b",v)
        put(player0+16,"H",f[0]);put(player1+16,"H",f[1]);put(player1+3,"b",ch)
        put(pose1+12,"I",yaw);put(pose1+20,"fff",*pos)
        put(task+64,"I",work);put(work,"b",mode);put(work+4,"h",-12)
        u.reg_write(R.UC_PPC_REG_1,0x100f00);u.reg_write(R.UC_PPC_REG_3,task)
        u.emu_start(base[1]+0x754,stop,count=300);end=u.reg_read(R.UC_PPC_REG_PC)-base[1]
        c=dict(mode=mode,engine_bytes=b,player_flags=f)
        if dispatched==0x828:c["opening"]=dict(query=query,character=ch,yaw=yaw,position=pos)
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        timer=get(work+4,"h")[0]
        assert json.loads(out.read_text())["controller"]==dict(dispatch=dispatched,continueAt=end,mode=get(work,"b")[0],timerWrite=None if timer==-12 else timer,operations=ops)
print(len(cases),"walker dispatch and opening observations matched")
# Enter at 0x1428 with r3 already set as by 0x1424, past the dispatch test's stop hook.
def tail_call(cpu,pc,*_):
    site=pc-base[1];calls.append(site)
    if site==0x142c:assert cpu.reg_read(R.UC_PPC_REG_3)==work and cpu.reg_read(R.UC_PPC_REG_4)==pose1
    elif site==0x1494:
        assert cpu.reg_read(R.UC_PPC_REG_3)==target
        put(task+12,"I",target if remains else 0)
    elif site==0x1514:assert cpu.reg_read(R.UC_PPC_REG_3)==task and cpu.reg_read(R.UC_PPC_REG_4)==work+8
    else:cpu.reg_write(R.UC_PPC_REG_3,next(returns)&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x142c,0x1440,0x144c,0x1458,0x1494,0x1514):
    u.hook_add(UC_HOOK_CODE,tail_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for flag,queries,word in [(0x4000,[],1),(0,[9],1),(0,[0,12],1),(0,[0,0,13],1),(0,[0,0,0],0),(0,[0,0,0],1)]:
        for value,cooldown,child,remains in [(0.,0,False,False),(-1.,-3,True,True),(7.,3,False,False),(11.,5,True,False)]:
            calls=[];returns=iter(queries)
            put(task+12,"I",target if child else 0);put(player0+16,"H",flag);put(0x801cbd88+124,"I",word)
            put(work,"bbh",4,3,32767);put(work+6,"h",cooldown);put(player1+72,"f",value)
            for r,v in [(3,work),(25,task),(26,player1),(27,pose1),(29,work),(30,0x801e732c),(31,player0)]:
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
            u.emu_start(base[1]+0x1428,base[1]+0x1518,count=100);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1518
            t=dict(mode=4,submode=3,counter=32767,cooldown=cooldown,slot0_flags=flag,queries=queries,engine_word=word,
                   slot1_value=value,child=child,child_after_remove=remains if 0x1494 in calls else None)
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],tail=t)))
            out=Path(tmp)/f"{idx}.json";idx+=1
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            m,s,c=get(work,"bbh")
            assert json.loads(out.read_text())["tail"]==dict(mode=m,submode=s,counter=c,cooldown=get(work+6,"h")[0],
                         slot1Value=get(player1+72,"f")[0],scaled=get(work+8,"f")[0],calls=calls)
print(idx,"walker shared tail observations matched")

def seven_call(cpu,pc,*_):
    site=pc-base[1];calls.append(site)
    if site in (0x1344,0x1358,0x1374):return # Execute the stage transition helper itself.
    value=draw
    cpu.reg_write(R.UC_PPC_REG_3,value&0xffffffff);cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1344,0x1358,0x1374,0x398,0x558,0x5c4):
    u.hook_add(UC_HOOK_CODE,seven_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    u.mem_map(0x803ad000,4096)
    for slot,word,flag in [(6,None,0),(6,None,-1),(6,0,0),(6,0,1),(6,1,0),(6,0x80000000,-128),(7,None,0),(-1,None,0)]:
        for counter in (-1,120,121):
            for draw in (0,16383,32767):
                calls=[];put(0x801e738c,"I",task);put(task+64,"I",0 if word is None else 0x100900)
                if word is not None:put(0x100c8c,"I",word)
                put(0x803ad494,"b",flag);put(player1+1,"b",slot);put(work,"bbhh",7,0,counter,-1)
                for r,v in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work),(0,slot&255)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
                # Prior dispatch coverage checks the first instruction loads slot-one byte 1.
                u.emu_start(base[1]+0x132c,base[1]+0x1424,count=150)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
                m,_,ctr,timer=get(work,"bbhh")
                c=dict(mode=7,engine_bytes=[0,0,0],player_flags=[0,0],
                       state_seven=dict(slot_byte=slot,counter=counter,decision=dict(work_word=word,engine_byte=flag) if slot==6 else None,draw=draw if m!=7 else None))
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0x1328,continueAt=0x1424,
                           mode=m,timerWrite=timer if m!=7 else None,counter=ctr,operations=calls)
print(idx,"state seven transitions matched")

running_eight=True
u.mem_map(0x801e4000,8192)
def eight_call(cpu,pc,*_):
    site=pc-base[1];ops.append(site)
    if site==0x13e8:return
    if site==0x1394:
        args.extend(struct.unpack(">d",cpu.reg_read(r).to_bytes(8,"big"))[0] for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2))
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",angle),"big"))
    else:cpu.reg_write(R.UC_PPC_REG_3,draw)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1394,0x13e8,0x448,0x4a8):
    u.hook_add(UC_HOOK_CODE,eight_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for pose,slot,draw in [(0,6,0),(0,7,32767),(0,-1,16384),(1,6,None),(65535,7,None)]:
        for counter,angle in [(-8,-2.75),(0,0.),(7,1.7),(8,3.14)]:
            ops=[];args=[];p0=[4.,11.,-31.];p1=[20.,-12.,41.]
            put(work,"bbhh",8,0,counter,-1);put(pose0+20,"fff",*p0);put(pose1+20,"fff",*p1)
            put(pose1+6,"H",pose);put(player1+1,"b",slot)
            pulse_bytes=[0x803ad485,0x803ad495]
            for addr in pulse_bytes:put(addr,"B",0)
            for r,v in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work)]:
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
            u.emu_start(base[1]+0x137c,base[1]+0x1424,count=160);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
            m,_,ctr,timer=get(work,"bbhh")
            c=dict(mode=8,engine_bytes=[0,0,0],player_flags=[0,0],state_eight=dict(slot0=p0,slot1=p1,
                   angle_return=angle,pose_word=pose,slot_byte=slot,counter=counter,draw=draw))
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
            out=Path(tmp)/f"{idx}.json";idx+=1
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            yaw,scalar=get(0x801e5000,"if")
            assert json.loads(out.read_text())["controller"]==dict(dispatch=0x137c,continueAt=0x1424,mode=m,counter=ctr,
                timerWrite=timer if m==4 else None,operations=ops,angleArgs=args,yaw=yaw,scalar=scalar,
                byteWrites=[addr for addr in pulse_bytes if get(addr,"B")== (1,)])
print(idx,"state eight updates matched")

messenger_case=None
def one_call(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x9f8:ops.append([site,list(get(reg(3),"fff")),*[reg(i) for i in range(4,9)]])
    elif site in (0xa00,0xa84,0xac4):ops.append([site,reg(3)])
    else:ops.append([site])
    if site==0x9f8 and messenger_case is not None:return
    if site in (0x988,0xa18):cpu.reg_write(R.UC_PPC_REG_3,(query if site==0x988 else draw)&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x988,0x9f8,0xa00,0xa04,0xa18,0xa84,0xac4):
    u.hook_add(UC_HOOK_CODE,one_call,begin=base[1]+site,end=base[1]+site)
cases=[(ctr,lim,0,ch,0,None,None) for ctr,lim in [(119,360),(120,120),(120,360),(121,360),(360,360)] for ch in (6,7,-1)]
cases += [(ctr,lim,0,6,b,q,draw) for ctr,lim in [(120,119),(361,360)] for b in (6,7)
          for q,draw in [(0,0),(1,123),(2,32767),(3,20000),(-1,12000)]]
cases += [(361,360,s,7,6,None,16384) for s in (-1,1)]
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,(counter,limit,sub,ch,b,query,draw) in enumerate(cases):
        ops=[];put(work,"bbhhh",1,sub,counter,limit,-1);put(player1+3,"b",ch);put(player0+1,"b",b)
        for r,v in [(1,0x100e00),(26,player1),(29,work),(31,player0)]:u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
        u.emu_start(base[1]+0x924,base[1]+0x1424,count=180);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
        m,s,ctr,t,cool=get(work,"bbhhh")
        c=dict(mode=1,engine_bytes=[0,0,0],player_flags=[0,0],state_one=dict(counter=counter,limit=limit,
               submode=sub,character=ch,slot0_byte=b,query=query,draw=draw))
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        assert json.loads(out.read_text())["controller"]==dict(dispatch=0x924,continueAt=0x1424,mode=m,submode=s,counter=ctr,
                   timerWrite=t if m==7 else None,cooldownWrite=None if cool==-1 else cool,operations=ops)
print(len(cases),"state one timing and transition paths matched")

def inspect_messenger(cpu,pc,*_):
    global saved_registers
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x26030:saved_registers=[reg(i) for i in range(25,32)]
    elif site==0x262ec:
        for i,val in enumerate(saved_registers,25):cpu.reg_write(getattr(R,"UC_PPC_REG_"+str(i)),val)
    elif site==0x26208:
        label=bytes(cpu.mem_read(reg(6),32)).split(b"\0")[0].decode()
        messenger.update(create=[reg(3),reg(4),reg(5)-base[1],label],created=None)
        cpu.reg_write(R.UC_PPC_REG_3,msg if messenger_case else 0)
    elif site==0x26284:
        assert reg(3)==msg
        put(msg+64,"I",extra)
        messenger["created"]=dict(position=list(get(mwork+20,"fff")),rotation=list(get(mwork+8,"iii")),
            group=get(mwork+32,"f")[0],parameter=get(mwork+36,"f")[0],taskWord=get(msg+76,"I")[0],initialize=0x25cac)
    else:
        messenger["created"].update(extraWord80=get(extra+80,"I")[0],extraFloat48=get(extra+48,"f")[0],
            mode=get(mwork,"b")[0],extraPosition=list(get(extra,"fff")),finalCall=[reg(i) for i in range(3,7)])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x26030,0x262ec,0x26208,0x26284,0x262d4):
    u.hook_add(UC_HOOK_CODE,inspect_messenger,begin=base[1]+site,end=base[1]+site)
msg,mwork,extra=0x100900,0x100a00,0x100b00
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for messenger_case in (False,True):
        for b in (6,7):
            for query in (1,2,3):
                draw=123;ops=[];messenger={}
                u.mem_write(mwork,bytes([0xcd])*96);u.mem_write(extra,bytes([0xcd])*96);put(msg+52,"I",mwork)
                put(work,"bbhhh",1,0,361,360,-1);put(player1+3,"b",6);put(player0+1,"b",b)
                for r,v in [(1,0x100e00),(26,player1),(29,work),(31,player0)]:u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
                u.emu_start(base[1]+0x924,base[1]+0x1424,count=350);assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
                c=dict(mode=1,engine_bytes=[0,0,0],player_flags=[0,0],state_one=dict(counter=361,limit=360,submode=0,
                    character=6,slot0_byte=b,query=query,draw=draw,messenger_allocated=messenger_case))
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                assert json.loads(out.read_text())["controller"]["messenger"]==messenger
print(idx,"opening messenger creations matched")

run_native_decision=False
def steering_call(cpu,pc,*_):
    site=pc-base[1]
    if site==0xae0:
        assert [cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i))) for i in (3,4,5)]==[work,pose0,pose1]
        if run_native_decision:return
        cpu.reg_write(R.UC_PPC_REG_3,decision&0xffffffff)
    elif site==0xbb0:
        turn_args.extend(struct.unpack(">i",cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i))).to_bytes(4,"big"))[0] for i in (3,4,5))
        cpu.reg_write(R.UC_PPC_REG_3,2468)
    else:
        args.extend(struct.unpack(">d",cpu.reg_read(r).to_bytes(8,"big"))[0] for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2))
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",angle),"big"))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0xae0,0xb04,0xb84,0xbb0):
    u.hook_add(UC_HOOK_CODE,steering_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for mode in (2,10):
        for decision in (-1,0,1):
            for slot,angle in [(6,-2.7),(7,1.3)]:
                args=[];turn_args=[]
                put(pose0+20,"fff",*p0);put(pose1+20,"fff",*p1)
                put(pose1+12,"i",-123);put(player1+1,"b",slot)
                for r,v in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),v)
                u.emu_start(base[1]+0xad4,base[1]+0xbd0,count=100)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xbd0
                v=dict(decision=decision,slot0=p0,slot1=p1,angle_return=angle,slot_byte=slot)
                if not decision:v["turn"]=dict(previous_yaw=-123,result=2468)
                c=dict(mode=mode,engine_bytes=[0,0,0],player_flags=[0,0],state_two=v)
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                yaw,scalar=get(0x801e5000,"if")
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0xad4,continueAt=0xbd0,mode=mode,
                    angleArgs=args,turnArgs=turn_args or None,yaw=yaw,scalar=scalar)
print(idx,"state 2/10 steering paths matched")

run_native_decision=True
def decision_call(cpu,pc,*_):
    site=pc-base[1]
    reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x1d4:
        trace["distanceArgs"]=[list(get(reg(i),"fff")) for i in (3,4)]
        value=distance
    elif site==0x228:
        trace["angleArgs"]=[struct.unpack(">d",reg("FPR"+str(i)).to_bytes(8,"big"))[0] for i in (1,2)]
        value=helper_angle
    elif site==0x250:
        trace["differenceArgs"]=[struct.unpack(">i",reg(i).to_bytes(4,"big"))[0] for i in (3,4)]
        cpu.reg_write(R.UC_PPC_REG_3,diff&0xffffffff)
    else:
        trace["direct"]=bool(reg(3))
        return
    if site!=0x250:cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",value),"big"))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1d4,0x228,0x250,0xae4):
    u.hook_add(UC_HOOK_CODE,decision_call,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for byte in (6,7):
        for distance in (99.5,100.,101.):
            for diff in (-1,0x2fff,0x3000,0x3001):
                args=[];turn_args=[];trace=dict(angleArgs=None,differenceArgs=None);helper_angle=-0.75;angle=1.3
                put(player0+1,"b",byte);put(player1+1,"b",6);put(pose1+12,"i",-123)
                for r,vv in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),vv)
                u.emu_start(base[1]+0xad4,base[1]+0xbd0,count=160)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xbd0
                dec=dict(slot0_byte=byte,distance_return=distance)
                if trace["differenceArgs"] is not None:dec["facing"]=dict(angle_return=helper_angle,previous_yaw=-123,difference_return=diff)
                v=dict(decision=dec,slot0=p0,slot1=p1,angle_return=angle,slot_byte=6)
                if turn_args:v["turn"]=dict(previous_yaw=-123,result=2468)
                c=dict(mode=2,engine_bytes=[0,0,0],player_flags=[0,0],state_two=v)
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                yaw,scalar=get(0x801e5000,"if")
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0xad4,continueAt=0xbd0,mode=2,
                    angleArgs=args,turnArgs=turn_args or None,yaw=yaw,scalar=scalar,decision=trace)
print(idx,"integrated walker steering decisions matched")

# Run the steering block together with its following range, timer and pulse branches.
run_native_decision=False
def range_service(cpu,pc,*_):
    site=pc-base[1]
    if site==0x294:
        assert [cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i))) for i in (3,4)]==[pose0+20,pose1+20]
        observed["distance"]=True
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",dist),"big"))
    else:
        observed["draw"]=True
        cpu.reg_write(R.UC_PPC_REG_3,draw&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x294,0xc60,0xccc):
    u.hook_add(UC_HOOK_CODE,range_service,begin=base[1]+site,end=base[1]+site)
expand_close=False
expand_expiry=False
def stop_at_branch(cpu,pc,*_):
    if not (expand_close if pc==base[1]+0x640 else expand_expiry):cpu.emu_stop()
for site in (0x640,0xd48):u.hook_add(UC_HOOK_CODE,stop_at_branch,begin=base[1]+site,end=base[1]+site)
cases=[(6,6,1,4.99,200,1,0,0)]
cases += [(6,slot0,1,5.,dist,1,0,0) for slot0 in (6,7) for dist in (69.9,70.,70.1,99.9,100.)]
cases += [(slot,6,0,0.,120.,ctr,limit,draw) for slot,ctr in ((6,-32),(7,64),(7,32),(-1,0))
          for limit in (ctr-1,ctr) for draw in (0,16384,26214,26215)]
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,(slot,slot0,pose,scalar,dist,ctr,limit,draw) in enumerate(cases):
        mode=(2,10)[idx%2];decision=1;angle=-0.25;args=[];turn_args=[];observed={}
        put(work,"bbhh",mode,0,ctr,limit);put(player0+1,"b",slot0);put(player1+1,"b",slot)
        put(pose1+6,"H",pose);put(player1+72,"f",scalar)
        for addr in pulse_bytes:put(addr,"B",0)
        for r,vv in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work)]:
            u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),vv)
        u.emu_start(base[1]+0xad4,base[1]+0x1424,count=250)
        dest=u.reg_read(R.UC_PPC_REG_PC)-base[1];assert dest in (0x640,0xd48,0x1424)
        a=dict(pose_word=pose,scalar=scalar,counter=ctr,limit=limit)
        if "distance" in observed:a["range"]=dict(slot0_byte=slot0,distance_return=dist)
        if "draw" in observed:a["draw"]=draw
        v=dict(decision=1,slot0=p0,slot1=p1,angle_return=angle,slot_byte=slot,after_steering=a)
        c=dict(mode=mode,engine_bytes=[0,0,0],player_flags=[0,0],state_two=v)
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        yaw,scalar_out=get(0x801e5000,"if")
        assert json.loads(out.read_text())["controller"]==dict(dispatch=0xad4,continueAt=dest,mode=get(work,"b")[0],
            angleArgs=args,turnArgs=None,yaw=yaw,scalar=scalar_out,counter=get(work+2,"h")[0],
            byteWrites=[addr for addr in pulse_bytes if get(addr,"B")== (1,)])
print(len(cases),"post-steering range and timer paths matched")

expand_close=True
def close_service(cpu,pc,*_):
    site=pc-base[1];r3=cpu.reg_read(R.UC_PPC_REG_3);r4=cpu.reg_read(R.UC_PPC_REG_4)
    calls.append(dict(engine=engines[site],args=[r3] if site==0x6a4 else [r3,r4]))
    if site==0x694:cpu.reg_write(R.UC_PPC_REG_3,queries[r4]&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x694,0x6a4,0x6ec,0x6fc,0x71c,0x72c):
    u.hook_add(UC_HOOK_CODE,close_service,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for slot,variant in [(6,62),(6,61),(7,61),(7,-1),(-1,62)]:
        for cool,queries in [(1,None),(0,[1,0]),(-1,[2,1]),(0,[1,1])]:
            mode=(2,10)[idx%2];calls=[];observed={};args=[];decision=1;angle=0.5;dist=60.
            put(work,"bbhhh",mode,0,55,22,cool);put(player0+1,"b",6);put(player1+1,"b",slot)
            put(player1+4,"b",variant);put(pose1+6,"H",0)
            for r,val in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work)]:
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
            u.emu_start(base[1]+0xad4,base[1]+0x1424,count=300)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
            close=dict(cooldown=cool)
            if queries is not None:close["services"]=dict(queries=queries,slot_byte=slot,variant=variant)
            a=dict(pose_word=0,scalar=0.,counter=55,limit=22,range=dict(slot0_byte=6,distance_return=dist),close_range=close)
            v=dict(decision=1,slot0=p0,slot1=p1,angle_return=angle,slot_byte=slot,after_steering=a)
            c=dict(mode=mode,engine_bytes=[0,0,0],player_flags=[0,0],state_two=v)
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
            out=Path(tmp)/f"{idx}.json";idx+=1
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            report=json.loads(out.read_text())["controller"]
            assert (report["mode"],report["counter"],report["continueAt"])==(get(work,"b")[0],get(work+2,"h")[0],0x1424)
            assert report["closeRange"]==dict(cooldown=get(work+6,"h")[0],calls=calls)
print(idx,"close-range state-five entries matched")

# Execute state 5 including its range, decision and transition helpers.
def five_service(cpu,pc,*_):
    site=pc-base[1]
    if site==0x100c:
        vals=[struct.unpack(">d",cpu.reg_read(r).to_bytes(8,"big"))[0] for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2)]
        observed["angle"]=vals
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",angle),"big"))
    else:
        observed["pulse" if site in (0x10ac,0x1118) else "timer"]=True
        cpu.reg_write(R.UC_PPC_REG_3,(pulse_draw if site in (0x10ac,0x1118) else draw)&0xffffffff)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x100c,0x10ac,0x1118,0x338):
    u.hook_add(UC_HOOK_CODE,five_service,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for slot in (6,7,-1):
        for ctr,pulse_draw in [(0,0),(32,32767),(64,32767),(65,0),(128,0),(128,32767)]:
            for dist,flag in [(70.,1),(100.,0),(101.,1)]:
                word=idx%4;angle=-0.71;draw=21000;args=[];calls=[];ops=[];observed={}
                put(work,"bbhh",5,0,ctr,-1);put(player0+28,"h",word);put(player0+1,"b",7)
                put(player1+1,"b",slot);put(0x801e738c,"I",task);put(task+64,"I",0)
                put(0x803ad494,"b",flag);put(0x801e5000,"if",-999,-999.)
                for addr in pulse_bytes:put(addr,"B",0)
                for r,val in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work),(31,player0)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
                u.emu_start(base[1]+0xfe4,base[1]+0x1424,count=320)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
                m,_,counter,timer=get(work,"bbhh")
                yaw,scalar=get(0x801e5000,"if")
                v=dict(player_word=word,slot0=p0,slot1=p1,range=dict(slot0_byte=7,distance_return=dist),slot_byte=slot,counter=ctr)
                if "angle" in observed:v["angle_return"]=angle
                else:assert (yaw,scalar)==(-999,-999.)
                if dist>=100:v["decision"]=dict(work_word=None,engine_byte=flag)
                if "pulse" in observed:v["pulse_draw"]=pulse_draw
                if m!=5:v["timer_draw"]=draw
                c=dict(mode=5,engine_bytes=[0,0,0],player_flags=[0,0],state_five=v)
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0xfe4,continueAt=0x1424,mode=m,counter=counter,
                    timerWrite=timer if m!=5 else None,steering=dict(angleArgs=observed["angle"],yaw=yaw,scalar=scalar) if word else None,
                    byteWrites=[addr for addr in pulse_bytes if get(addr,"B")== (1,)])
print(idx,"state-five updates matched")

# The two axis calls independently sample the same engine word.
phase_addr=0x801cbd88+0x7c
reg_float=lambda cpu,r:struct.unpack(">d",cpu.reg_read(r).to_bytes(8,"big"))[0]
def four_service(cpu,pc,*_):
    site=pc-base[1]
    if site in (0xf34,0xf58):
        idx_axis=int(site==0xf58)
        axis_args.append(cpu.reg_read(R.UC_PPC_REG_3))
        value=axis_returns[idx_axis]
        put(phase_addr,"I",phase_words[1])
    else:
        angle_args.extend(reg_float(cpu,r) for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2))
        put(player1+1,"b",final_slot)
        value=angle
    cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",value),"big"))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0xf34,0xf58,0xf7c):
    u.hook_add(UC_HOOK_CODE,four_service,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for slot in (6,7,-1):
        for dist,scalar,ctr,limit in [(70.,0.,10,0),(100.,0.,10,0),(101.,20.,65,64),(101.,20.,64,64),(101.,20.,-1,0)]:
            for final_slot in (6,7):
                calls=[];observed={};axis_args=[];angle_args=[];draw=19531;angle=-1.34
                phase_words=[0xfabc0123,0x80000000];axis_returns=[-0.3125,0.8125]
                put(phase_addr,"I",phase_words[0]);put(work,"bbhhh",4,0,ctr,limit,5)
                put(player0+1,"b",7);put(player1+1,"b",slot);put(player1+72,"f",scalar)
                put(0x801e5000,"if",-777,-777.)
                for r,val in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work),(31,player0)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
                u.emu_start(base[1]+0xe9c,base[1]+0x1424,count=300)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
                m,_,counter,timer,cool=get(work,"bbhhh");yaw,out_scalar=get(0x801e5000,"if")
                v=dict(range=dict(slot0_byte=7,distance_return=dist),slot_byte=slot,scalar=scalar,counter=ctr,limit=limit)
                if m==5:v["close_range"]=dict(cooldown=5)
                if m==2:v["draw"]=draw
                if axis_args:v["steering"]=dict(slot0=p0,slot1=p1,phase_words=phase_words,axis_returns=axis_returns,angle_return=angle,final_slot_byte=final_slot)
                else:assert (yaw,out_scalar)==(-777,-777.)
                c=dict(mode=4,engine_bytes=[0,0,0],player_flags=[0,0],state_four=v)
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0xe9c,continueAt=0x1424,mode=m,counter=counter,
                    timerWrite=timer if m==2 else None,closeRange=dict(cooldown=cool,calls=calls) if m==5 else None,
                    steering=dict(axisArgs=axis_args,angleArgs=angle_args,yaw=yaw,scalar=out_scalar) if axis_args else None)
print(idx,"state-four updates matched")

def six_service(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x1258:
        assert reg(3)==reg(1)+16 and reg(4)==pose1+20
        seen["vectorArgs"]=[list(get(reg(i),"fff")) for i in (3,4)]
    elif site==0x1264:assert reg(3)==0
    elif site==0x1270:assert (reg(3),reg(4))==(0,0x4000)
    elif site==0x1280:assert (reg(3),reg(4),reg(5))==(0,reg(1)+16,reg(1)+16)
    elif site==0x128c:
        assert reg(3)==reg(1)+16
        put(reg(3),"fff",*end_vector)
    elif site==0x1298:
        seen["angleArgs"]=[reg_float(cpu,r) for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2)]
        put(player1+1,"b",final_slot)
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",angle),"big"))
    elif site==0x12fc:
        seen["waveArg"]=reg(3)
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",wave),"big"))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1258,0x125c,0x1264,0x1270,0x1280,0x1284,0x128c,0x1298,0x12fc):
    u.hook_add(UC_HOOK_CODE,six_service,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for slot in (6,7,-1):
        for ctr,limit,scalar,dist in [(1,0,-1.,50.),(0,0,-1.,50.),(0,0,20.,99.),(-1,0,20.,100.)]:
            for final_slot in (6,7):
                observed={};seen={};calls=[];draw=21234;phase=0xffaabbcc;wave=-0.3125;angle=1.45;end_vector=[1.5,-2.25,6.]
                put(phase_addr,"I",phase);put(work,"bbhh",6,0,ctr,limit);put(player0+1,"b",7)
                put(player1+1,"b",slot);put(player1+72,"f",scalar);put(0x801e5000,"if",-999,-999.)
                for r,val in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work),(31,player0)]:
                    u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
                u.emu_start(base[1]+0x11a8,base[1]+0x1424,count=220)
                assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
                m,_,counter,timer=get(work,"bbhh");yaw,scalar_out=get(0x801e5000,"if")
                v=dict(counter=ctr,limit=limit,slot_byte=slot,scalar=scalar)
                if "distance" in observed:v["range"]=dict(slot0_byte=7,distance_return=dist)
                if m==2:v["timer"]=dict(slot_byte=slot,draw=draw)
                if m==6:
                    h=dict(slot0=p0,slot1=p1,final_vector=end_vector,angle_return=angle,final_slot_byte=final_slot)
                    if "waveArg" in seen:h["wave"]=dict(phase_word=phase,service_return=wave)
                    v["steering"]=h
                else:assert (yaw,scalar_out)==(-999,-999.)
                c=dict(mode=6,engine_bytes=[0,0,0],player_flags=[0,0],state_six=v)
                req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
                out=Path(tmp)/f"{idx}.json";idx+=1
                subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
                if m==6:seen.update(yaw=yaw,scalar=scalar_out);seen.setdefault("waveArg",None)
                assert json.loads(out.read_text())["controller"]==dict(dispatch=0x11a8,continueAt=0x1424,mode=m,counter=counter,
                    timerWrite=timer if m==2 else None,steering=seen if m==6 else None)
print(idx,"state-six updates matched")

expand_expiry=True
def expiry_draw(cpu,pc,*_):
    site=pc-base[1];value={0xd70:first,0xddc:second,0xe2c:third}[site]
    draws.append(value)
    if site==0xddc:put(player1+1,"b",second_byte)
    cpu.reg_write(R.UC_PPC_REG_3,value&0xffffffff);cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0xd70,0xddc,0xe2c):u.hook_add(UC_HOOK_CODE,expiry_draw,begin=base[1]+site,end=base[1]+site)
def record_expiry_byte(cpu,pc,*_):bytes_read.append(get(player1+1,"b")[0])
for site in (0xdcc,0xe1c,0x328,0x438):
    u.hook_add(UC_HOOK_CODE,record_expiry_byte,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    idx=0
    for slot,second_byte in ((6,6),(6,7),(7,7),(-1,-1)):
        for flag,first,second,third in [(1,0,0,0),(0,6553,0,0),(0,6554,29491,16383),(0,6554,29492,16384),(0,32767,0,16384)]:
            mode=(2,10)[idx%2];decision=1;dist=150.;angle=0.3;draw=18921
            calls=[];ops=[];observed={};args=[];draws=[];bytes_read=[]
            put(work,"bbhhh",mode,0,5,4,13);put(player0+1,"b",6);put(player1+1,"b",slot)
            put(pose1+6,"H",0);put(0x801e738c,"I",task);put(task+64,"I",0);put(0x803ad494,"b",flag)
            for r,val in [(1,0x100e00),(26,player1),(27,pose1),(28,pose0),(29,work),(31,player0)]:
                u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
            u.emu_start(base[1]+0xad4,base[1]+0x1424,count=480)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1424
            m,_,counter,timer,cool=get(work,"bbhhh")
            if m in (4,6):draws.append(draw)
            exp=dict(decision=dict(work_word=None,engine_byte=flag),draws=draws,slot_bytes=bytes_read)
            a=dict(counter=5,limit=4,scalar=20.,pose_word=0,range=dict(slot0_byte=6,distance_return=dist),expiry=exp)
            if m==5:a["close_range"]=dict(cooldown=13)
            v=dict(slot0=p0,slot1=p1,slot_byte=slot,angle_return=angle,decision=1,after_steering=a)
            c=dict(mode=mode,engine_bytes=[0,0,0],player_flags=[0,0],state_two=v)
            req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],controller=c)))
            out=Path(tmp)/f"{idx}.json";idx+=1
            subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
            rep=json.loads(out.read_text())["controller"]
            assert (rep["mode"],rep["counter"],rep["timerWrite"],rep["continueAt"])==(m,counter,timer if m in (4,6) else None,0x1424)
            if m==5:assert rep["closeRange"]==dict(cooldown=cool,calls=calls)
print(idx,"integrated expiry transitions matched")

# The tail's first local helper works on a copy, even when all three axes are out.
def capture_correction(cpu,pc,*_):
    corrections.append(dict(engine=engines[pc-base[1]],slot=cpu.reg_read(R.UC_PPC_REG_3),
                            position=[reg_float(cpu,r) for r in (R.UC_PPC_REG_FPR1,R.UC_PPC_REG_FPR2,R.UC_PPC_REG_FPR3)]))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
u.hook_add(UC_HOOK_CODE,capture_correction,begin=base[1]+0x13c,end=base[1]+0x13c)
from itertools import product
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,pos in enumerate(product((-176.,-175.,15.,205.,206.),(-341.,-340.,-170.,-70.,-69.),(-321.,-320.,-20.,280.,281.))):
        corrections=[];put(pose1+20,"fff",*pos)
        for r,val in [(1,0x100e00),(3,work),(4,pose1)]:
            u.reg_write(getattr(R,"UC_PPC_REG_"+str(r)),val)
        u.reg_write(R.UC_PPC_REG_LR,stop);u.emu_start(base[1],stop,count=120)
        assert u.reg_read(R.UC_PPC_REG_PC)==stop and get(pose1+20,"fff")==pos and len(corrections)<=1
        result=u.reg_read(R.UC_PPC_REG_3)
        tail=dict(mode=5,submode=0,counter=0,cooldown=0,slot0_flags=0,queries=[0,0,0],
                  engine_word=1,slot1_value=0.,child=False,slot1_position=pos)
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],tail=tail)))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        assert json.loads(out.read_text())["tail"]["arenaBounds"]==dict(result=result,call=next(iter(corrections),None))
print(idx+1,"arena boundary positions matched")

# Relocate section-six BSS references used by the startup's SET handle store.
layout_bss=0x81e00000
u.mem_map(layout_bss,4096)
put(base[1]+0x17e6,"H",(layout_bss+32768)>>16)
put(base[1]+0x17ea,"H",layout_bss&65535)
def capture_layout(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x1778:args=[reg(3),reg(4)-base[5]]
    elif site==0x17e0:
        args=[bytes(cpu.mem_read(reg(i),32)).split(b"\0")[0].decode() for i in (3,4)]+[reg(5)]
        cpu.reg_write(R.UC_PPC_REG_3,handle)
    elif site==0x17fc:args=[reg(3)-base[5],reg(4)]
    else:args=[reg(i)-base[5] for i in (3,4)]
    layout_calls.append(dict(site=site,engine=engines[site],args=args))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1778,0x1794,0x17a0,0x17bc,0x17c8,0x17e0,0x17fc):
    u.hook_add(UC_HOOK_CODE,capture_layout,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,handle in enumerate((0,0x12345678,0xffffffff)):
        layout_calls=[];put(layout_bss,"I",~handle&0xffffffff)
        u.emu_start(base[1]+0x176c,base[1]+0x1800,count=100)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1800
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],
            startup=dict(character_queries=[7],slot_one_present=False,layout_handle=handle))))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        result=json.loads(out.read_text())["startup"]
        assert result["continueAt"]==0x1800 and result["layoutSetup"]==dict(operations=layout_calls,
            storedHandle=dict(section=6,offset=0,value=get(layout_bss,"I")[0]))
print("3 startup layout traces matched")

def capture_scene(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    name=lambda ptr:bytes(cpu.mem_read(ptr,32)).split(b"\0")[0].decode()
    if site==0x18b4:
        dest=site+(get(pc,"I")[0]&0x03fffffc)
        scene_calls.append(dict(site=site,local=dest,args=[name(reg(3))]))
    else:
        if site in (0x1800,0x1804):args=[]
        elif site in (0x1824,0x1838,0x1848):args=[reg_float(cpu,getattr(R,"UC_PPC_REG_FPR"+str(i))) for i in range(1,3 if site==0x1838 else 4)]
        elif site in (0x185c,0x1868,0x1874,0x1880):args=[reg(3),reg(4)-base[5]]
        elif site in (0x188c,0x1898):args=[reg(3),reg(4)]
        elif site in (0x18a0,0x18a8):args=[reg(3)]
        else:
            args=[reg(3),reg(4),reg(5)-base[1],name(reg(6))]
            cpu.reg_write(R.UC_PPC_REG_3,msg if allocated else 0)
        scene_calls.append(dict(site=site,engine=engines[site],args=args))
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x1800,0x1804,0x1824,0x1838,0x1848,0x185c,0x1868,0x1874,0x1880,0x188c,0x1898,0x18a0,0x18a8,0x18b4,0x18d0):
    u.hook_add(UC_HOOK_CODE,capture_scene,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for idx,allocated in enumerate((False,True)):
        scene_calls=[];layout_calls=[];handle=123
        for off in (20,32,24):put(msg+off,"I",0)
        u.emu_start(base[1]+0x176c,base[1]+0x1900,count=160)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x1900
        req=Path(tmp)/"request.json";req.write_text(json.dumps(dict(mode=0,position=[0,0,0],targets=[],
            startup=dict(character_queries=[7],slot_one_present=False,layout_handle=handle,background_allocated=allocated))))
        out=Path(tmp)/f"{idx}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage20-entry",str(path),str(out),str(req)],check=True)
        result=json.loads(out.read_text())["startup"]
        callbacks=[[off,get(msg+off,"I")[0]-base[1]] for off in (20,32,24) if get(msg+off,"I")[0]]
        assert result["continueAt"]==0x1900 and result["sceneSetup"]==dict(operations=scene_calls,callbacks=callbacks)
print("2 startup lighting and background task paths matched")

# Run the duel loader's derivation loop against each installed base table.
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    sources=sorted((game/"resource/gd_PC").glob("*_light.bin"))
    assert len(sources)==52
    for light in sources:
        raw=light.read_bytes();assert len(raw)==384
        src,dst=0x801e5470,0x801e5230
        u.mem_write(src,raw);u.mem_write(dst,b"\xa5"*576)
        u.reg_write(R.UC_PPC_REG_3,src);u.reg_write(R.UC_PPC_REG_30,dst)
        u.emu_start(base[1]+0x7c18,base[1]+0x7d64,count=500)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x7d64
        out=Path(tmp)/(light.stem+".json")
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage-light-base",str(light),str(out)],check=True)
        assert json.loads(out.read_text())["derivedWords"]==[list(get(dst+48*i,"9I")) for i in range(12)]
        assert all(bytes(u.mem_read(dst+48*i+36,12))==b"\xa5"*12 for i in range(12))
print(len(sources),"installed light tables matched the stage 20 PPC loop")

# Compare the destination after the first-row clear, derivation and optional GC copy.
def light_io(cpu,pc,*_):
    site=pc-base[1];reg=lambda i:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
    if site==0x7c08:
        assert (reg(3),reg(4),reg(5))==(dst,0,48)
        cpu.mem_write(dst,bytes(48))
    elif site==0x7d70:cpu.reg_write(R.UC_PPC_REG_3,len(gc_data))
    else:
        assert (reg(3),reg(4),reg(5))==(dst,0x80fffe60,len(gc_data))
        cpu.mem_write(dst,gc_data)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x7c08,0x7d70,0x7d8c):
    u.hook_add(UC_HOOK_CODE,light_io,begin=base[1]+site,end=base[1]+site)
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    hits=0
    overrides=list((game/"resource/gd_PC").glob("*_light_gc.bin"))
    standalone=[g for g in overrides if not g.with_name(g.name.replace("_gc.bin",".bin")).exists()]
    assert len(standalone)==4
    for light in [*sources,*standalone]:
        gc_file=light.with_name(light.stem+"_gc.bin")
        if light in standalone:gc_file=light
        gc_data=gc_file.read_bytes() if gc_file.exists() else b""
        u.mem_write(src,bytes(384) if light in standalone else light.read_bytes());u.mem_write(dst,b"\xa5"*576)
        u.reg_write(R.UC_PPC_REG_1,0x100e00)
        u.emu_start(base[1]+0x7bf4,base[1]+0x7d9c,count=600)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x7d9c
        loaded=bool(u.reg_read(R.UC_PPC_REG_3));hits+=loaded
        out=Path(tmp)/(light.stem+".json")
        subprocess.run([sys.argv[1],"--source-root",str(game),"stage-light-table",str(light),str(out)],check=True)
        rep=json.loads(out.read_text());assert rep["overrideLoaded"]==loaded
        for idx,row in enumerate(rep["activeWords"]):
            assert len(row)==12 and [0xa5a5a5a5 if x is None else x for x in row]==list(get(dst+48*idx,"12I"))
            assert sum(x is None for x in row)==(3 if not loaded and idx else 0)
    assert hits==18
print(hits,"overrides and",len(sources)+len(standalone)-hits,"base fallbacks matched the stage 20 loader")
