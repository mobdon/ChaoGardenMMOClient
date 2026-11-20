"""Local PowerPC oracle: run the installed stage 42 per-frame routine and compare duel-stage-frame.

Usage: python duel_stage_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes stg42D.rel +0x3078 and its SET wrap (+0x2B8C) under Unicorn against
scenario engine state (players, SET status entries). The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
GAME,OBJ1,OBJ2,ENTRIES,COUNT,LIST=0x803ad9d0,0x801e7768,0x801e7728,0x801f5968,0x803ada14,0x803ada2c
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
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
def put(p,fmt,v):u.mem_write(p,struct.pack('>'+fmt,v))
def get(p,fmt):return struct.unpack('>'+fmt,u.mem_read(p,struct.calcsize('>'+fmt)))[0]
def reg(n):return u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
place=lambda m,s,a:(B+a if s==1 else D+s*0x400000+a) if m else a
S5=D+5*0x400000
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
code=bytearray(d[sec(1):sec(1)+size(1)]);imports=set()
for (t,off),(m,s,a,k) in refs.items():
    if t!=1:continue
    val=place(m,s,a)
    if k in (4,6):struct.pack_into('>H',code,off,(val if k==4 else (val+32768)>>16)&65535)
    elif k==10:
        if m==0:imports.add(val)
        struct.pack_into('>I',code,off,0x48000001|((val-(B+off))&0x3fffffc))
page(B,len(code));u.mem_write(B,bytes(code))
for a in imports|{STOP}:page(a,4);put(a,'I',0x4e800020)
for a in (GAME,OBJ1,ENTRIES,COUNT,LIST):page(a,0x2000)
page(T,0x400000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
calls=[]
def hook(uc,pc,size,_):
    if pc==0x8003b8fc:calls.append(('kill',reg(3)))
for a in imports:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
put(LIST,'I',S5+0x36cc)
game_obj,objs1,objs2=T+0x100,[T+0x200,T+0x300],[T+0x400,T+0x500]
records=T+0x10000
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
def run(frames):
    out=[]
    for f in frames:
        calls.clear()
        put(GAME,'I',game_obj if f['gameReady'] is not None else 0);put(game_obj+0xc,'I',0 if f['gameReady'] else 1)
        for i,p in enumerate(f['players']):
            put(OBJ1+4*i,'I',objs1[i] if p else 0)
            if p:put(objs1[i]+0x14,'f',p[0]);put(objs1[i]+0x18,'f',p[1])
            put(OBJ2+4*i,'I',objs2[i]);put(objs2[i]+0x10,'H',0x4000 if f['playerFlags'][i] else 0x0101)
        put(COUNT,'h',len(f['entries']))
        for i,e in enumerate(f['entries']):
            a=ENTRIES+16*i;r=records+0x20*i
            put(a,'B',e['bytes'][0]);put(a+1,'B',e['bytes'][1]);put(a+2,'h',e['status']);put(a+8,'I',r)
            put(r,'H',e['objectId']);put(r+8,'f',e['x'])
        u.reg_write(R.UC_PPC_REG_1,T+0x3f0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
        u.emu_start(B+0x3078,STOP,count=1<<22);assert reg('PC')==STOP
        ents=[((get(ENTRIES+16*i,'B'),get(ENTRIES+16*i+1,'B')),get(ENTRIES+16*i+2,'h'),get(records+0x20*i+8,'I')) for i in range(len(f['entries']))]
        out.append((list(calls),ents))
    return out
rng=random.Random(5)
ids=[0x19,0x3,0x1a,0x1a,0x3,0x1a,0x2,0x18,0x8003,0x801a]
frames=[]
for k in range(400):
    px=rng.choice([0.0,150.0,299.9,300.0,rng.uniform(300,60000)])
    py=rng.choice([0.0,-99.9,-100.0,-100.5,rng.uniform(-5000,50)])
    f={'gameReady':rng.choice([True,True,False,None]),'players':[[px,py] if rng.random()>0.05 else [px,py],[rng.uniform(0,6e4),0.0] if rng.random()>0.2 else None],
       'playerFlags':[rng.random()<0.15,rng.random()<0.15],
       'entries':[{'bytes':[rng.randrange(256),rng.randrange(256)],'status':rng.choice([0,0,1,8,9,2,-0x8000,0x100]),'objectId':rng.choice(ids),
                   'x':rng.choice([px+rng.uniform(-1200,1200),px+1200.0,px-1200.0,px+rng.uniform(-20000,20000),rng.uniform(-1e5,1e5)])} for _ in range(rng.randrange(1,30))]}
    frames.append(f)
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    native=run(frames)
    rust_frames=[dict(f,gameReady=bool(f['gameReady'])) for f in frames]
    req=Path(tmp)/'frames.json';out=Path(tmp)/'out.json';req.write_text(json.dumps({'frames':rust_frames}))
    subprocess.run([sys.argv[1],'duel-stage-frame',str(rel),str(out),str(req)],check=True)
    doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
    moved=kills=0
    for i,(v,(nc,ne)) in enumerate(zip(doc['frames'],native,strict=True)):
        assert [('kill',c['args'][0]) for c in v['calls']]==nc,(i,v['calls'],nc)
        mine=[(tuple(e['bytes']),e['status'],fb(e['x'])) for e in v['entries']]
        assert mine==ne,(i,[(a,b) for a,b in zip(mine,ne) if a!=b][:3])
        moved+=sum(a[2]!=fb(e['x']) for a,e in zip(ne,frames[i]['entries']));kills+=len(nc)
    assert moved and kills
    print(len(frames),'frames matched installed PPC;',moved,'SET objects wrapped,',kills,'fall-outs')
