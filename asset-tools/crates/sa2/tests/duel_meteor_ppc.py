"""Local PowerPC oracle: run the installed stage 42 FIREBALL task and compare duel-meteor.

Usage: python duel_meteor_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes the meteor's constructor, update and destructor (stg42D, section 1
+0x24BC..+0x2B8C) under Unicorn. Every engine import, the shared debris task
(+0x8308) and the Ninja matrix calls are recording stubs whose results come from
the scenario. The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
FIRST,END,INIT,UPDATE,DELETE,DEBRIS=0x24bc,0x2b8c,0x24bc,0x2670,0x2590,0x8308
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
def fpr(n):return struct.unpack('>I',struct.pack('>f',struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,'UC_PPC_REG_FPR'+str(n)))))[0]))[0]
place=lambda m,s,a:(B+a if s==1 else D+s*0x400000+a) if m else a
S5=D+5*0x400000
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
code=bytearray(d[sec(1):sec(1)+size(1)]);imports=set()
for (t,off),(m,s,a,k) in refs.items():
    if t!=1 or not FIRST<=off<END:continue
    val=place(m,s,a)
    if k in (4,6):struct.pack_into('>H',code,off,(val if k==4 else (val+32768)>>16)&65535)
    else:
        assert (m,k)==(0,10);imports.add(val)
        struct.pack_into('>I',code,off,0x48000001|((val-(B+off))&0x3fffffc))
page(B,len(code));u.mem_write(B,bytes(code))
for a in imports|{STOP}:page(a,4);put(a,'I',0x4e800020)
put(B+DEBRIS,'I',0x4e800020);page(T,0x400000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000]
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
vec=lambda p:tuple(get(p+4*i,'I') for i in range(3))
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
calls=[];frame={};rands=[];ctx={}
def hook(uc,pc,size,_):
    f=frame
    if pc==0x800655f4:calls.append(('taskCheck',));uc.reg_write(R.UC_PPC_REG_3,int(f.get('taskCheck',False)))
    elif pc==0x8000f8b8:
        assert reg(3)==ctx['task'];calls.append(('collisionInit',reg(4)-S5,reg(5),reg(6)))
    elif pc==0x800c5ad0:
        p=reg(3);calls.append(('light',get(p,'I'),vec(p+4),vec(p+0x10),get(p+0x1c,'I'),get(p+0x24,'I')))
        uc.reg_write(R.UC_PPC_REG_3,ctx['light'])
    elif pc==0x800c5abc:calls.append(('lightFree',reg(3)))
    elif pc==0x800066f4:calls.append(('hit',));uc.reg_write(R.UC_PPC_REG_3,0x90001234 if f.get('hit') else 0)
    elif pc==0x80116098:calls.append(('push',))
    elif pc==0x801153e4:calls.append(('unit',reg(3)))
    elif pc==0x8011616c:calls.append(('translate',vec(reg(3))))
    elif pc==0x801158a8:calls.append(('rotA',reg(3),struct.unpack('>i',struct.pack('>I',reg(4)))[0]))
    elif pc==0x80115c60:calls.append(('rotB',reg(3),struct.unpack('>i',struct.pack('>I',reg(4)))[0]))
    elif pc==0x80114f0c:
        calls.append(('calcPoint',reg(3),vec(reg(4))));t=f['transforms'][ctx['piece']]
        for i,v in enumerate(t['point']):put(reg(5)+4*i,'f',v)
    elif pc==0x801154ec:
        calls.append(('calcVector',reg(3),vec(reg(4))));t=f['transforms'][ctx['piece']];ctx['piece']+=1
        for i,v in enumerate(t['vector']):put(reg(5)+4*i,'f',v)
    elif pc==B+DEBRIS:
        calls.append(('debris',vec(reg(3)),tuple(get(reg(4)+4*i,'i') for i in range(3)),vec(reg(5)),fpr(1),reg(6)-S5,reg(7),reg(8)-S5,reg(9)))
    elif pc==0x801160e4:calls.append(('pop',))
    elif pc==0x80115dc8:calls.append(('popN',reg(3)))
    elif pc==0x800653b0:assert reg(3)==ctx['task'];calls.append(('destroy',))
    elif pc==0x8000f4ec:assert reg(3)==ctx['task'];calls.append(('collide',))
    elif pc==0x800398d8:calls.append(('visible',vec(reg(3)),fpr(1)));uc.reg_write(R.UC_PPC_REG_3,int(f.get('visible',False)))
    elif pc==0x8010dc88:
        v=rands.pop(0);calls.append(('rand',v));uc.reg_write(R.UC_PPC_REG_3,v)
    elif pc==0x800e2694:
        calls.append(('smoke',vec(reg(3)),vec(reg(4)),fpr(1),fpr(2)))
        if f.get('smokeAllocated'):
            ctx['particle']=alloc(0x40);uc.reg_write(R.UC_PPC_REG_3,ctx['particle'])
        else:uc.reg_write(R.UC_PPC_REG_3,0)
for a in imports|{B+DEBRIS}:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
def run(case):
    task=alloc(0x60);work=alloc(0x40);info=alloc(0x20);ctx.update(task=task,light=case['lightHandle'])
    put(task+0x34,'I',work);put(work+0x2c,'I',info);put(info+0xc,'I',S5+0x1e48)
    wk=case['work'];put(work+0x20,'f',wk['scaleX']);put(work+1,'B',wk['exploded']);put(work+6,'H',wk['timer'])
    put(work+8,'i',wk['angles'][0]);put(work+0xc,'i',wk['angles'][1])
    for i,v in enumerate(wk['position']):put(work+0x14+4*i,'f',v)
    rands[:]=list(case['rand']);out=[]
    for f in case['frames']:
        frame.clear();frame.update(f);calls.clear();ctx['piece']=0;ctx.pop('particle',None)
        entry={'init':INIT,'delete':DELETE}.get(f.get('call'),UPDATE)
        u.reg_write(R.UC_PPC_REG_1,T+0x3f0000);u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_LR,STOP)
        u.emu_start(B+entry,STOP,count=1<<20);assert u.reg_read(R.UC_PPC_REG_PC)==STOP
        if f.get('call')=='init' and not f.get('taskCheck'):
            assert [get(task+o,'I') for o in (0x10,0x14,0x18)]==[B+0x2670,B+0x25bc,B+0x2590]
            assert get(task+0x4c,'I')==ctx['light'];calls.insert(1,('callbacks',))
        if 'particle' in ctx:calls.append(('color',get(ctx['particle']+0x14,'I')))
        out.append(((get(work+1,'B'),get(work+6,'H'),get(work+8,'i'),get(work+0xc,'i'),vec(work+0x14)),list(calls)))
    return out
def rust(cs):
    o=[];F=lambda v:tuple(map(fb,v))
    for c in cs:
        if 'callbacks' in c:o.append(('callbacks',));continue
        if 'local' in c:
            o.append(('debris',F(c['position']),tuple(c['angles']),F(c['velocity']),fb(c['floorY']),int(c['model'][5:],16),c['flags'],int(c['texlist'][5:],16),c['mode']));continue
        if 'particleColor' in c:o.append(('color',int(c['particleColor'],16)));continue
        e=int(c['engine'],16);a=c['args']
        name={0x800655f4:'taskCheck',0x800066f4:'hit',0x80116098:'push',0x801160e4:'pop',0x800653b0:'destroy',0x8000f4ec:'collide'}.get(e)
        if name:o.append((name,))
        elif e==0x8000f8b8:o.append(('collisionInit',int(a['collisionData'][5:],16),a['count'],a['list']))
        elif e==0x800c5ad0:o.append(('light',a['kind'],F(a['position']),F(a['vector']),fb(a['radius']),fb(a['radiusSquared'])))
        elif e==0x800c5abc:o.append(('lightFree',a[0]))
        elif e==0x801153e4:o.append(('unit',a[0]))
        elif e==0x8011616c:o.append(('translate',F(a['translate'])))
        elif e==0x801158a8:o.append(('rotA',*a))
        elif e==0x80115c60:o.append(('rotB',*a))
        elif e==0x80114f0c:o.append(('calcPoint',0,F(a['point'])))
        elif e==0x801154ec:o.append(('calcVector',0,F(a['vector'])))
        elif e==0x80115dc8:o.append(('popN',a[0]))
        elif e==0x800398d8:o.append(('visible',F(a['position']),fb(a['radius'])))
        elif e==0x8010dc88:o.append(('rand',a['returned']))
        elif e==0x800e2694:o.append(('smoke',F(a['position']),F(a['velocity']),fb(a['scale']),fb(a['rotation'])))
        else:raise AssertionError(c)
    return o
rng=random.Random(42)
def pieces():return [{'point':[rng.uniform(-80,80) for _ in range(3)],'vector':[rng.uniform(-3,3) for _ in range(3)]} for _ in range(20)]
timeout=[{'call':'init'}]+[{'visible':i>=5,'smokeAllocated':i%2==0} for i in range(66)]+[{'transforms':pieces()}]
touched=[{'call':'init','taskCheck':True},{'call':'init'},{'visible':True},{'taskCheck':True},{'hit':True,'transforms':pieces()}]
deleted=[{'call':'init'}]+[{'visible':i==2,'smokeAllocated':True} for i in range(8)]+[{'taskCheck':True},{'call':'delete'}]
cases={
 'timeout':{'work':{'exploded':0,'timer':0,'angles':[0,0],'position':[17623.539,-71.0,-153.31],'scaleX':30.0},'collisionRadius':32.0,'lightHandle':0x91000040,'frames':timeout},
 'touched':{'work':{'exploded':0,'timer':0,'angles':[0x4000,-5],'position':[16808.695,40.0,-66.414],'scaleX':7.0},'collisionRadius':32.0,'lightHandle':0x91000080,'frames':touched},
 'deleted':{'work':{'exploded':0,'timer':0,'angles':[0,0],'position':[17484.891,41.0,56.262],'scaleX':0.0},'collisionRadius':32.0,'lightHandle':0x910000c0,'frames':deleted},
 'no-light':{'work':{'exploded':0,'timer':0,'angles':[0,0],'position':[0.0,0.0,0.0],'scaleX':0.0},'collisionRadius':32.0,'lightHandle':0,'frames':[{'call':'init'},{'call':'delete'}]},
}
for c in cases.values():c['rand']=[rng.randrange(32768) for _ in range(800)]
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    total=0
    for name,case in cases.items():
        native=run(case);req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json'
        req.write_text(json.dumps(case))
        subprocess.run([sys.argv[1],'duel-meteor',str(rel),str(out),str(req)],check=True)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        names=set()
        for i,(v,(state,nc)) in enumerate(zip(doc['frames'],native,strict=True)):
            x=v['work'];mine=(x['exploded'],x['timer'],x['angles'][0],x['angles'][1],tuple(map(fb,x['position'])))
            assert mine==state,(name,i,mine,state)
            assert rust(v['calls'])==nc,(name,i,rust(v['calls']),nc)
            names|={c[0] for c in nc}
        print(name,len(native),'callbacks matched installed PPC; calls',sorted(names))
        total+=len(native)
    print(total,'meteor callbacks matched')
