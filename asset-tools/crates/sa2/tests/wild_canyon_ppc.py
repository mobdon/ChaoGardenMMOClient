"""Compare the mounted Hero #5 stage task with the installed Wild Canyon PPC callback."""
import hashlib,json,os,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

game=Path(os.environ["SA2_GAME_PATH"])
d=(game/"resource/gd_PC/stg16D.rel").read_bytes()
assert hashlib.sha256(d).hexdigest()=="5078ec861c527beaee651cc0272fbfba36793b0afb8f6c132c12718ffdf9282d"
w=lambda i:struct.unpack_from(">I",d,i)[0]
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={s:0x81000000+s*0x200000 for s in (1,4,5,6)}
for s,b in base.items():
    row=w(16)+8*s;at=w(row)&~1;n=w(row+4)
    u.mem_map(b,(n+4095)&~4095);
    if s!=6:u.mem_write(b,d[at:at+n])
def put(at,fmt,*v):u.mem_write(at,struct.pack(">"+fmt,*v))
def get(at,fmt):return struct.unpack(">"+fmt,u.mem_read(at,struct.calcsize(">"+fmt)))
engines={}
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;off=0;continue
        off+=inc
        if mod==0 and target==1 and k==10:engines[off]=a
        if mod==0 and target==1 and k in (4,6) and (0xac90<=off<=0xaeec or 0x8c68<=off<0x8cb0 or 0x20a68<=off<0x20a70 or 0x1fae8<=off<0x1faf0):v=a
        elif mod==15 and s in base and target in base:v=base[s]+a
        else:continue
        if k==1:put(base[target]+off,"I",v)
        if k in (4,6):put(base[target]+off,"H",(v if k==4 else (v+32768)>>16)&65535)
stage=0x100000;child=stage+256;work=child+128;stop=stage+1024
u.mem_map(stage,4096);u.reg_write(R.UC_PPC_REG_MSR,8192)
def reg(i):return u.reg_read(getattr(R,"UC_PPC_REG_"+str(i)))
def text(ptr):return bytes(u.mem_read(ptr,32)).split(b"\0")[0].decode()
def hook(uc,pc,size,_):
    if pc==base[1]+0x8ac4:
        desc=reg(3);callbacks=[v-base[1] for v in get(desc+12,"III")]
        result.update(descriptor=text(desc),startup=callbacks[0],teardown=callbacks[1],entry=callbacks[2])
    else:
        result["create"]=dict(arguments=[reg(3),reg(4)],name=text(reg(6)),main=reg(5)-base[1])
        uc.reg_write(R.UC_PPC_REG_3,child if allocated else 0)
    uc.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in (0x8ac4,0x916c):u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+site,end=base[1]+site)
def invoke(off):
    u.reg_write(R.UC_PPC_REG_1,stage+4000);u.reg_write(R.UC_PPC_REG_LR,stop);u.reg_write(R.UC_PPC_REG_3,stage)
    u.emu_start(base[1]+off,stop,count=200);assert u.reg_read(R.UC_PPC_REG_PC)==stop
def startup(entry):
    calls=[];groups=[]
    def watch(uc,pc,size,_):
        site=pc-base[1]
        if site==0x8afc:args=[reg(i) for i in (3,4,5)]
        elif site in (0x8b18,0x8c64):args=[reg(3),reg(4),reg(5)-base[1],text(reg(6))]
        elif site==0x8b24:args=[reg(3)-base[5]]
        elif site==0x8b80:
            args=[reg(3),reg(4)];uc.reg_write(R.UC_PPC_REG_3,handle)
        elif site==0x8b9c:
            assert reg(4)==handle and get(base[6]+20,"I")[0]==handle
            args=[reg(3)-base[5],dict(returnFrom=0x8b80,bssOffset=20)]
        elif site in (0x8ba0,0x8ba4):args=[]
        elif site in (0x8bc4,0x8bd8,0x8be8):
            count=2 if site==0x8bd8 else 3
            vals=[struct.unpack(">d",struct.pack(">Q",uc.reg_read(getattr(R,"UC_PPC_REG_FPR"+str(i)))))[0] for i in range(1,count+1)]
            calls.append(dict(engine=engines[site],floats=vals));uc.reg_write(R.UC_PPC_REG_PC,pc+4);return
        elif site in (0x8bfc,0x8c08,0x8c14,0x8c20):args=[reg(3),reg(4)-base[5]]
        elif site in (0x8c2c,0x8c38):args=[reg(3),reg(4)]
        elif site in (0x8c40,0x8c48):args=[reg(3)]
        else:
            tex,obj=reg(3),reg(4);args=[tex-base[5],obj-base[5]]
            if site in (0x8b40,0x8b68):
                rows=[];entries=[]
                while True:
                    label,lst=get(tex+8*len(rows),"II")
                    if not label:break
                    rows.append(dict(name=text(label),textureList=lst-base[5]))
                while (ptr:=get(obj+4*len(entries),"I")[0]):entries.append(ptr-base[5])
                groups.append(dict(textures=args[0],objects=args[1],textureRows=rows,objectOffsets=entries))
        calls.append(dict(engine=engines[site],args=args))
        uc.reg_write(R.UC_PPC_REG_PC,pc+4)
    hooks=[u.hook_add(UC_HOOK_CODE,watch,begin=base[1]+s,end=base[1]+s) for s in engines if entry<=s<0x8c68]
    hooks.append(u.hook_add(UC_HOOK_CODE,lambda cpu,*_:cpu.emu_stop(),begin=base[1]+0x8c68,end=base[1]+0x8c68))
    try:
        u.reg_write(R.UC_PPC_REG_1,stage+4000)
        u.emu_start(base[1]+entry,base[1]+0x8c68,count=200)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x8c68 and len(calls)==23,(allocated,hex(u.reg_read(R.UC_PPC_REG_PC)),len(calls))
    finally:
        for h in hooks:u.hook_del(h)
    return dict(calls=calls,groups=groups)
def lighting(flag):
    gd=game/"resource/gd_PC"
    src,dst=2149471344,2149470768
    state={};music={};rows={}
    def watch(uc,pc,size,_):
        site=pc-base[1];r3,r4,r5=map(reg,(3,4,5))
        if site in (0xacb4,0xacc0):
            name=text(r4);state["lightFile"]=name;uc.mem_write(r3,name.encode()+bytes(1))
        elif site==0xaccc:uc.reg_write(R.UC_PPC_REG_3,r3+text(r3).index(chr(r4)))
        elif site==0xacec:
            suffix=text(r4);state["gcSuffix"]=suffix;uc.mem_write(r3,(text(r3)+suffix).encode()+bytes(1))
        elif site in (0xad04,0xad44):uc.mem_write(r3,bytes([r4])*r5)
        elif site in (0xad14,0xaeac):
            if site==0xaeac:rows["derivedWords"]=[list(get(dst+j*48,"9I")) for j in range(12)]
            payload=(gd/text(r3)).read_bytes();uc.mem_write(r4,payload);uc.reg_write(R.UC_PPC_REG_3,len(payload))
        elif site in (0xad2c,0xaec8):uc.mem_write(r3,bytes(uc.mem_read(r4,r5)))
        elif site==0x8c78:music["volumeArgument"]=r3
        else:music["engineByteZero" if not flag else "engineByteNonzero"]=text(r3)
        uc.reg_write(R.UC_PPC_REG_PC,pc+4)
    sites=(0xacb4,0xacc0,0xaccc,0xacec,0xad04,0xad44,0xad14,0xaeac,0xad2c,0xaec8,0x8c78,0x8c9c,0x8cac)
    hooks=[u.hook_add(UC_HOOK_CODE,watch,begin=base[1]+s,end=base[1]+s) for s in sites]
    try:
        put(2149367176,"B",flag);u.reg_write(R.UC_PPC_REG_1,stage+4000)
        u.emu_start(base[1]+0x8c68,base[1]+0x8cb0,count=1000)
        assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x8cb0
        rows["activeWords"]=[list(get(dst+j*48,"12I")) for j in range(12)]
    finally:
        for h in hooks:u.hook_del(h)
    return state,music,rows
for pg in {v&~4095 for v in (2149479272,2149471344,2149471344+383,2149470768,2149470768+575,2150329376,2149367176,0x80fffe60,0x80fffe60+575)}:u.mem_map(pg,4096)
def emerald_paths():
    u.reg_write(R.UC_PPC_REG_1,stage+4000)
    u.emu_start(base[1]+0x8e54,base[1]+0x8e64,count=30)
    table=get(base[6]+2832,"I")[0];count=get(base[5]+619596,"I")[0]
    assert count==12
    def hook(cpu,pc,size,_):
        if pc-base[1] in (0x21cdc,0x21d1c):
            bits=cpu.reg_read(R.UC_PPC_REG_FPR1)
            cpu.reg_write(R.UC_PPC_REG_3,int(struct.unpack(">d",bits.to_bytes(8,"big"))[0]))
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
    hooks=[u.hook_add(UC_HOOK_CODE,hook,begin=base[1]+s,end=base[1]+s) for s in (0x21ccc,0x21cdc,0x21d1c,0x21e28)]
    paths=[]
    try:
        for i in range(count):
            ptr=get(table+i*4,"I")[0];points=get(ptr+8,"I")[0]
            put(child+0x34,"I",work);put(work+32,"f",float(i+count))
            u.reg_write(R.UC_PPC_REG_1,stage+4000);u.reg_write(R.UC_PPC_REG_3,child)
            u.reg_write(R.UC_PPC_REG_4,stage+768);u.reg_write(R.UC_PPC_REG_LR,stop)
            u.emu_start(base[1]+0x21ca4,stop,count=6000);assert u.reg_read(R.UC_PPC_REG_PC)==stop
            paths.append(dict(headerOffset=ptr-base[5],pointsOffset=points-base[5],center=list(get(stage+768,"fff")),
                points=[list(get(points+12*j,"fff")) for j in range(get(ptr+2,"h")[0])]))
    finally:
        for h in hooks:u.hook_del(h)
    return paths
def emerald_tasks():
    expected=[]
    def intercept(cpu,pc,size,_):
        if pc==base[1]+0x1fd20:
            observed.append("query");cpu.reg_write(R.UC_PPC_REG_3,answer)
        elif pc==base[1]+0x207e0:
            setup.update(typeWord=reg(3),lowByte=reg(4),position=list(get(reg(5),"fff")))
        elif pc==base[1]+0x207f8:
            assert reg(3)==child
            cpu.reg_write(R.UC_PPC_REG_3,answer)
        elif pc==base[1]+0x20818:
            assert reg(3)==child
            setup.update(shapeOffset=reg(4)-base[5],count=reg(5),id=reg(6))
            put(work+0x2c,"I",stage+704);put(stage+716,"I",stage+800)
        else:
            assert reg(3)==child
            observed.append(cpu.reg_read(R.UC_PPC_REG_LR)-base[1])
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
    hooks=[u.hook_add(UC_HOOK_CODE,intercept,begin=base[1]+s,end=base[1]+s) for s in (0x1fd20,0x1fe34,0x207e0,0x207f8,0x20818)]
    try:
        for layer in ("set0016_s.bin","set0016_u.bin"):
            data=(game/"resource/gd_PC"/layer).read_bytes()
            for index in range(struct.unpack_from(">I",data)[0]):
                packed,rx=struct.unpack_from(">HH",data,32+32*index)
                obj=base[5]+0x7c6f8+(packed&4095)*16
                main,label=get(obj+8,"II");label=text(label)
                if label not in ("EMERALD","EMERALD_F"):continue
                row=dict(layer=layer,index=index,object=label,variant=(rx>>8)%11)
                for answer in (0,1):
                    observed=[];u.mem_write(child,bytes(128));put(child+0x34,"I",work);put(work+8,"I",rx)
                    u.reg_write(R.UC_PPC_REG_1,stage+4000);u.reg_write(R.UC_PPC_REG_3,child);u.reg_write(R.UC_PPC_REG_LR,stop)
                    u.emu_start(main,stop,count=160);assert u.reg_read(R.UC_PPC_REG_PC)==stop
                    update,display14,destroy,_,display20=get(child+16,"5I")
                    assert bool(display14)!=bool(display20)
                    if answer==0:
                        assert display20
                        row.update(main=update-base[1],destroy=destroy-base[1],display=display20-base[1],
                            defaultDisplaySlot=32,initialize=observed[-1])
                    else:
                        assert (display14 or display20)-base[1]==row["display"]
                        assert bool(display14)==("query" in observed)
                        row["nonzeroQueryDisplaySlot"]=20 if display14 else None
                row["commonSetup"]=None
                if row["initialize"]==0x20780:
                    for mode,answer in ((0,0),(0,1),(1,1)):
                        setup={};put(work,"B",mode);put(work+8,"I",rx)
                        xyz=struct.unpack_from(">fff",data,32+32*index+8)
                        put(work+20,"fff",*xyz);put(work+0x24,"f",struct.unpack_from(">f",data,32+32*index+24)[0])
                        u.reg_write(R.UC_PPC_REG_1,stage+4000);u.reg_write(R.UC_PPC_REG_3,child);u.reg_write(R.UC_PPC_REG_LR,stop)
                        u.emu_start(base[1]+row["initialize"],stop,count=110);assert u.reg_read(R.UC_PPC_REG_PC)==stop
                        if not mode and answer:assert "count" not in setup
                        else:
                            setup.update(shapeParameter=get(stage+820,"f")[0],skipIfModeZeroAndQueryNonzero=True)
                            if row["commonSetup"] is None:row["commonSetup"]=setup
                            else:assert row["commonSetup"]==setup
                row["pathIndex"]=(int(struct.unpack_from(">f",data,32+32*index+20)[0])%12 if row["initialize"]==0x220ac else None)
                expected.append(row)
    finally:
        for h in hooks:u.hook_del(h)
    return expected
def contact_cases(tasks,tmp):
    cases=[];expected=[]
    ccl=stage+1100;other=stage+1400;hit=stage+1800
    stops={0x20bf8:"continue",0x20ca8:"return"}
    def watch(cpu,pc,size,_):
        site=pc-base[1]
        if site in stops:
            row.setdefault("outcome",stops[site])
            return
        args=[site];value=0
        if site in (0x2097c,0x209e0,0x20a5c,0x208cc,0x20b38,0x1fb28,0x20b94):
            if site==0x209e0:args.extend(reg(i) for i in (3,4,5))
            value=answers[site];case["queries"].append(value)
        elif site==0x20c64:args.extend([reg(3),reg(4),list(get(reg(5),"fff"))])
        elif site==0x20c8c:assert reg(3)==child
        elif site==0x20bd0:
            args.extend([reg(3),reg(4),reg(5),list(get(reg(6),"fff"))]);row["outcome"]="pickupRequested"
        elif site in (0x20be4,0x20bf0):assert reg(3)==child
        elif site==0x208c0 and remaining:
            byte=remaining.pop(0);case["player_hits"].append(byte);put(hit+1,"b",byte);value=hit
        row["calls"].append(args);cpu.reg_write(R.UC_PPC_REG_3,value&0xffffffff)
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
    sites=[*stops,0x2097c,0x209e0,0x20a5c,0x2088c,0x208c0,0x208cc,0x20b38,0x1fb28,0x20b94,0x20bd0,0x20be4,0x20bf0,0x20c64,0x20c8c]
    hooks=[u.hook_add(UC_HOOK_CODE,watch,begin=base[1]+s,end=base[1]+s) for s in sites]
    try:
        for task in tasks:
            if task["main"]!=0x20940:continue
            data=(game/"resource/gd_PC"/task["layer"]).read_bytes()
            rx=struct.unpack_from(">H",data,32+32*task["index"]+2)[0]
            for n in range(22):
                mode=[0,5,1,0,1,0][n%6];sm=[0,-1,1,1,1,1,1,2,2,2,2,2,2,2,2,2,2,3,0,0,0,0][n]
                flags=int(n!=8);hn=int(n in (9,10,11));owner=n%3;gb=int(n==11)
                answers={0x2097c:int(n==3),0x209e0:[-1,0,1,2,3,4][n%6],0x20a5c:26,0x208cc:8 if n%2 else 0}
                if n>=18:answers[0x209e0]=2
                remaining=[1,0] if n in (12,14) else ([-1] if n in (13,15) else [])
                case=dict(layer=task["layer"],index=task["index"],mode=mode,submode=sm,collision_flags=flags,
                    hit_num=hn,engine_byte_1e=gb,owner_id=owner,queries=[],player_hits=[])
                if n>=18:mode=4 if n!=21 else 5;sm=0 if n==18 else get(work+1,"b")[0];case.update(mode=mode,collision_flags=0);flags=0
                if n<=18:
                    spin=(0xffffff00+n)&0xffffffff;case.update(submode=sm,spin_angle=spin);put(child+0x4c,"I",spin)
                else:del case["submode"]
                actions=[(None,0x42,3)[n%3],(0x3e,-1,5)[n%3]];pos=[13.,float(n),float(rx)];parent=n%2==0
                case.update(player_actions=actions,position=pos,has_set_entry=parent)
                answers.update({0x20b38:n%2,0x20b94:1-n%2,0x1fb28:[8,16,18,41,46,0][n%6]})
                for j,action in enumerate(actions):
                    a=stage+1900+j*32;put(2149479272+4*j,"I",a if action is not None else 0)
                    if action is not None:put(a,"b",action)
                put(child+0x30,"I",stage if parent else 0);put(work+20,"fff",*pos)
                put(child+0x34,"I",work);put(work,"bb",mode,sm);put(work+8,"I",rx);put(work+0x2c,"I",ccl)
                put(ccl+4,"H",flags);put(ccl+0x9c,"I",child);put(ccl+0xa2,"h",hn);put(ccl+0xa4,"I",other)
                put(other,"H",owner);put(2149367176+0x1e,"b",gb)
                row={"calls":[]};u.reg_write(R.UC_PPC_REG_1,stage+4000);u.reg_write(R.UC_PPC_REG_3,child)
                u.reg_write(R.UC_PPC_REG_LR,stop);u.emu_start(base[1]+0x20940,stop,count=400)
                assert u.reg_read(R.UC_PPC_REG_PC)==stop and "outcome" in row
                row.update(submode=get(work+1,"b")[0],spinAngle=get(child+0x4c,"I")[0]);cases.append(case);expected.append(row)
    finally:
        for h in hooks:u.hook_del(h)
    req=Path(tmp)/"contacts.json";out=Path(tmp)/"result.json";req.write_text(json.dumps(cases))
    subprocess.run([sys.argv[1],"--source-root",str(game),"fan-emerald-contact",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","5",str(req)],check=True)
    for a,e in zip(json.loads(out.read_text())["contacts"],expected,strict=True):assert a==e,(a,e)
    picked=next(case for case,result in zip(cases,expected) if result["outcome"]=="pickupRequested")
    resume={k:v for k,v in picked.items() if k not in ("submode","spin_angle")}
    req.write_text(json.dumps([picked,resume]))
    invalid=Path(tmp)/"invalid.json"
    rejected=subprocess.run([sys.argv[1],"--source-root",str(game),"fan-emerald-contact",str(game/"sonic2app.exe"),str(invalid),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","5",str(req)],capture_output=True,text=True)
    assert rejected.returncode!=0 and "no live emerald update" in rejected.stderr and not invalid.exists()
    print(len(cases),"complete common emerald updates matched native callbacks")
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    for allocated in (True,False):
        handle=0x247c0000 if allocated else 0;put(base[6]+20,"I",0xffffffff)
        result={};u.mem_write(stage,bytes(512));put(child+0x34,"I",work)
        invoke(w(52));result["startupResources"]=startup(result["startup"]);invoke(result["entry"])
        result.update(nextMain=get(stage+0x10,"I")[0]-base[1],created=None)
        if allocated:result["created"]=dict(position=list(get(work+20,"fff")),callbackAt20=get(child+0x20,"I")[0]-base[1])
        invoke(result["nextMain"])
        out=Path(tmp)/f"out-{allocated}.json"
        subprocess.run([sys.argv[1],"--source-root",str(game),"fan-wild-canyon",str(game/"sonic2app.exe"),str(out),os.environ["SA2_PC_CONFIG"],"Hero Story Sequence","5",str(allocated).lower()],check=True)
        doc=json.loads(out.read_text())
        state,music,rows=lighting(0);other,alt,rows2=lighting(255)
        assert state==other and rows==rows2==doc["lighting"]
        result.update(state,music=music|alt,emeraldTasks=emerald_tasks(),emeraldPaths=emerald_paths())
        assert doc["stageTask"]==result and doc["dispatch"]["character"]==4 and doc["dispatch"]["level"]==16
    contact_cases(result["emeraldTasks"],tmp)
print("Wild Canyon Hero dispatch, module registration and both stage-task allocation routes matched")
