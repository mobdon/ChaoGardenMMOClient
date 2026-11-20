"""Local PowerPC oracle: run the installed stage 42 debris manager and compare duel-debris.

Usage: python duel_debris_ppc.py SA2_ASSETS_EXE   (SA2_GAME_PATH = Steam install; Unicorn 2.1)
Executes the module's spawn (+0x8308 and +0x8348), update (+0x888C), delete
(+0x8820), wind query (+0x7558) and trail effect helpers (+0x1DFE0, +0x1E194)
under Unicorn. Engine imports and the wind force callback are recording stubs
whose results come from the scenario. The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);rel=game/"resource/gd_PC/stg42D.rel"
B,D,T,STOP,WIND=0x81000000,0x82000000,0x90000000,0x80f00000,0x80f00100
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
def freg(n):return struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,'UC_PPC_REG_FPR'+str(n)))))[0]
def fbits(n):return struct.unpack('>I',struct.pack('>f',freg(n)))[0]
f32=lambda v:struct.unpack('>f',struct.pack('>f',v))[0]
def setf(n,v):v=f32(v);u.reg_write(getattr(R,'UC_PPC_REG_FPR'+str(n)),struct.unpack('>Q',struct.pack('>d',v))[0])
place=lambda m,s,a:(B+a if s==1 else D+s*0x400000+a) if m else a
S5,S6=D+5*0x400000,D+6*0x400000
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
page(S6,size(6))
code=bytearray(d[sec(1):sec(1)+size(1)]);imports=set()
for (t,off),(m,s,a,k) in refs.items():
    if t!=1:continue
    val=place(m,s,a)
    if k in (4,6):struct.pack_into('>H',code,off,(val if k==4 else (val+32768)>>16)&65535)
    elif k==10:
        if m==0:imports.add(val)
        struct.pack_into('>I',code,off,0x48000001|((val-(B+off))&0x3fffffc))
page(B,len(code));u.mem_write(B,bytes(code))
for a in imports|{STOP,WIND}:page(a,4);put(a,'I',0x4e800020)
page(T,0x800000);page(0x801cbd88,0x100)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000]
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
vec=lambda p:tuple(get(p+4*i,'I') for i in range(3))
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
calls=[];op={};rands=[];ctx={'allocs':0,'effects':[],'winds':[]}
def ok():
    n=ctx['allocs'];ctx['allocs']+=1;return n not in op.get('failedAllocations',[])
def hook(uc,pc,size,_):
    if pc==0x80071ba4:
        made=ok();assert (reg(3),reg(4),reg(5),reg(6))==(2,3,B+0x888c,S5+0xd590)
        calls.append(('task',made))
        if made:t=alloc(0x60);put(t+0x34,'I',alloc(0x40));uc.reg_write(R.UC_PPC_REG_3,t)
        else:uc.reg_write(R.UC_PPC_REG_3,0)
    elif pc==0x801194b4:
        assert (reg(3),reg(4))==(1,0x50);got=ok();calls.append(('alloc',got))
        uc.reg_write(R.UC_PPC_REG_3,alloc(0x50) if got else 0)
    elif pc==0x80115d50:calls.append(('push',))
    elif pc==0x8011509c:calls.append(('unit',reg(3)))
    elif pc==0x80115044:calls.append(('translate',vec(reg(4))))
    elif pc in (0x80115560,0x801154e0,0x80115918):calls.append(('rot',pc,struct.unpack('>i',struct.pack('>I',reg(4)))[0]))
    elif pc==0x80114bc4:
        calls.append(('calcPoint',vec(reg(4))))
        for i,v in enumerate(op['point']):put(reg(5)+4*i,'f',v)
    elif pc==0x80115a80:calls.append(('popN',reg(3)))
    elif pc==0x8010d940:v=rands.pop(0);calls.append(('rand',v));uc.reg_write(R.UC_PPC_REG_3,v)
    elif pc==0x801194e0:calls.append(('free',))
    elif pc==0x80071888:calls.append(('freeTask',))
    elif pc==0x800e254c:calls.append(('splash',vec(reg(3)),vec(reg(4)),fbits(1),fbits(2)))
    elif pc==0x80032910:
        made=ok();calls.append(('effect',reg(3)-S5,made))
        if made:e=alloc(0x40);ctx['effects'].append(e);uc.reg_write(R.UC_PPC_REG_3,e)
        else:uc.reg_write(R.UC_PPC_REG_3,0)
    elif pc==WIND:
        wr=ctx['winds'].pop(0);buf=reg(5);put(buf,'I',wr['flags'])
        for i in range(3):put(buf+4+4*i,'f',wr['vector'][i]);put(buf+0x10+4*i,'f',wr['direction'][i])
        calls.append(('wind',wr['flags'],tuple(map(fb,wr['vector'])),tuple(map(fb,wr['direction']))))
    elif pc==0x80114eac:
        calls.append(('scalor',vec(reg(3))));setf(1,ctx['length'])
for a in imports|{WIND}:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
force=alloc(0x50);put(force+0x38,'I',WIND);put(S6+0x628,'I',force)
def pieces():
    out=[];p=get(S6+0x63c,'I')
    while p:
        def opt(o):
            v=get(p+o,'I');return None if v==0 else v-S5
        out.append((vec(p),vec(p+0xc),tuple(get(p+0x18+4*i,'i') for i in range(3)),vec(p+0x24),get(p+0x30,'I')-S5,opt(0x34),opt(0x38),
                    get(p+0x3c,'I'),get(p+0x40,'i'),get(p+0x44,'I'),get(p+0x48,'I')))
        p=get(p+0x4c,'I')
    return out
def run(ops):
    out=[]
    for o in ops:
        op.clear();op.update(o);calls.clear();ctx['allocs']=0;ctx['effects']=[]
        u.reg_write(R.UC_PPC_REG_1,T+0x7f0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
        if o['call']=='spawn':
            put(0x801cbd88+0x37,'b',int(o['paused']));pos,ang,vel=alloc(12),alloc(12),alloc(12)
            for i in range(3):put(pos+4*i,'f',o['position'][i]);put(ang+4*i,'i',o['angles'][i]);put(vel+4*i,'f',o['velocity'][i])
            u.reg_write(R.UC_PPC_REG_3,pos);u.reg_write(R.UC_PPC_REG_4,ang);u.reg_write(R.UC_PPC_REG_5,vel);setf(1,o['floorY'])
            if o['spin'] is None:
                for r,v in zip((6,7,8,9),(S5+o['model'],0,S5+o['texlist'],o['flags'])):u.reg_write(getattr(R,f'UC_PPC_REG_{r}'),v)
                entry=0x8308
            else:
                sp=alloc(12)
                for i in range(3):put(sp+4*i,'i',o['spin'][i])
                for r,v in zip((6,7,8,9,10),(sp,S5+o['model'],0,S5+o['texlist'],o['flags'])):u.reg_write(getattr(R,f'UC_PPC_REG_{r}'),v)
                entry=0x8348
        else:
            t=get(S6+0x638,'I');assert t;u.reg_write(R.UC_PPC_REG_3,t)
            if o['call']=='update':
                put(0x801cbd88+0x37,'b',int(o['paused']));ctx['winds']=list(o['wind'])
                ctx['length']=o['wind'][0]['length'] if o['wind'] else 0.0
                entry=0x888c
            else:entry=0x8820
        u.emu_start(B+entry,STOP,count=1<<22);assert reg('PC')==STOP,(o['call'],hex(reg('PC')))
        for e in ctx['effects']:
            calls.append(('trail',vec(e+8),vec(e+0x20),get(e+4,'I'),get(e,'i'),get(e+0x2c,'i'),get(e+0x38,'I'),get(e+0x18,'I')))
        t=get(S6+0x638,'I')
        out.append((bool(t),get(get(t+0x34,'I')+6,'H') if t else None,pieces(),list(calls)))
    return out
def rust(cs):
    o=[];F=lambda v:tuple(map(fb,v))
    for c in cs:
        if 'callbacks' in c:continue
        if 'windQuery' in c:q=c['windQuery'];o.append(('wind',q['flags'],F(q['vector']),F(q['direction'])));continue
        if 'trail' in c:continue
        e=int(c['engine'],16);a=c['args']
        if e==0x80071ba4:o.append(('task',a['returnedNonzero']))
        elif e==0x801194b4:o.append(('alloc',a['returnedNonzero']))
        elif e==0x80115d50:o.append(('push',))
        elif e==0x8011509c:o.append(('unit',a[0]))
        elif e==0x80115044:o.append(('translate',F(a['translate'])))
        elif e in (0x80115560,0x801154e0,0x80115918):o.append(('rot',e,a[1]))
        elif e==0x80114bc4:o.append(('calcPoint',F(a['point'])))
        elif e==0x80115a80:o.append(('popN',a[0]))
        elif e==0x8010d940:o.append(('rand',a['returned']))
        elif e==0x801194e0:o.append(('free',))
        elif e==0x80071888:o.append(('freeTask',))
        elif e==0x800e254c:o.append(('splash',F(a['position']),F(a['vector']),fb(a['scale']),fb(a['size'])))
        elif e==0x80032910:o.append(('effect',int(a['descriptor'][5:],16),a['returnedNonzero']))
        elif e==0x80114eac:o.append(('scalor',F(a['vector'])))
        else:raise AssertionError(c)
    o+=[('trail',F(t['position']),F(t['velocity']),fb(t['scale']),t['angle0'],t['angle2C'],fb(t['size']),fb(t['add18'])) for t in (x['trail'] for x in cs if 'trail' in x)]
    return o
rng=random.Random(7)
def spawn(pos,vel,flags,model=0x1dd8,tex=0x1c7c,floor=-100000.0,spin=None,paused=False,fail=()):
    return {'call':'spawn','position':pos,'angles':[rng.randrange(-65536,65536) for _ in range(3)],'velocity':vel,'floorY':floor,
            'spin':spin,'model':model,'texlist':tex,'flags':flags,'paused':paused,'failedAllocations':list(fail),
            'point':[pos[0]+rng.uniform(-20,20),pos[1]+rng.uniform(-5,40),pos[2]+rng.uniform(-20,20)]}
def update(paused=False,wind=(),fail=()):return {'call':'update','paused':paused,'wind':list(wind),'failedAllocations':list(fail)}
W=lambda f,dx:{'flags':f,'vector':[3.0,4.0,0.0],'direction':[dx,1.0,-0.5],'length':5.0}
meteor=[spawn([100.0,40.0,0.0],[rng.uniform(-3,3),rng.uniform(1,3),rng.uniform(-3,3)],0x2000,model=0x1dd8 if i<5 else 0x1c14,tex=0x1c7c if i<5 else 0x1a28) for i in range(20)]
meteor+= [update() for _ in range(240)]+[{'call':'delete'}]
mixed=[spawn([0.0,50.0,0.0],[1.0,-2.0,0.5],0x2000|0x20|0x10,floor=0.0,fail=(0,)),
       spawn([0.0,50.0,0.0],[1.0,-2.0,0.5],0x2000|0x20|0x10,floor=0.0),
       spawn([5.0,30.0,1.0],[0.0,-1.0,0.0],0x8000|0x400|0x20,model=0x13a0,floor=0.0),
       spawn([5.0,30.0,1.0],[0.5,-3.0,0.2],0x4000|3|0x10,floor=10.0),
       spawn([5.0,30.0,1.0],[0.5,-3.0,0.2],0x4000|1,floor=10.0,spin=[300,-200,90]),
       spawn([-5.0,20.0,1.0],[0.0,0.0,0.0],0x20000|0x80|0x100,floor=0.0),
       spawn([-5.0,20.0,1.0],[0.0,-0.5,0.0],0x40000|0x200,floor=0.0),
       spawn([-5.0,20.0,1.0],[0.0,-0.5,0.0],0x40800,floor=0.0),
       spawn([-5.0,20.0,1.0],[0.0,-4.0,0.0],0x40|0x20,floor=0.0),
       spawn([9.0,2.0,9.0],[0.0,-0.1,0.0],0x2000,floor=0.0,paused=True),
       spawn([9.0,2.0,9.0],[0.0,-0.1,0.0],0x2000,floor=0.0,fail=(0,))]
for i in range(260):
    mixed.append(update(paused=60<=i<70,wind=[W(3 if i%2 else 2,2.0)],fail=(0,) if i%7==0 else ()))
mixed.append({'call':'delete'})
cases={'meteor':meteor,'mixed':mixed}
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    total=0
    for name,ops in cases.items():
        rands[:]=[rng.randrange(32768) for _ in range(20000)];pool=list(rands)
        native=run(ops)
        req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json'
        req.write_text(json.dumps({'rand':pool,'ops':ops}))
        r=subprocess.run([sys.argv[1],'duel-debris',str(rel),str(out),str(req)],capture_output=True,text=True)
        assert r.returncode==0,r.stderr
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        names=set()
        for i,(v,(mgr,visited,ps,nc)) in enumerate(zip(doc['ops'],native,strict=True)):
            mine=[(tuple(map(fb,p['position'])),tuple(map(fb,p['velocity'])),tuple(p['angles']),tuple(map(fb,p['spin'])),p['texlist'],p['object'],p['model'],
                   fb(p['floorY']),p['timer'],p['flags'],fb(p['life'])) for p in v['pieces']]
            assert v['manager']==mgr,(name,i)
            if visited is not None and ops[i]['call']=='update':assert v['visited']==visited,(name,i,v['visited'],visited)
            assert mine==ps,(name,i,mine[:2],ps[:2])
            assert rust(v['calls'])==nc,(name,i,rust(v['calls'])[:12],nc[:12])
            names|={c[0] for c in nc}
        if name=='mixed':
            states=[p for _,_,ps,_ in native for p in ps]
            assert any(p[1][1]==0 and p[3]==(0,0,0) and p[10]!=fb(1.0) and p[8]>=10 for p in states),'rest path'
            assert any(p[9]&0x4000 for p in states) and any(p[9]>>31 for p in states),'permuted and paused pieces'
        print(name,len(ops),'callbacks matched installed PPC; calls',sorted(names))
        total+=len(ops)
    print(total,'debris callbacks matched')
