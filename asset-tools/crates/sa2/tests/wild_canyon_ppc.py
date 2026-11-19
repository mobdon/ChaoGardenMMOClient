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
        if mod==0 and target==1 and k in (4,6) and (0xac90<=off<=0xaeec or 0x8c68<=off<0x8cb0):v=a
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
for pg in {v&~4095 for v in (2149471344,2149471344+383,2149470768,2149470768+575,2150329376,2149367176,0x80fffe60,0x80fffe60+575)}:u.mem_map(pg,4096)
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
        result.update(state,music=music|alt)
        assert doc["stageTask"]==result and doc["dispatch"]["character"]==4 and doc["dispatch"]["level"]==16
print("Wild Canyon Hero dispatch, module registration and both stage-task allocation routes matched")
