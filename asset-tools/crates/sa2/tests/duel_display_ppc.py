"""Local PowerPC oracle: run stage 42's display callbacks and compare duel-display.

Usage: python duel_display_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes the platform display/delete (+0x19C8, +0x1A58), meteor (+0x25BC),
debris (+0x91A8), BgExec (+0x312C), gauge (+0x20700) and title glyph/window
(+0x2140C, +0x21A00) displays under Unicorn with recording engine stubs and
compares every engine call (pointer arguments as the data they point to) and
every write to the scratch buffer, the module data and the task.
The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn import ppc_const as R
rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,H,T,STOP,SCR=0x81100000,0x86000000,0x90000000,0x91000000,0x80f00000,0x80fffe60
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
base=lambda s:D+s*0x400000
place=lambda m,s,a:(B+a if s==1 else base(s)+a) if m else a
S5,S6=base(5),base(6)
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
sng=lambda x:struct.unpack('>f',struct.pack('>f',x))[0]
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
hx=lambda x:f'0x{fb(x):08x}'
def fbits(n):
    v=struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,f'UC_PPC_REG_FPR{n}'))))[0]
    return fb(v)
def setf(n,x):u.reg_write(getattr(R,f'UC_PPC_REG_FPR{n}'),struct.unpack('>Q',struct.pack('>d',x))[0])
data5=None
for s in (4,5):
    data=bytearray(d[sec(s):sec(s)+size(s)])
    for (t,off),(m,ts,a,k) in refs.items():
        if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
    page(base(s),len(data));u.mem_write(base(s),bytes(data))
    if s==5:data5=bytes(data)
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
SAVE=[0x8010af98+4*k for k in range(19)];REST=[0x8010afe4+4*k for k in range(19)]
for a in imports|{STOP}|set(SAVE)|set(REST):page(a,4);u.mem_write(a,struct.pack('>I',0x4e800020))
page(H,0x100000);page(T,0x100000);page(0x80ff0000,0x20000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
GAME=0x801cc168
TASK,WORK,OBJ,MODEL,PARENT,PWORK,OTHER,ENTRY,LINE,CAM=(H+0x100*i for i in range(1,11))
PIECES=H+0x10000
NAMES={TASK:'task',OBJ:'object',MODEL:'model'}
def sym(v):
    if S5<=v<S5+size(5):return f's5+0x{v-S5:x}'
    if v==SCR:return 'scratch'
    return NAMES.get(v,v)
events=[];inp={}
def vec(p,f):return [get(p+4*i,f) for i in range(3)]
SPEC={0x8002b2e8:'i',0x8002b2d8:'i',0x80116e18:'p',0x8011616c:'V',0x80115f68:'Ai',0x80066820:'iVf',0x800665a4:'VA',
      0x80121b64:'p',0x8011e158:'M',0x8011e17c:'p',0x8011df20:'p',0x80121bdc:'p',0x80031a28:'p',0x8011538c:'iV',
      0x801218c8:'ii',0x800156fc:'ffff',0x80115bd8:'iF',0x80115178:'iF',0x80024cb8:'i',0x801179f8:'ii',0x80115c60:'ii',
      0x80115828:'ii',0x80115dc8:'i',0x8011e8e4:'Sifi',0x8011c508:'Qiii',0x80117c80:'i',0x800231cc:'pp',0x80119828:'p',
      0x80112398:'i',0x800334b0:'iif',0x80111240:'f'}
def hook(uc,pc,sz,_):
    if pc in SAVE or pc in REST:
        save=pc in SAVE;first=14+(pc-(SAVE[0] if save else REST[0]))//4;r11=reg(11)
        for r in range(first,32):
            at=r11-4*(32-r)
            if save:put(at,'I',reg(r))
            else:uc.reg_write(getattr(R,f'UC_PPC_REG_{r}'),get(at,'I'))
        return
    spec=SPEC.get(pc,'');ir=3;fr=1;args=[]
    for c in spec:
        if c=='f':args.append(fbits(fr));fr+=1;continue
        if c=='F':args.append([fbits(fr+i) for i in range(3)]);fr+=3;continue
        v=reg(ir);ir+=1
        if c=='i':args.append(v)
        elif c=='p':args.append(sym(v))
        elif c=='V':args.append(vec(v,'I'))
        elif c=='A':args.append(vec(v,'i'))
        elif c=='M':args+= [sym(v),sym(get(v,'I'))]
        elif c=='Q':args.append([get(v+4*i,'I') for i in range(24)])
        elif c=='S':
            s=[get(v+4*i,'I') for i in range(8)];s[6]=sym(s[6]);an=s[7];s[7]='tanim'
            args+=[s,[get(an+4*i,'I') for i in range(5)]]
    if pc==0x80112398:args.append(fb(inp['sin']));setf(1,inp['sin'])
    if pc==0x80111240:args.append(fb(inp['floor']));setf(1,inp['floor'])
    if pc==0x800334b0:args.append(inp['color']);uc.reg_write(R.UC_PPC_REG_3,inp['color'])
    events.append({'engine':f'0x{pc:08x}','args':args})
for a in imports|set(SAVE)|set(REST):u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
def wr(uc,acc,a,sz,v,_):
    if sz!=4:
        if 0x80ff0000<=a<0x81010000 or S5<=a<S5+size(5) or a in (TASK+0x40,OBJ+4):raise AssertionError(('narrow write',hex(a)))
        return
    v&=0xffffffff
    if 0x80ff0000<=a<0x81010000:events.append({'write':f'scratch+0x{a-SCR:x}','value':v})
    elif S5<=a<S5+size(5):events.append({'write':f's5+0x{a-S5:x}','value':sym(v)})
    elif a==TASK+0x40:events.append({'write':'task+0x40','value':v})
    elif a==OBJ+4:events.append({'write':'object+4','value':v})
u.hook_add(UC_HOOK_MEM_WRITE,wr)
ENTRY_PC={'platform':0x19c8,'platformDelete':0x1a58,'meteor':0x25bc,'debris':0x91a8,'background':0x312c,'gauge':0x20700,
          'titleGlyph':0x2140c,'titleWindow':0x21a00}
fl=lambda x:struct.unpack('>f',bytes.fromhex(x[2:]))[0]
def title_state(c):
    put(GAME+0x39,'b',c['menu']);put(GAME+0xb,'b',c['enabled']);put(S6+0x6d4,'I',c['texlist'])
    p=c.get('parent')
    put(TASK+8,'I',PARENT if p else 0);put(TASK+4,'I',0);put(OTHER+4,'I',0)
    if p:
        put(PARENT+0x34,'I',PWORK);put(PWORK+4,'H',0x200 if p['hidden'] else rng.choice([0,0x1ff,0xfdff]))
        child=TASK if p['first'] else OTHER;put(PARENT+0xc,'I',child)
        put(child+4,'I',TASK if p['last'] else OTHER+0x40)
def setup(c):
    k=c['callback'];put(TASK+0x34,'I',WORK)
    if k=='platform':
        put(TASK+0x40,'I',OBJ);put(OBJ+4,'I',MODEL)
        for i in range(3):put(OBJ+8+4*i,'f',fl(c['position'][i]));put(OBJ+0x14+4*i,'i',c['angles'][i])
        put(MODEL+0x24,'f',fl(c['radius']))
    elif k=='platformDelete':
        put(TASK+0x40,'I',OBJ if c['object'] else 0);put(OBJ+4,'I',MODEL if c['model'] else 0)
    elif k=='meteor':
        put(WORK+6,'H',c['timer'])
        for i in range(3):put(WORK+8+4*i,'i',c['angles'][i]);put(WORK+0x14+4*i,'f',fl(c['position'][i]))
        inp['sin']=fl(c['sin']);inp['color']=c['color']
    elif k=='debris':
        put(0x803ad926,'b',c['gameByte'])
        nxt=0
        for i,p in reversed(list(enumerate(c['pieces']))):
            a=PIECES+0x60*i
            for j in range(3):put(a+4*j,'f',fl(p['position'][j]));put(a+0x18+4*j,'i',p['angles'][j])
            put(a+0x30,'I',p['texlist']);put(a+0x34,'I',S5+p['object'] if p.get('object') is not None else 0)
            put(a+0x38,'I',S5+p['model'] if p.get('model') is not None else 0)
            put(a+0x44,'I',p['flags']);put(a+0x48,'f',fl(p['life']));put(a+0x4c,'I',nxt);nxt=a
        put(S6+0x63c,'I',nxt)
    elif k=='background':
        put(GAME+0x3d,'b',c['gameByte']);put(0x803adad0,'i',c['screen'])
        put(0x80175378+4*(c['screen']&3),'I',CAM)
        for i in range(3):put(CAM+4*i,'f',fl(c['camera'][i]))
        put(S6+4,'f',fl(c['bss4']));put(S6+8,'f',fl(c['bss8']));put(GAME+0x7c,'I',c['frame'])
        inp['floor']=fl(c['floor'])
    elif k=='gauge':
        put(GAME+0x39,'b',c['menu']);wk=c['work']
        put(WORK,'b',wk['mode']);put(WORK+0x14,'f',fl(wk['shown']));put(WORK+0x28,'f',fl(wk['start']))
    elif k=='titleGlyph':
        title_state(c);g=c['glyph'];put(WORK+8,'I',ENTRY)
        put(ENTRY,'I',g['code']);put(ENTRY+4,'h',g['width']);put(ENTRY+6,'h',g['height'])
        put(WORK+0xc,'I',g['color'])
        for o,n in ((0x14,'x'),(0x18,'y'),(0x20,'scale'),(0x24,'sx'),(0x28,'sy')):put(WORK+o,'f',fl(g[n]))
    elif k=='titleWindow':
        title_state(c);wd=c['window'];put(WORK+1,'b',wd['count']);put(WORK+8,'I',LINE);put(WORK+0xc,'I',wd['color'])
        for o,n in ((0x14,'x'),(0x18,'y'),(0x20,'scale'),(0x28,'alpha')):put(WORK+o,'f',fl(wd[n]))
        for i,code in enumerate(wd['line']):put(LINE+16*i,'I',code)
def run(c):
    events.clear();inp.clear();setup(c)
    u.reg_write(R.UC_PPC_REG_3,TASK);u.reg_write(R.UC_PPC_REG_1,T+0xf0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    u.emu_start(B+ENTRY_PC[c['callback']],STOP,count=1<<22);u.mem_write(S5,data5)
    if reg('PC')!=STOP:
        # The vertex copies spin on a list that does not end in 0xFF.
        assert reg('PC')-B in (0x5c20,0x5c24,0x5f90,0x5f94),hex(reg('PC'));return 'hang'
    return json.loads(json.dumps(events))
rng=random.Random(42)
F=lambda lo,hi:hx(sng(rng.uniform(lo,hi)))
ptr5={off:v for (s,off),(m,ts,v,k) in refs.items() if s==5 and m==w(0) and ts==5 and k==1}
d8=[m for m,vl in ptr5.items() if vl+8<=size(5) and struct.unpack_from('>I',data5,vl)[0]&0xffff==0x23 and m+0x18<=size(5)]
clean=lambda m:all((5,m+o) not in refs for o in (8,12,16)) and m+0x18<=size(5)
d8=[m for m in d8 if clean(m)]
plain=[m for m,vl in ptr5.items() if vl+8<=size(5) and (5,vl) not in refs and struct.unpack_from('>I',data5,vl)[0]&0xffff!=0x23 and clean(m)][:40]
cases=[]
ang=lambda:[rng.randrange(-1<<31,1<<31) for _ in range(3)]
for _ in range(20):cases.append({'callback':'platform','position':[F(-5e4,5e4) for _ in range(3)],'angles':ang(),'radius':F(0,500)})
for o in (False,True):
    for m in (False,True):cases.append({'callback':'platformDelete','object':o,'model':m})
for _ in range(60):
    t=rng.choice([0,0,1,2,30,0x33,0x199,0xffff,rng.randrange(1<<16)])
    cases.append({'callback':'meteor','timer':t,'angles':ang(),'position':[F(-5e4,5e4) for _ in range(3)],
                  'sin':rng.choice([F(-1,1),hx(0.0),hx(-1.0),hx(1.0)]),'color':rng.choice([0xffffffff,rng.randrange(1<<32),0x80ffffff])})
def piece():
    flags=rng.choice([0x2000,0x2000,0,0x10000,0x80000,0x82000,0x12000,0x92000,rng.randrange(1<<32)&~0x8000])
    return {'position':[F(-5e4,5e4) for _ in range(3)],'angles':ang(),'texlist':rng.randrange(0x80000000,0x80800000),
            'object':rng.choice(d8+plain[:10]),'flags':flags,'life':rng.choice([F(-2,3),hx(0.0),F(0,1),F(1,40)])}
raw5=lambda a:struct.unpack_from('>I',d,sec(5)+a)[0]
def gc_ok(m):
    if any((5,m+o) in refs for o in (0x14,0x18,0x1c)) or m+0x20>size(5) or m not in ptr5:return False
    e=ptr5[m]
    for _ in range(64):
        if e+16>size(5) or (5,e) in refs:return False
        if raw5(e)>>24==0xff:return True
        if raw5(e)>>24==3 and ((5,e+4) in refs or raw5(e+4)&0xff==0xa8):return False
        e+=16
    return False
gcm=[m for m in ptr5 if gc_ok(m)][:12]
assert gcm
for _ in range(12):
    ps=[piece() for _ in range(3)]
    for q in ps:
        if rng.random()<.6:q['flags']|=0x8000;q['model']=rng.choice(gcm)
    cases.append({'callback':'debris','pieces':ps,'gameByte':rng.randrange(-128,128)})
for _ in range(40):cases.append({'callback':'debris','pieces':[piece() for _ in range(rng.randrange(0,5))],'gameByte':rng.randrange(-128,128)})
for _ in range(40):
    cases.append({'callback':'background','gameByte':rng.choice([0,1,2,3,4,-1]),'screen':rng.choice([0,1]),
                  'camera':[F(-3e4,3e4) for _ in range(3)],'floor':hx(float(rng.randrange(-40,40))),'bss4':F(-500,500),'bss8':F(-500,500),
                  'frame':rng.randrange(1<<32)})
for _ in range(80):
    cases.append({'callback':'gauge','menu':rng.choice([0,0,0,1]),'work':{'mode':rng.choice([0,1,2,3,3,4,-1]),
                  'shown':rng.choice([F(-10,200),F(0,40),hx(0.0),F(0,500)]),'start':rng.choice([F(0,1),F(0,8),hx(0.0),F(-1,0)])}})
def tstate():
    return {'menu':rng.choice([0,0,0,1]),'enabled':rng.choice([1,1,1,0,-3]),'texlist':rng.randrange(1<<32),
            'parent':rng.choice([None,{'hidden':rng.random()<.2,'first':rng.random()<.5,'last':rng.random()<.5}])}
for _ in range(60):
    c={'callback':'titleGlyph',**tstate(),'glyph':{'x':F(0,640),'y':F(0,480),'scale':F(0,3),'sx':F(0,2),'sy':F(0,2),
       'color':rng.randrange(1<<32),'code':rng.randrange(1<<32),'width':rng.randrange(-40,200),'height':rng.randrange(-40,200)}}
    cases.append(c)
for _ in range(60):
    n=rng.randrange(0,12)
    c={'callback':'titleWindow',**tstate(),'window':{'count':n,'x':F(0,640),'y':F(0,480),'scale':F(0,3),'alpha':F(-0.5,1.5),
       'color':rng.randrange(1<<32),'line':[rng.randrange(1<<32) for _ in range(n)]}}
    cases.append(c)
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    kinds={}
    for i,c in enumerate(cases):
        try:native=run(c)
        except Exception as e:raise SystemExit(f'case {i} {c}: {e}')
        req=Path(tmp)/f'c{i}.json';out=Path(tmp)/f'o{i}.json';req.write_text(json.dumps(c))
        r=subprocess.run([sys.argv[1],'duel-display',str(rel),str(out),str(req)],capture_output=True,text=True)
        if native=='hang':
            assert r.returncode!=0 and ('loops forever' in r.stderr or 'relocated pointer' in r.stderr),(i,r.stderr);kinds['hang']=kinds.get('hang',0)+1;continue
        assert r.returncode==0,(i,c,r.stderr)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        mine=doc['calls']
        if mine!=native:
            for j,(a,b2) in enumerate(zip(mine,native)):
                if a!=b2:print('first difference',j,'\nport  ',a,'\nnative',b2);break
            raise SystemExit(f'case {i} {c["callback"]}: {len(mine)} vs {len(native)} events')
        kinds[c['callback']]=kinds.get(c['callback'],0)+len(native)
    print(len(cases),'display callbacks matched installed PPC; events',kinds)
