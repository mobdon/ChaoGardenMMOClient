"""Local PowerPC oracle: run stage 42's duel director (ControlShadow) and compare duel-director.

Usage: python duel_director_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes stg42D.rel +0x3CC with its local helpers (+0x0, +0x8C, +0x168, +0x280,
+0x334) under Unicorn on random worlds. Engine imports return scenario values;
the task-spawning helpers +0x22070, +0x18620 and +0x20470 are recording stubs.
The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
G,CAM,BODIES,DATA,SLOTS,READY=0x801cc168,0x801e53f8,0x801e7768,0x801e7728,0x801e7788,0x803ad9d0
FLAGS=[0x803ad884,0x803ad888,0x803ad894,0x803ad898]
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
S5=D+5*0x400000
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
sreg=lambda n:struct.unpack('>i',struct.pack('>I',reg(n)))[0]
def fbits(n):return struct.unpack('>I',struct.pack('>f',struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,f'UC_PPC_REG_FPR{n}'))))[0]))[0]
def setf(n,v):u.reg_write(getattr(R,f'UC_PPC_REG_FPR{n}'),struct.unpack('>Q',struct.pack('>d',v))[0])
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
page(D+6*0x400000,size(6))
code=bytearray(d[sec(1):sec(1)+size(1)]);imports=set()
for (t,off),(m,s,a,k) in refs.items():
    val=place(m,s,a)
    if m==0 and k in (1,4,6):page(val&~0xfff,0x2000)
    if t!=1:continue
    if k in (4,6):struct.pack_into('>H',code,off,(val if k==4 else (val+32768)>>16)&65535)
    elif k==10:
        if m==0:imports.add(val)
        struct.pack_into('>I',code,off,0x48000001|((val-(B+off))&0x3fffffc))
LOCALS=(0x22070,0x18620,0x20470)
for a in LOCALS:struct.pack_into('>I',code,a,0x4e800020)
page(B,len(code));u.mem_write(B,bytes(code))
SAVE=[0x8010af98+4*k for k in range(19)];REST=[0x8010afe4+4*k for k in range(19)]
for a in imports|{STOP}|set(SAVE)|set(REST):page(a,4);put(a,'I',0x4e800020)
page(T,0x100000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
TASK,WORK=T+0x100,T+0x200
BOD=[T+0x1000,T+0x2000];DAT=[T+0x3000,T+0x4000];RDY=T+0x5000
calls=[];queues={}
def name(p):
    for i in range(2):
        if p==BOD[i]+0x14:return f'bodies[{i}]+0x14'
        if p==DAT[i]:return f'data[{i}]'
        if p==DAT[i]+0x230:return f'data[{i}]+0x230'
        if p==DAT[i]+0x48:return f'data[{i}]+0x48'
    if p==WORK+0x14:return 'work+0x14'
    if p==TASK:return 'task'
    return None
vec=lambda p:[get(p+4*i,'I') for i in range(3)]
def ret(a):return queues[f'0x{a:08x}'].pop(0)
def hook(uc,pc,sz,_):
    if pc in SAVE or pc in REST:
        save=pc in SAVE;first=14+(pc-(SAVE[0] if save else REST[0]))//4;r11=reg(11)
        for r in range(first,32):
            at=r11-4*(32-r)
            if save:put(at,'I',reg(r))
            else:uc.reg_write(getattr(R,f'UC_PPC_REG_{r}'),get(at,'I'))
        return
    E=lambda args,v=None:calls.append({'engine':f'0x{pc:08x}','args':args,**({'returned':v} if v is not None else {})})
    if pc==B+0x22070:calls.append({'local':'0x22070','args':[reg(3),f's5+0x{reg(4)-S5:x}']});return
    if pc==B+0x18620:calls.append({'local':'0x18620','args':[vec(reg(3)),reg(4),reg(5),sreg(6),reg(7),reg(8)]});return
    if pc==B+0x20470:calls.append({'local':'0x20470','args':[name(reg(3)),name(reg(4))]});return
    if pc in (0x8001cd40,0x8001dda0,0x8010dc88):v=ret(pc);E([],v);uc.reg_write(R.UC_PPC_REG_3,int(v)&0xffffffff);return
    if pc==0x800118e0:v=ret(pc);E([reg(3),reg(4)],v);uc.reg_write(R.UC_PPC_REG_3,int(v)&0xffffffff);return
    if pc==0x800c3bdc:v=ret(pc);E([reg(3)],v);uc.reg_write(R.UC_PPC_REG_3,int(v)&0xffffffff);return
    if pc==0x80111168:v=ret(pc);E([fbits(1),fbits(2)],v);setf(1,v);return
    if pc==0x8002b8e0:v=ret(pc);E([sreg(3),sreg(4)],v);uc.reg_write(R.UC_PPC_REG_3,int(v)&0xffffffff);return
    if pc==0x8011b65c:v=ret(pc);E([vec(reg(3)),vec(reg(4))],v);setf(1,v);return
    if pc==0x8011217c:v=ret(pc);E([fbits(1)],v);setf(1,v);return
    if pc==0x8002ae5c:v=ret(pc);E([fbits(1),fbits(2),fbits(3)],v);setf(1,v);return
    if pc in (0x8001162c,0x80022e24,0x80022e64,0x800c77a8,0x80071af0):E([reg(3)]);return
    if pc in (0x8001dd50,0x8001dd64):E([]);return
    if pc==0x800117ec:E([reg(3),reg(4)]);return
    if pc==0x80088124:E([reg(3),name(reg(4)) or vec(reg(4))]);return
    if pc==0x8004691c:E([reg(3),name(reg(4))]);return
    if pc in (0x8003a318,0x800399bc):E([reg(3),fbits(1),fbits(2),fbits(3)]);return
    if pc==0x800c7aac:
        p=reg(4);E([reg(3),{'f0':get(p,'I'),'w1':get(p+4,'I'),'point':vec(get(p+8,'I')),'f3':get(p+0xc,'I'),'f4':get(p+0x10,'I'),'w5':get(p+0x14,'I')}]);return
    if pc==0x8003a54c:E([reg(3),reg(4),vec(reg(5)) if reg(5) else 0,reg(6)]);return
    if pc==0x800cd69c:p=reg(4);E([reg(3),{'w0':get(p,'I'),'from':name(get(p+4,'I')),'to':name(get(p+8,'I')),'f3':get(p+0xc,'I')}]);return
    if pc==0x8006b978:E([reg(3),reg(4),reg(5),reg(6)]);return
    raise AssertionError(hex(pc))
for a in imports|set(SAVE)|set(REST)|{B+x for x in LOCALS}:u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
bf=lambda b:struct.unpack('>f',struct.pack('>I',b))[0]
def write_world(wd,wk):
    for k,o in (('g30',0x30),('g31',0x31),('g32',0x32),('g34',0x34),('g37',0x37),('g39',0x39),('g42',0x42)):put(G+o,'b',wd[k])
    put(G+0x7c,'I',wd['g7c'])
    for i,v in enumerate(wd['camera']):put(CAM+4*i,'I',v)
    for a,v in zip(FLAGS,wd['flags']):put(a+1,'B',v)
    put(READY,'I',RDY if wd['ready'] else 0);put(RDY+0xc,'I',0)
    for i in range(2):
        b=wd['bodies'][i];p=BOD[i];put(BODIES+4*i,'I',p)
        put(p,'b',b['action']);put(p+4,'h',b['flags4']);put(p+6,'H',b['field6']);put(p+0xc,'i',b['yaw'])
        for j in range(3):put(p+0x14+4*j,'f',b['pos'][j])
        q=wd['data'][i];p=DAT[i];put(DATA+4*i,'I',p)
        put(p+1,'b',q['byte1']);put(p+3,'b',q['character']);put(p+0x10,'h',q['flags10']);put(p+0x1c,'h',q['field1c'])
        for k,o in (('gauge',0x48),('speed',0x64),('field68',0x68),('field6c',0x6c),('fieldec',0xec),('field1b4',0x1b4)):put(p+o,'f',q[k])
        put(SLOTS+4*i,'I',wd['slots'][i])
    put(TASK+0xc,'I',wd['child']);put(TASK+0x40,'I',WORK)
    put(WORK,'b',wk['mode']);put(WORK+1,'b',wk['flag']);put(WORK+2,'h',wk['timer']);put(WORK+4,'h',wk['limit'])
    for j in range(3):put(WORK+8+4*j,'f',wk['point'][j]);put(WORK+0x14+4*j,'f',wk['target'][j])
def read_back(wd):
    out=json.loads(json.dumps(wd))
    for k,o in (('g30',0x30),('g31',0x31),('g32',0x32),('g34',0x34)):out[k]=get(G+o,'b')
    out['camera']=[get(CAM+4*i,'I') for i in range(4)];out['flags']=[get(a+1,'B') for a in FLAGS]
    out['data'][1]['gauge']=fb(get(DAT[1]+0x48,'f'))
    wk={'mode':get(WORK,'b'),'flag':get(WORK+1,'b'),'timer':get(WORK+2,'h'),'limit':get(WORK+4,'h'),
        'point':[get(WORK+8+4*j,'I') for j in range(3)],'target':[get(WORK+0x14+4*j,'I') for j in range(3)]}
    return out,wk
rng=random.Random(42)
f32=lambda v:struct.unpack('>f',struct.pack('>f',v))[0]
def world():
    p1x=rng.choice([0.0,300.0,rng.uniform(0,20000)])
    def body(x):
        return {'action':rng.choice([0,1,2,3,4,5,0xd,0xd,0x3d,0x40,0x42,0x43,0x44,0x45,0x10,-3]),'flags4':rng.choice([0,1,2,3,4,0x100]),
                'field6':rng.choice([0,0,1]),'yaw':rng.randrange(-65536,65536),'pos':[f32(x),f32(rng.choice([0.0,-49.0,-51.0,-4.5,26.0,40.0,rng.uniform(-100,100)])),f32(rng.choice([0.0,49.0,-50.0,50.5,rng.uniform(-80,80)]))]}
    def data():
        return {'byte1':rng.choice([0,1]),'character':rng.choice([0,1,1,0,2]),'flags10':rng.choice([0,0,0,0x4000,0x10]),'field1c':rng.choice([0,1,4]),
                'gauge':f32(rng.choice([0.0,1.5,2.0,2.5,3.0,3.5,4.0,5.0,-1.0])),'speed':f32(rng.uniform(0,10)),'field68':f32(rng.choice([0.0,0.0,0.3,0.2,-1.0,1.0])),
                'field6c':f32(rng.uniform(-5,5)),'fieldec':f32(rng.uniform(0,10)),'field1b4':f32(rng.uniform(-50,40))}
    p2x=p1x+rng.choice([-200.0,-151.0,-149.0,-11.0,-9.0,0.0,24.0,26.0,44.0,46.0,59.0,61.0,300.0,rng.uniform(-300,400)])
    wd={'g30':rng.choice([0,3]),'g31':rng.choice([0,2]),'g32':rng.choice([0,5]),'g34':rng.choice([0,0,5,-1]),'g37':rng.choice([0,0,0,2,1]),
        'g39':rng.choice([0,0,0,0,1]),'g42':rng.choice([0,0,0,0,1,2]),'g7c':rng.choice([1,1,1,0]),
        'camera':[rng.randrange(1<<32) for _ in range(4)],'flags':[rng.randrange(3) for _ in range(4)],'ready':rng.random()<0.7,
        'bodies':[body(p1x),body(p2x)],'data':[data(),data()],'child':rng.choice([0,0x91000000]),'slots':[0x91000100,0x91000200]}
    wk={'mode':rng.choice(list(range(14))+[-1]),'flag':rng.choice([0,1]),'timer':rng.choice([0,1,2,0x3c,0x3d,0x77,0x78,0x12b,0x12c,0x14a,0x150,rng.randrange(-5,400)]),
        'limit':rng.choice([0,0x14,0x3c,0x78,0x168,rng.randrange(0,400)]),'point':[f32(rng.uniform(-10,10)) for _ in range(3)],'target':[f32(rng.uniform(-10,10)) for _ in range(3)]}
    rets={'0x8001cd40':[rng.choice([7,0x10,0x10,3,0]) for _ in range(4)],'0x800118e0':[rng.choice([0,1]) for _ in range(8)],'0x8001dda0':[rng.choice([0,1,2,3,-1])],
          '0x800c3bdc':[rng.choice([0xd,0])],'0x80111168':[rng.uniform(-3.2,3.2) for _ in range(8)],'0x8002b8e0':[rng.choice([0,0x4fff,0x5000,0x7000]) for _ in range(8)],
          '0x8011b65c':[rng.choice([10.0,18.9,19.0,79.0,81.0,rng.uniform(0,200)]) for _ in range(8)],'0x8011217c':[rng.choice([0.6,0.65,0.7,rng.random()])],
          '0x8002ae5c':[rng.uniform(-100,100) for _ in range(3)],'0x8010dc88':[rng.randrange(32768)]}
    return wd,wk,rets
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    seen_modes=set();n=0;nc_total=0
    for i in range(600):
        wd,wk,rets=world()
        calls.clear();queues.clear();queues.update({k:list(v) for k,v in rets.items()})
        write_world(wd,wk)
        u.reg_write(R.UC_PPC_REG_3,TASK);u.reg_write(R.UC_PPC_REG_1,T+0xf0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
        u.emu_start(B+0x3cc,STOP,count=1<<20);assert reg('PC')==STOP
        nw,nk=read_back(wd)
        req=Path(tmp)/f'r{i}.json';out=Path(tmp)/f'o{i}.json'
        hexf=lambda v:'0x%016x'%struct.unpack('>Q',struct.pack('>d',v))[0] if isinstance(v,float) else v
        req.write_text(json.dumps({'world':wd,'work':wk,'returns':{k:[hexf(x) for x in vs] for k,vs in rets.items()}}))
        r=subprocess.run([sys.argv[1],'duel-director',str(rel),str(out),str(req)],capture_output=True,text=True);assert r.returncode==0,(i,r.stderr)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        norm=lambda v:json.loads(json.dumps(v))
        mc=norm(doc['calls']);ncs=norm(calls)
        assert mc==ncs,(i,wk['mode'],next(((j,a,b) for j,(a,b) in enumerate(zip(mc,ncs)) if a!=b),(len(mc),len(ncs))))
        dk=doc['work'];dk['point']=[fb(x) for x in dk['point']];dk['target']=[fb(x) for x in dk['target']]
        assert norm(dk)==norm(nk),(i,dk,nk,[(v.hex(),struct.unpack('>I',struct.pack('>f',v))[0]) for v in rets['0x8002ae5c']])
        dw=doc['world'];dw['data'][1]['gauge']=fb(dw['data'][1]['gauge'])
        assert norm({k:dw[k] for k in ('g30','g31','g32','g34','camera','flags')})==norm({k:nw[k] for k in ('g30','g31','g32','g34','camera','flags')}),(i,dw,nw)
        assert dw['data'][1]['gauge']==nw['data'][1]['gauge'],(i,)
        seen_modes.add(wk['mode']);n+=1;nc_total+=len(ncs)
    print(n,'director updates matched installed PPC;',nc_total,'calls; modes',sorted(seen_modes))
