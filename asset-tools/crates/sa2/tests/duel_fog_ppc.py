"""Local PowerPC oracle: run stage 42's fog tasks and compare duel-fog.

Usage: python duel_fog_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes the fog manager (+0xAE04, +0xAE94) and worker (+0xAD48, +0xADE4)
callbacks and their apply/blend helpers under Unicorn against scenario state.
The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP,P=0x81000000,0x82000000,0x90000000,0x80f00000,0x80e00000
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
S6=D+6*0x400000
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
fbits=lambda n:struct.unpack('>I',struct.pack('>f',struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,f'UC_PPC_REG_FPR{n}'))))[0]))[0]
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
page(S6,size(6))
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
for a in imports|{STOP}:page(a,4);u.mem_write(a,struct.pack('>I',0x4e800020))
page(T,0x100000);page(P,0x1000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
calls=[];inp={}
def hook(uc,pc,sz,_):
    a=[reg(i) for i in range(3,8)]
    if pc==0x80071e0c:calls.append(('0x80071e0c',[a[0],a[1],f"s1+0x{a[2]-B:x}",'FogTaskExec',inp['spawned']]));uc.reg_write(R.UC_PPC_REG_3,inp['spawned']);return
    if pc==0x800334b0:calls.append(('0x800334b0',[a[0],a[1],fbits(1),inp['lerpColor']]));uc.reg_write(R.UC_PPC_REG_3,inp['lerpColor']);return
    if pc==0x8010af3c:calls.append(('0x8010af3c',[fbits(1),inp['farInt']]));uc.reg_write(R.UC_PPC_REG_3,inp['farInt']);return
    if pc==0x8011c2fc:calls.append(('0x8011c2fc',[fbits(1),fbits(2)]));return
    if pc==0x8011c2c8:calls.append(('0x8011c2c8',[struct.unpack('>i',struct.pack('>I',a[0]))[0] if a[0]==0xffffffff else a[0]]));return
    if pc in (0x8011c308,0x80071af0):calls.append((hex(pc),[a[0]]));return
    calls.append((hex(pc),[]))
for a in imports:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
ENTRY={'managerUpdate':0xae04,'managerDelete':0xae94,'workerUpdate':0xad48,'workerDelete':0xade4}
LAYOUT=[('worker',0x640,'I'),('current',0x644,'I'),('target',0x648,'I'),('blend',0x64c,'f'),('busy',0x650,'I'),('manager',0x654,'I')]
def run(c):
    calls.clear();inp.clear();inp.update(c['inputs'])
    st=c['state']
    for k,o,f in LAYOUT:put(S6+o,f,st[k])
    put(0x803adbc8,'I',st['engineFog'])
    for addr,p in c['inputs'].get('params',{}).items():
        a=int(addr,16);put(a,'I',p['flags']);put(a+4,'I',p['color']);put(a+8,'f',p['far']);put(a+12,'f',p['near'])
    worker=st['worker'] if c['call']!='managerUpdate' else 0
    if c['inputs'].get('spawned'):page(c['inputs']['spawned'],0x60);u.mem_write(c['inputs']['spawned'],bytes(0x60))
    u.reg_write(R.UC_PPC_REG_3,c['task']);u.reg_write(R.UC_PPC_REG_1,T+0xf0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    u.emu_start(B+ENTRY[c['call']],STOP,count=1<<20);assert reg('PC')==STOP
    out={k:(fb(get(S6+o,'f')) if f=='f' else get(S6+o,'I')) for k,o,f in LAYOUT};out['engineFog']=get(0x803adbc8,'I')
    extra=[]
    if any(n=='0x80071e0c' for n,_ in calls) and inp.get('spawned'):extra=[get(inp['spawned']+0x18,'I')-B]
    return list(calls),out,extra
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
rng=random.Random(9)
def params():return {'flags':rng.choice([0,2,0x10002,0x30002,0x50002,0x0f0002,0x20000]),'color':rng.randrange(1<<32),
                    'far':rng.choice([100000.0,250000.0,rng.uniform(500,99999),99999.99]),'near':rng.uniform(-100,800)}
cases=[]
for k in range(120):
    call=rng.choice(list(ENTRY));task=rng.choice([0x91000040,0x91000080])
    st={'worker':rng.choice([0,0x91000040,0x91000080]),'manager':rng.choice([0,0x91000040,0x91000080]),'current':rng.choice([0,P+0x100]),
        'target':rng.choice([0,P+0x200]),'blend':rng.choice([-0.5,0.0,0.25,0.5,1.0,1.75,rng.random()]),'busy':rng.choice([0,0,1]),'engineFog':rng.choice([0,S6+0x644])}
    inputs={'params':{f'0x{P+0x100:08x}':params(),f'0x{P+0x200:08x}':params()},'spawned':rng.choice([0,0x91000100]),
            'lerpColor':rng.randrange(1<<32),'farInt':rng.randrange(1<<32)}
    st['blend']=struct.unpack('>f',struct.pack('>f',st['blend']))[0]
    for p in inputs['params'].values():p['far']=struct.unpack('>f',struct.pack('>f',p['far']))[0];p['near']=struct.unpack('>f',struct.pack('>f',p['near']))[0]
    cases.append({'call':call,'task':task,'state':st,'inputs':inputs})
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    names=set()
    for i,c in enumerate(cases):
        nc,ns,extra=run(c)
        req=Path(tmp)/f'c{i}.json';out=Path(tmp)/f'o{i}.json';req.write_text(json.dumps(c))
        r=subprocess.run([sys.argv[1],'duel-fog',str(rel),str(out),str(req)],capture_output=True,text=True);assert r.returncode==0,(i,c,r.stderr)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        mine=[(x['engine'],x['args']) for x in doc['calls'] if 'engine' in x]
        writes=[int(x['value'][3:],16) for x in doc['calls'] if 'write' in x]
        assert json.loads(json.dumps(mine))==json.loads(json.dumps([(n,list(a)) for n,a in nc])),(i,c['call'],mine,nc)
        st=doc['state'];st['blend']=fb(st['blend'])
        assert st==ns,(i,c['call'],st,ns)
        assert writes==extra,(i,writes,extra)
        names|={n for n,_ in nc}
    print(len(cases),'fog callbacks matched installed PPC; engine calls',sorted(names))
