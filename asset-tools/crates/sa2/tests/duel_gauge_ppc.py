"""Local PowerPC oracle: run stage 42's gauge task update and compare duel-gauge.

Usage: python duel_gauge_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes stg42D.rel +0x2052C under Unicorn: a full rise, drain and removal
sequence plus random single updates. The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP,G=0x81000000,0x82000000,0x90000000,0x80f00000,0x801cc168
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="4403be7d54343b13c2fe4770865d26c2dd755e75d58eff96e66b7fffb2600538"
w=lambda a:struct.unpack_from('>I',d,a)[0]
sec=lambda i:w(w(16)+8*i)&~1
size=lambda i:w(w(16)+8*i+4)
refs={}
for mod,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    off=0
    while True:
        inc,kind,s,v=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=inc;refs[section,off]=(mod,s,v,kind)
place=lambda m,s,a:(B+a if s==1 else D+s*0x400000+a) if m else a
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
code=bytearray(d[sec(1):sec(1)+size(1)]);imports=set()
for (t,off),(m,s,a,k) in refs.items():
    val=place(m,s,a)
    if m==0 and k in (1,4,6):page(val&~0xfff,0x2000)
    if t!=1:continue
    if k in (4,6):struct.pack_into('>H',code,off,(val if k==4 else (val+32768)>>16)&65535)
    elif k==10:
        if m==0:imports.add(val)
        struct.pack_into('>I',code,off,0x48000001|((val-(B+off))&0x3fffffc))
page(B,len(code));u.mem_write(B,bytes(code))
for a in imports|{STOP}:page(a,4);put(a,'I',0x4e800020)
page(T,0x10000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
TASK,WORK,GAUGE=T+0x100,T+0x200,T+0x300
calls=[]
def hook(uc,pc,sz,_):
    assert pc==0x80071ae0 and reg(3)==TASK;calls.append({'engine':'0x80071ae0','args':['task']})
for a in imports:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
f32=lambda x:struct.unpack('>f',struct.pack('>f',x))[0]
def step(wk,gauge,paused,menu):
    calls.clear();put(TASK+0x34,'I',WORK);put(WORK,'b',wk['mode']);put(WORK+6,'H',wk['timer']);put(WORK+8,'I',GAUGE)
    for k,o in (('shown',0x14),('fill',0x1c),('drain',0x24),('start',0x28)):put(WORK+o,'f',wk[k])
    put(GAUGE,'f',gauge);put(G+0x37,'b',int(paused));put(G+0x39,'b',int(menu))
    u.reg_write(R.UC_PPC_REG_3,TASK);u.reg_write(R.UC_PPC_REG_1,T+0xf000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    u.emu_start(B+0x2052c,STOP,count=1<<16);assert reg('PC')==STOP
    return {'mode':get(WORK,'b'),'timer':get(WORK+6,'H'),**{k:get(WORK+o,'f') for k,o in (('shown',0x14),('fill',0x1c),('drain',0x24),('start',0x28))}},list(calls)
rng=random.Random(3)
cases=[]
wk={'mode':0,'timer':0,'shown':0.0,'fill':f32(0.8),'drain':f32(0.4),'start':5.0};g=5.0
for i in range(400):
    if i in (120,160,200):g=f32(g-rng.uniform(0.5,2))
    if i==260:g=0.0
    cases.append((dict(wk),g,i%37==5,i%53==7));wk,_=step(wk,g,i%37==5,i%53==7)
for _ in range(300):
    cases.append(({'mode':rng.choice([-1,0,1,2,3,4]),'timer':rng.choice([0,0x27,0x28,0x29,rng.randrange(65536)]),'shown':f32(rng.uniform(-20,200)),
                   'fill':f32(rng.uniform(0,2)),'drain':f32(rng.uniform(0,2)),'start':f32(rng.uniform(0,6))},f32(rng.uniform(-1,6)),rng.random()<0.1,rng.random()<0.1))
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    frees=0
    for i,(wk,g,p,m) in enumerate(cases):
        nw,nc=step(wk,g,p,m)
        req=Path(tmp)/f'r{i}.json';out=Path(tmp)/f'o{i}.json';req.write_text(json.dumps({'work':wk,'gauge':g,'paused':p,'menu':m}))
        subprocess.run([sys.argv[1],'duel-gauge',str(rel),str(out),str(req)],check=True,capture_output=True)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        mine=doc['work'];conv=lambda x:{k:(fb(v) if isinstance(v,float) else v) for k,v in x.items()}
        assert conv(mine)==conv(nw),(i,wk,mine,nw)
        assert doc['calls']==nc,(i,doc['calls'],nc);frees+=len(nc)
    print(len(cases),'gauge updates matched installed PPC;',frees,'removals')
