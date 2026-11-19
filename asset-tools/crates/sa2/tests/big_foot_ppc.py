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
ranges=[(0,0x35c),(0x63a8,0x6434),(0x66bc,0x67f4)]
for lo,hi in ranges:
    for pc in range(lo,hi,4):page(B+pc)
    u.mem_write(B+lo,d[sec(1)+lo:sec(1)+hi])
page(T);page(T+0x2000);page(ret)
player_ptr=0
for module,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;off=0;continue
        off+=inc
        if target!=1 or not any(lo<=off<hi for lo,hi in ranges):continue
        if k==10:continue # observed at the external call site
        val=(B+a if s==1 else D+s*0x400000+a) if module else a;page(val)
        if module and s in (4,5):u.mem_write(val,d[sec(s)+a:sec(s)+a+16])
        if off==0x673a:player_ptr=val
        assert k in (4,6)
        put(B+off,"H",(val if k==4 else (val+0x8000)>>16)&65535)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|8192)
def hook(uc,pc,size,_):
    off=pc-B
    if off==0x10:saved[:]=[reg(i) for i in range(26,32)]
    if off==0x250:
        for i,v in enumerate(saved,26):setreg(i,v)
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
    if off==0x6790:
        assert get(reg(3),"fff")==tuple(position)
        put(reg(3),"fff",*vector);calls.append(off)
    u.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x10,0x34,0x44,0x54,0x68,0x80,0x94,0x134,0x168,0x180,0x1a4,0x1d4,0x1e0,0x208,0x238,0x250,0x338,0x6790):
    u.hook_add(UC_HOOK_CODE,hook,begin=B+off,end=B+off)
def invoke(pc):
    setreg(1,T+0x2ff0);setreg(3,STAGE);u.reg_write(R.UC_PPC_REG_LR,ret)
    u.emu_start(B+pc,ret,count=1200);assert u.reg_read(R.UC_PPC_REG_PC)==ret
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
print("Both Big Foot setup routes and 202 arena callbacks matched installed PPC")
