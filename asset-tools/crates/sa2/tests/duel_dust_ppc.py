"""Local PowerPC oracle: run the installed stage 42 dust helper and compare duel-dust.

Usage: python duel_dust_ppc.py SA2_ASSETS_EXE   (SA2_GAME_PATH = Steam install; Unicorn 2.1)
Executes stg42D.rel +0x1E8DC and its particle spawner +0x1DDFC under Unicorn.
Engine imports are recording stubs whose results come from the scenario. The
game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);rel=game/"resource/gd_PC/stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d"
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
f32=lambda v:struct.unpack('>f',struct.pack('>f',v))[0]
def setf(n,v):v=f32(v);u.reg_write(getattr(R,'UC_PPC_REG_FPR'+str(n)),struct.unpack('>Q',struct.pack('>d',v))[0])
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
page(T,0x400000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000]
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
vec=lambda p:tuple(get(p+4*i,'I') for i in range(3))
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
calls=[];case={};st={}
def hook(uc,pc,size,_):
    if pc==0x80115d50:calls.append(('push',))
    elif pc==0x8011509c:calls.append(('unit',reg(3)))
    elif pc==0x80115e24:calls.append(('translate',vec(reg(3))))
    elif pc in (0x801154e0,0x80115560):calls.append(('rot',pc,struct.unpack('>i',struct.pack('>I',reg(4)))[0]))
    elif pc==0x80114bc4:
        calls.append(('calcPoint',vec(reg(4))));t=case['transforms'][st['i']]
        for i,v in enumerate(t['point']):put(reg(5)+4*i,'f',v)
    elif pc==0x801151a4:
        calls.append(('calcVector',vec(reg(4))));t=case['transforms'][st['i']]
        for i,v in enumerate(t['vector']):put(reg(5)+4*i,'f',v)
    elif pc==0x80114eac:
        t=case['transforms'][st['i']];st['i']+=1;calls.append(('scalor',vec(reg(3))));setf(1,t['length'])
    elif pc==0x80032910:
        n=st['a'];st['a']+=1;made=n not in case['failedAllocations'];calls.append(('effect',reg(3)-S5,made))
        if made:e=alloc(0x40);st['e']=e;uc.reg_write(R.UC_PPC_REG_3,e)
        else:uc.reg_write(R.UC_PPC_REG_3,0)
    elif pc==0x8010d940:v=st['r'].pop(0);calls.append(('rand',v));uc.reg_write(R.UC_PPC_REG_3,v)
    elif pc==0x80115d9c:calls.append(('pop',))
for a in imports:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
def spawned(uc,pc,size,_):
    e=st.pop('e',None)
    if e:calls.append(('particle',vec(e+8),vec(e+0x20),get(e+4,'I'),get(e,'i'),get(e+0x2c,'i'),get(e+0x38,'I'),get(e+0x18,'I')))
# Read each particle when its spawner returns (+0x1DFDC is its blr).
u.hook_add(UC_HOOK_CODE,spawned,begin=B+0x1dfdc,end=B+0x1dfdc)
def run(c):
    case.clear();case.update(c);calls.clear();st.update(i=0,a=0,r=list(c['rand']))
    pos,ang=alloc(12),alloc(12)
    for i in range(3):put(pos+4*i,'f',c['position'][i]);put(ang+4*i,'i',c['angles'][i])
    u.reg_write(R.UC_PPC_REG_3,pos);u.reg_write(R.UC_PPC_REG_4,ang)
    for i,v in enumerate(c['args']):setf(1+i,v)
    u.reg_write(R.UC_PPC_REG_1,T+0x3f0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    u.emu_start(B+0x1e8dc,STOP,count=1<<22);assert reg('PC')==STOP
    return list(calls)
def rust(cs):
    o=[];F=lambda v:tuple(map(fb,v))
    for c in cs:
        if 'particle' in c:
            p=c['particle'];o.append(('particle',F(p['position']),F(p['velocity']),fb(p['size']),p['angle0'],p['angle2C'],fb(p['field38']),fb(p['add18'])));continue
        e=int(c['engine'],16);a=c['args']
        if e==0x80115d50:o.append(('push',))
        elif e==0x8011509c:o.append(('unit',a[0]))
        elif e==0x80115e24:o.append(('translate',F(a['translate'])))
        elif e in (0x801154e0,0x80115560):o.append(('rot',e,a[1]))
        elif e==0x80114bc4:o.append(('calcPoint',F(a['point'])))
        elif e==0x801151a4:o.append(('calcVector',F(a['vector'])))
        elif e==0x80114eac:o.append(('scalor',F(a['vector'])))
        elif e==0x80032910:o.append(('effect',int(a['descriptor'][5:],16),a['returnedNonzero']))
        elif e==0x8010d940:o.append(('rand',a['returned']))
        elif e==0x80115d9c:o.append(('pop',))
        else:raise AssertionError(c)
    return o
rng=random.Random(11)
def case_of(args,fails=()):
    n=2*(int(2*args[0]/(10/args[3]))+int(2*args[1]/(10/args[3])))+8
    return {'position':[rng.uniform(-3e4,3e4) for _ in range(3)],'angles':[rng.randrange(-65536,65536) for _ in range(3)],'args':args,
            'transforms':[{'point':[rng.uniform(-500,500) for _ in range(3)],'vector':[rng.uniform(-1,1) for _ in range(3)],'length':rng.uniform(0.2,2)} for _ in range(n)],
            'failedAllocations':list(fails),'rand':[rng.randrange(32768) for _ in range(4*n)]}
cases={'platform-fall':case_of([150.0,50.0,7.0,0.5,1.0]),'odd':case_of([33.3,12.5,3.0,0.37,2.5],fails=(0,3,4))}
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    for name,c in cases.items():
        native=run(c);req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json'
        req.write_text(json.dumps(c))
        subprocess.run([sys.argv[1],'duel-dust',str(rel),str(out),str(req)],check=True)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        mine=rust(doc['calls']);assert mine==native,next(((i,a,b) for i,(a,b) in enumerate(zip(mine,native)) if a!=b),(len(mine),len(native)))
        print(name,len(native),'calls matched installed PPC,',sum(x[0]=='particle' for x in native),'particles')
