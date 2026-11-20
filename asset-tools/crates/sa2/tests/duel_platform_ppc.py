"""Local PowerPC oracle: run the installed stage 42 DODAI task and compare duel-platform-update.

Usage: python duel_platform_ppc.py SA2_ASSETS_EXE   (SA2_GAME_PATH = Steam install; Unicorn 2.1)
Executes DODAI's own constructor, update and collision-transform helper (module 41,
section 1 +0x1AD0..+0x2278) under Unicorn. Every engine import is a recording stub
whose return value comes from the scenario. The game is never started.
"""
import hashlib,json,os,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);rel=game/"resource/gd_PC/stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
G,OBJ1,OBJ2=0x801cbd88,0x801e7368,0x801e7328
FIRST,END,ENTRY,DEBRIS=0x1ad0,0x2278,0x1d68,0x1e8dc
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
def fpr(n):return struct.unpack('>f',struct.pack('>f',struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,'UC_PPC_REG_FPR'+str(n)))))[0]))[0]
place=lambda m,s,a:D+s*0x400000+a if m else a
# Data sections 4 and 5 with their local ADDR32 pointers resolved.
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
put(B+DEBRIS,'I',0x4e800020)
page(G,0x100);page(OBJ1,8);page(OBJ2,8);page(T,0x400000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000]
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
bits=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
calls=[];frame={};rands=[];task=[0]
def call(uc,pc,size,_):
    if pc==0x8006538c:calls.append(('taskCheck',));uc.reg_write(R.UC_PPC_REG_3,int(frame.get('taskCheck',False)))
    elif pc==0x80039670:calls.append(('visibility',bits(fpr(1))));uc.reg_write(R.UC_PPC_REG_3,int(frame.get('visible',False)))
    elif pc==B+DEBRIS:
        calls.append(('debris',tuple(get(reg(4)+4*i,'i') for i in range(3)),tuple(bits(fpr(i)) for i in range(1,6))))
        assert reg(3)==work+0x14
    elif pc==0x800c3468:calls.append(('rumble',reg(3),bits(fpr(1)),reg(4)))
    elif pc==0x8006b584:calls.append(('sound',reg(3),reg(4),reg(5),reg(6)));assert reg(7)==work+0x14
    elif pc==0x8010d940:
        v=rands.pop(0);calls.append(('rand',v));uc.reg_write(R.UC_PPC_REG_3,v&0xffffffff)
    elif pc==0x80069af8:calls.append(('fallEffect',reg(3),reg(5),reg(6),reg(7)));assert (reg(4),reg(8))==(work,work+0x14)
    elif pc==0x80071878:calls.append(('free',))
    elif pc==0x80119488:uc.reg_write(R.UC_PPC_REG_3,alloc(reg(3)))
    elif pc==0x8002305c:
        assert reg(4)==task[0];calls.append(('init',reg(3),get(get(reg(5)+4,'I')+0x24,'I')))
for a in imports|{B+DEBRIS}:u.hook_add(UC_HOOK_CODE,call,begin=a,end=a)
def run(case):
    global work
    task[0]=alloc(0x60);work=alloc(0x40);motion=alloc(0x38);t=task[0]
    put(t+0x34,'I',work);put(t+0x3c,'I',motion);put(t+0x30,'I',int(case.get('taskField30',False)))
    sp=case['spawn'];put(work+0x14,'f',sp['x']);put(work+6,'H',sp['timer']);put(work+3,'B',int(sp['activated']))
    p1,p2=alloc(0x40),alloc(0x100);put(OBJ1,'I',p1);put(OBJ2,'I',p2)
    rands[:]=list(case.get('rand',[]));out=[]
    for f in case['frames']:
        frame.clear();frame.update(f);calls.clear()
        put(p1+4,'H',int(f.get('playerFlag',False)));put(p2+0xa0,'I',t if f.get('standing') else 0)
        put(G+0x37,'b',int(f.get('paused',False)));put(G+0x42,'b',f.get('gameByte42',0))
        u.reg_write(R.UC_PPC_REG_1,T+0x3f0000);u.reg_write(R.UC_PPC_REG_3,t);u.reg_write(R.UC_PPC_REG_LR,STOP)
        u.emu_start(B+ENTRY,STOP,count=1<<20);assert reg('PC')==STOP
        obj=get(t+0x40,'I')
        state=(get(work,'b'),get(work+3,'B'),get(work+4,'H'),get(work+6,'H'),tuple(get(work+8+4*i,'i') for i in range(3)),
               tuple(get(work+0x14+4*i,'I') for i in range(3)),get(obj+0xc,'I'),tuple(get(obj+0x14+4*i,'i') for i in range(3)),
               tuple(get(motion+4*i,'I') for i in range(14)))
        out.append((state,list(calls)))
    return out
def rust(c):
    calls=[]
    for x in c:
        n=x['call']
        if n=='init':calls.append((n,x['collisionFlags'],bits(x['modelRadius'])))
        elif n=='visibility':calls.append((n,bits(x['radius'])))
        elif n=='debris':calls.append((n,tuple(x['angles']),tuple(map(bits,x['args']))))
        elif n=='rumble':calls.append((n,0,bits(x['strength']),x['frames']))
        elif n in ('sound','fallEffect'):calls.append((n,*x['args']))
        elif n=='rand':calls.append((n,x['value']))
        else:calls.append((n,))
    return calls
F=lambda **k:k
stand=[F(visible=i%3>0) for i in range(6)]+[F(visible=True,standing=True)]+[F(visible=True,standing=True,playerFlag=True,paused=40<=i<50,gameByte42=1 if 60<=i<64 else 0) for i in range(200)]
fall=[F(visible=i%5>0) for i in range(760)]+[F(taskCheck=True)]
cases={
 'stand-and-fall':{'spawn':{'x':900.0,'timer':(-360)&0xffff,'activated':False},'rand':[12345,32767,0],'frames':stand+fall},
 'behind':{'spawn':{'x':300.0,'timer':1160,'activated':False},'taskField30':True,'rand':[1,16384,30000],
           'frames':[F(visible=True)]*3+[F(taskCheck=True)]+[F(visible=True)]*780},
 'late':{'spawn':{'x':6000.0,'timer':900,'activated':True},'rand':[7,8,9],'frames':[F()]*4+[F(gameByte42=2)]*700},
}
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    total=0
    for name,case in cases.items():
        native=run(case);req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json'
        req.write_text(json.dumps(case))
        subprocess.run([sys.argv[1],'duel-platform-update',str(rel),str(out),str(req)],check=True)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        names=set()
        for i,(v,(state,nc)) in enumerate(zip(doc['frames'],native,strict=True)):
            x=v['work']
            mine=(x['mode'],x['activated'],x['flags'],x['timer'],tuple(x['angles']),tuple(map(bits,x['position'])),
                  bits(x['objectY']),tuple(x['objectAngles']),tuple(x['motion']))
            assert mine==state,(name,i,mine,state)
            assert rust(v['calls'])==nc,(name,i,v['calls'],nc)
            names|={c[0] for c in nc}
        last=native[-1][0]
        assert last[0]==2 and struct.unpack('>f',struct.pack('>I',last[5][1]))[0]<-100,(name,last)
        print(name,len(native),'updates matched installed PPC; calls',sorted(names))
        total+=len(native)
    print(total,'DODAI updates matched')
