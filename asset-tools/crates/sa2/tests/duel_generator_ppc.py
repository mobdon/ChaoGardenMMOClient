"""Local PowerPC oracle: run the installed stage 42 DODAIGEN update and compare duel-generator.

Usage: python -I duel_generator_ppc.py SA2_ASSETS_EXE   (SA2_GAME_PATH = Steam install)
Executes only DODAIGEN's own code (module 41, section 1 +0x2278..+0x24bc) under Unicorn.
The engine allocator and register save helpers are stubs; the game is never started.
"""
import hashlib,json,os,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);rel=game/"resource/gd_PC/stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
G,PLAYERS,SPAWN=0x801cbd88,0x801e7368,0x80071ba4
FIRST,END=0x2278,0x24bc
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d"
w=lambda a:struct.unpack_from('>I',d,a)[0]
sec=lambda i:w(w(16)+8*i)&~1
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
code=bytearray(d[sec(1)+FIRST:sec(1)+END]);imports=set()
for (t,off),(mod,s,a,k) in refs.items():
    if t!=1 or not FIRST<=off<END:continue
    val=D+s*0x400000+a if mod else a
    page(val,4)
    if mod and s!=1:u.mem_write(val,d[sec(s)+a:sec(s)+a+4])
    if k in (4,6):struct.pack_into('>H',code,off-FIRST,(val if k==4 else (val+32768)>>16)&65535)
    else:
        assert (mod,k)==(0,10);imports.add(val)
        struct.pack_into('>I',code,off-FIRST,0x48000001|((val-(B+off))&0x3fffffc))
assert imports=={0x8010ac70,0x8010acbc,SPAWN},imports
page(B+FIRST,END-FIRST);u.mem_write(B+FIRST,bytes(code))
for a in imports:put(a,'I',0x4e800020)
page(STOP);put(STOP,'I',0x4e800020);page(G,0x100);page(PLAYERS,8);page(T,0x200000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000];spawned=[];failing=set()
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
def hook(uc,pc,size,_):
    if pc!=SPAWN:return
    assert (reg(3),reg(4),reg(5),reg(6))==(6,2,D+0x400000+0x1d68,D+5*0x400000+0x1408)
    if len(spawned) in failing:
        spawned.append(None);u.reg_write(R.UC_PPC_REG_3,0);return
    task=alloc(0x60);work=alloc(0x40);put(task+0x34,'I',work);spawned.append(work)
    u.reg_write(R.UC_PPC_REG_3,task)
u.hook_add(UC_HOOK_CODE,hook,begin=SPAWN,end=SPAWN)
bits=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
def run(frames):
    task=alloc(0x60);gen=alloc(0x40);put(task+0x34,'I',gen);slots=[alloc(0x40),alloc(0x40)];out=[]
    for f in frames:
        put(G+0x42,'b',int(f.get('resultMode',False)));put(G+0x37,'b',int(f.get('paused',False)))
        for i,x in enumerate(f['players']):
            put(PLAYERS+4*i,'I',slots[i] if x is not None else 0)
            if x is not None:put(slots[i]+0x14,'f',x)
        spawned.clear();failing.clear();failing.update(f.get('failedAllocations',[]));u.reg_write(R.UC_PPC_REG_1,T+0x1f0000);u.reg_write(R.UC_PPC_REG_3,task)
        u.reg_write(R.UC_PPC_REG_LR,STOP);u.emu_start(B+FIRST,STOP,count=1<<22)
        assert reg('PC')==STOP
        out.append(((get(gen,'b'),get(gen+0x14,'I'),get(gen+4,'H'),get(gen+6,'H')),
                    [None if s is None else (get(s+0x14,'I'),get(s+6,'H'),get(s+3,'B'),get(s+0x18,'I'),get(s+0x1c,'I')) for s in spawned]))
    return out
# Two runners: player 1 accelerates, player 2 lags, leaves and returns; a
# paused stretch; the 36000-frame deadline; and a far jump past the timer clamp.
frames=[];a=b=0.0
for i in range(36800):
    a+=min(i*0.05,22.5);b+=min(i*0.04,19.0)
    f={'players':[a,None if 900<=i<1500 else b]}
    if 2000<=i<2600:f['paused']=True
    if i==20000:f['players']=[a+12000.25,b]
    if i in (3000,20000):f['failedAllocations']=[0,2]
    frames.append(f)
cases={'race':frames,'result':[{'players':[0.0,0.0]},{'players':[0.0,0.0],'resultMode':True},{'players':[5e3,None]}],
       'result-null':[{'players':[0.0,0.0]},{'players':[0.0,0.0],'resultMode':True,'failedAllocations':[0]},{'players':[5e3,None]}],
       'wrap':[{'players':[None,None]},{'players':[-1e5,1.5e5]}]}
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    total=0
    for name,fr in cases.items():
        native=run(fr);req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json'
        req.write_text(json.dumps({'frames':fr}))
        subprocess.run([sys.argv[1],'duel-generator',str(rel),str(out),str(req)],check=True)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        for i,(v,(state,sp)) in enumerate(zip(doc['frames'],native,strict=True)):
            s=v['state'];assert (s['mode'],bits(s['cursorX']),s['flags'],s['frames'])==state,(name,i,s,state)
            assert [None if p is None else (bits(p['x']),p['timer'],int(p['activated']),0,0) for p in v['spawns']]==sp,(name,i,v['spawns'],sp)
            total+=len(sp)
        nulls=sum(p is None for _,sp in native for p in sp);flat=[p for _,sp in native for p in sp if p is not None]
        if name=='race':assert any(p[2] for p in flat) and any(not p[2] and (p[1]^0x8000)-0x8000>900 for p in flat) and native[-1][0][2]==0x1000
        if name=='wrap':assert any(p[1]>0x7fff and p[1]!=0xf1f0 for p in flat)
        if name in ('race','result-null'):assert nulls>0
        print(name,len(fr),'frames,',len(flat),'spawns,',sum(p[2] for p in flat),'activated: matched installed PPC')
    print(total,'native platform spawns matched')
