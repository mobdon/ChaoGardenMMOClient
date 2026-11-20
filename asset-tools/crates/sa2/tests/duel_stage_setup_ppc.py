"""Local PowerPC oracle: run stage 42's setup and teardown and compare duel-stage-setup.

Usage: python duel_stage_setup_ppc.py SA2_ASSETS_EXE   (SA2_GC_REL_DIR = GameCube disc stage RELs; Unicorn 2.1)
Executes stg42D.rel +0x2CF0 (setup) and +0x2F88 (teardown) with their module
helpers under Unicorn. Engine imports are recording stubs: string functions
behave as C strings; loads, allocations and queries return scenario values;
copies and clears are recorded, not performed. Every location the module can
write is pre-filled with a sentinel, and the changed set and values must equal
the Rust result's state. The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);gd=game/"resource/gd_PC";rel=Path(os.environ["SA2_GC_REL_DIR"])/"stg42D.rel"
B,D,T,STOP=0x81000000,0x82000000,0x90000000,0x80f00000
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
S4,S5,S6=D+4*0x400000,D+5*0x400000,D+6*0x400000
SENT=0xa5a5a5a5
def machine():
    u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
    def page(p,n=1):
        for a in range(p&~4095,p+n,4096):
            if a not in seen:u.mem_map(a,4096);seen.add(a)
    u.page=page
    for s in (4,5):
        data=bytearray(d[sec(s):sec(s)+size(s)])
        for (t,off),(m,ts,a,k) in refs.items():
            if t==s and k==1:struct.pack_into('>I',data,off,place(m,ts,a))
        page(D+s*0x400000,len(data));u.mem_write(D+s*0x400000,bytes(data))
    page(S6,size(6));u.mem_write(S6,struct.pack('>I',SENT)*(size(6)//4))
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
    page(T,0x400000);page(0x80fff000,0x1000)
    u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
    return u,imports
def run(scn,mode):
    u,imports=machine()
    put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
    get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
    reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
    fbits=lambda n:struct.unpack('>I',struct.pack('>f',struct.unpack('>d',struct.pack('>Q',u.reg_read(getattr(R,f'UC_PPC_REG_FPR{n}'))))[0]))[0]
    heap=[T+0x1000];names={};written=set()
    def alloc(n,name=None):
        a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,struct.pack('>I',SENT)*((n+3)//4))
        if name:names[a]=name
        return a
    def cstr(p):
        out=b''
        while len(out)<128:
            c=bytes(u.mem_read(p+len(out),1))
            if c==b'\0':break
            out+=c
        return out.decode()
    def desc(v):
        if v==0:return 0
        if v in names:return names[v]
        if B<=v<B+0x400000:return f"s1+0x{v-B:x}"
        if S4<=v<S4+0x400000:return f"s4+0x{v-S4:x}"
        if S5<=v<S5+0x400000:return f"s5+0x{v-S5:x}"
        if S6<=v<S6+0x400000:return f"s6+0x{v-S6:x}"
        return f"0x{v:08x}"
    calls=[];st={'modes':list(scn.get('modeQueries',[])),'tasks':list(scn.get('tasks',[])),'allocs':list(scn.get('allocations',[]))}
    # Player tables: obj1 slots at 0x801e7768, data slots 0x801e7728, presence 0x801e7788.
    p1=[alloc(0x100,f'player1[{i}]') for i in range(2)];p2=[alloc(0x100,f'player2[{i}]') for i in range(2)]
    for i in range(2):put(0x801e7768+4*i,'I',p1[i]);put(0x801e7728+4*i,'I',p2[i])
    rc=scn.get('rivalCharacter');put(0x801e7788+4,'I',alloc(0x10) if rc is not None else 0)
    if rc is not None:put(p2[1]+3,'b',rc)
    names[p2[1]+0x48]='player2[1]+0x48'
    put(0x801cc168+0x11,'b',scn.get('language',0))
    for a,v in zip((0x801a2a18,0x801a2a20),scn.get('callbackSlotsRaw',[0,0])):put(a,'I',v)
    # Module state before the call.
    for off,v in ((0x654,scn.get('fogTaskRaw',0)),(0x698,scn['banksRaw'][0]),(0x69c,scn['banksRaw'][1]),(0,scn.get('setRaw',SENT))):put(S6+off,'I',v)
    src=bytes(scn.get('lightSourceMemory',[]));u.mem_write(0x801e5870,src or bytes(384))
    files={k:(v[0],v[1]) for k,v in scn.get('messageFiles',{}).items()}
    for k,(at,words) in files.items():
        u.page(at,len(words)*4);u.mem_write(at,b''.join(struct.pack('>I',x) for x in words))
    task_names={B+0x3cc:'task0',B+0x3128:'task1',B+0xae04:'task2'}
    def hook(uc,pc,sz,_):
        # CodeWarrior _savegpr_N / _restgpr_N: r(N)..r31 at r11 - 4 * (32 - r).
        if 0x8010af98<=pc<0x8010afe4 or 0x8010afe4<=pc<0x8010b030:
            save=pc<0x8010afe4;first=14+(pc-(0x8010af98 if save else 0x8010afe4))//4;r11=reg(11)
            for r in range(first,32):
                at=r11-4*(32-r)
                if save:put(at,'I',reg(r))
                else:uc.reg_write(getattr(R,f'UC_PPC_REG_{r}'),get(at,'I'))
            return
        a=[reg(i) for i in range(3,10)]
        if pc==0x8010dff8:s=cstr(a[1]);uc.mem_write(a[0],s.encode()+b'\0');written.add(a[0]);return
        if pc==0x8010df88:s=cstr(a[0])+cstr(a[1]);uc.mem_write(a[0],s.encode()+b'\0');return
        if pc==0x8010dda8:
            s=cstr(a[0]);i=s.rfind(chr(a[1]));uc.reg_write(R.UC_PPC_REG_3,a[0]+i if i>=0 else 0);return
        if pc==0x8001dda8:v=st['modes'].pop(0);calls.append(('0x8001dda8',[a[0],v]));uc.reg_write(R.UC_PPC_REG_3,v&0xffffffff);return
        if pc==0x80071e0c:
            made=st['tasks'].pop(0);calls.append(('0x80071e0c',[a[0],a[1],desc(a[2]),cstr(a[3]),made]))
            t=alloc(0x60,task_names[a[2]]) if made else 0;uc.reg_write(R.UC_PPC_REG_3,t);return
        if pc==0x801197fc:
            got=st['allocs'].pop(0);calls.append(('0x801197fc',[a[0],a[1],got]));uc.reg_write(R.UC_PPC_REG_3,alloc(0x20,'alloc0') if got else 0);return
        if pc==0x800709f8:
            calls.append(('0x800709f8',[a[0],desc(a[1])]));t=alloc(0x60,'child');wk=alloc(0x40,'child.work');put(t+0x34,'I',wk);uc.reg_write(R.UC_PPC_REG_3,t);return
        if pc==0x800651ec:calls.append(('0x800651ec',[cstr(a[0]),cstr(a[1]),a[2],scn['setHandle']]));uc.reg_write(R.UC_PPC_REG_3,scn['setHandle']);return
        if pc==0x8001a13c:
            v=scn['lightLoads'][st.setdefault('light',0)];st['light']+=1
            calls.append(('0x8001a13c',[cstr(a[0]),desc(a[1]),v]));uc.reg_write(R.UC_PPC_REG_3,v&0xffffffff);return
        if pc==0x8001a16c:calls.append(('0x8001a16c',[cstr(a[0]),desc(a[1]),scn['fogLoad']]));uc.reg_write(R.UC_PPC_REG_3,scn['fogLoad']&0xffffffff);return
        if pc==0x8001a190:
            n=cstr(a[0]);at=files.get(n,(0,))[0];calls.append(('0x8001a190',[n,at]));uc.reg_write(R.UC_PPC_REG_3,at);return
        if pc in (0x80072820,):calls.append((hex(pc),[cstr(a[0]),desc(a[1])]));return
        if pc in (0x8006d500,):calls.append((hex(pc),[cstr(a[0]),desc(a[1])]));return
        if pc==0x80011050:calls.append((hex(pc),[cstr(a[0])]));return
        if pc in (0x801125c0,0x8011c2fc):calls.append((hex(pc),[fbits(1),fbits(2)]));return
        if pc==0x80117b8c:calls.append((hex(pc),a[:3]));return
        if pc in (0x8000532c,0x80119778):calls.append((hex(pc),[desc(a[0]),a[1],a[2]]));return
        if pc==0x801196ec:calls.append((hex(pc),[desc(a[0]),desc(a[1]),struct.unpack('>i',struct.pack('>I',a[2]))[0]]));return
        if pc in (0x80072270,0x80072218,0x80072118,0x8006638c):calls.append((hex(pc),[desc(a[0]),desc(a[1]) if pc!=0x8006638c else a[1]]));return
        if pc in (0x80086e3c,0x800874c0,0x8001d74c,0x80022e24,0x80011a04,0x8011c308):calls.append((hex(pc),[a[0]]));return
        if pc==0x8011c2c8:calls.append((hex(pc),[struct.unpack('>i',struct.pack('>I',a[0]))[0]]));return
        if pc in (0x80071ae0,0x80071af0,0x80119828,0x8001a2b0):calls.append((hex(pc),[desc(a[0]) if a[0] in names else a[0]]));return
        if pc==0x8003bcc4:calls.append((hex(pc),[desc(a[0])]));return
        calls.append((hex(pc),[]))
    for a in imports|{0x8010af98+4*k for k in range(19)}|{0x8010afe4+4*k for k in range(19)}:
        u.page(a,4);u.mem_write(a,struct.pack('>I',0x4e800020));u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
    # Locations the module may write, with how to read each.
    slots={'player1[1]+0x2':(p1[1]+2,'B'),'player2[1]+0x48':(p2[1]+0x48,'I'),
           's6+0x0':(S6,'I'),'s6+0x14':(S6+0x14,'I'),'s6+0x644':(S6+0x644,'P'),'s6+0x648':(S6+0x648,'I'),'s6+0x650':(S6+0x650,'I'),
           's6+0x654':(S6+0x654,'P'),'s6+0x698':(S6+0x698,'I'),'s6+0x69c':(S6+0x69c,'I'),'s5+0x1747c':(S5+0x1747c,'i'),'s5+0x17480':(S5+0x17480,'i'),
           '0x803adbc8':(0x803adbc8,'P'),'0x801a2a18':(0x801a2a18,'P'),'0x801a2a20':(0x801a2a20,'P'),
           '0x802b7020':(0x802b7020,'S'),'0x802b7000':(0x802b7000,'S')}
    for a in (0x803adbc8,0x802b7020,0x802b7000,S5+0x1747c,S5+0x17480):put(a,'I',SENT)
    u.mem_write(0x801e77d8,struct.pack('>I',SENT)*14)
    for k in range(12):u.mem_write(0x801e5630+0x30*k,struct.pack('>I',SENT)*9)
    before={}
    def read(addr,kind):
        if kind=='S':return cstr(addr)
        if kind=='P':return desc(get(addr,'I'))
        return get(addr,{'B':'B','I':'I','i':'i'}[kind])
    for k,(a,kind) in slots.items():before[k]=bytes(u.mem_read(a,4))
    watch={a for a,_ in slots.values()}
    def wr(uc,access,addr,size,value,_):
        for a in watch:
            if addr<a+4 and a<addr+size:written.add(a)
    u.hook_add(UC_HOOK_MEM_WRITE,wr)
    u.reg_write(R.UC_PPC_REG_1,T+0x3f0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    try:u.emu_start(B+(0x2cf0 if mode=='setup' else 0x2f88),STOP,count=1<<22)
    except Exception as e:raise AssertionError((mode,hex(reg('PC')),[hex(reg(i)) for i in range(3,8)],calls[-3:])) from e
    assert reg('PC')==STOP
    state={}
    for k,(a,kind) in slots.items():
        if a in written:state[k]=read(a,kind)
    tbl=[desc(get(0x801e77d8+4*i,'I')) for i in range(14)]
    if get(0x801e77d8,'I')!=SENT:state['0x801e77d8']=tbl
    for k in range(12):
        row=[get(0x801e5630+0x30*k+4*i,'I') for i in range(9)]
        if row!=[SENT]*9:state[f'0x{0x801e5630+0x30*k:08x}']=row
    for nm,(at,words) in files.items():
        n=words.index(0xffffffff);cur=[get(at+4*i,'I') for i in range(n)]
        if cur!=words[:n]:state[f'message@0x{at:08x}']=cur
    for t,nm in list(names.items()):
        if nm.startswith('player'):continue
        if nm=='alloc0':
            if bytes(u.mem_read(t,1))[0]!=0xa5:state['alloc0+0x0']=bytes(u.mem_read(t,1))[0]
            continue
        for off in (0x40,0x18,0x14,0x1c,0x8,0x28,0x24):
            raw=bytes(u.mem_read(t+off,4))
            if raw==struct.pack('>I',SENT):continue
            v=struct.unpack('>I',raw)[0]
            state[f'{nm}+0x{off:x}']=desc(v) if (v in names or B<=v<B+0x400000) else v
    return calls,state
def rust_calls(cs):
    out=[]
    for c in cs:
        a=c['args'];e=c['engine']
        out.append((e if e!='0x8001dda8' else e,a))
    return out
light=list((gd/"stg42_light.bin").read_bytes())
mh={'mh0042e.prs':[0x900,[0,0x10,0x20,0,0x30,0xffffffff,7]],'mhsysj.prs':[0x80f00800,[0x4,0x8,0xffffffff]]}
cases=[
 ('setup-solo',{'call':'setup','modeQueries':[0],'tasks':[True,True],'allocations':[],'setHandle':0x91234560,'lightSourceMemory':light,'lightLoads':[384,0],'fogLoad':1,
   'fogTask':0,'messageBanks':[0,0],'language':1,'messageFiles':{'mh0042e.prs':[0x80f00400,[0,0x10,0x20,0,0x30,0xffffffff]],'mhsysj.prs':[0x80f00800,[4,8,0xffffffff]]},'callbackSlots':[0,0]}),
 ('setup-rival-sonic',{'call':'setup','modeQueries':[3,1],'rivalCharacter':0,'tasks':[True,True,True],'allocations':[True],'setHandle':0x91234560,'lightSourceMemory':light,'lightLoads':[384,576],'fogLoad':0,
   'fogTask':0,'messageBanks':[0,0],'language':3,'messageFiles':{'mh0042.prs':[0x80f00400,[0x40,0xffffffff]]},'callbackSlots':[0,0x80001234]}),
 ('setup-rival-shadow-fail',{'call':'setup','modeQueries':[5,0],'rivalCharacter':1,'tasks':[True,False,False],'allocations':[False],'setHandle':0,'lightSourceMemory':light,'lightLoads':[-1,0],'fogLoad':1,
   'fogTask':0x91000000,'messageBanks':[0x91000100,0],'language':0,'messageFiles':{},'callbackSlots':[0x80001234,0]}),
 ('teardown',{'call':'teardown','messageBanks':[0x80f00400,0x80f00800],'callbackSlots':['s1+0x19d70','s1+0x9fb8'],'fogTask':0x91000000,'setHandle':0x91234560}),
 ('teardown-empty',{'call':'teardown','messageBanks':[0,0],'callbackSlots':['0x80001234','0'],'fogTask':0,'setHandle':0}),
]
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    for name,c in cases:
        scn=dict(c)
        if c['call']=='setup':
            scn['banksRaw']=c['messageBanks'];scn['fogTaskRaw']=c['fogTask'];scn['callbackSlotsRaw']=c['callbackSlots'];scn['setRaw']=SENT
        else:
            scn['banksRaw']=c['messageBanks'];scn['fogTaskRaw']=c['fogTask'];scn['setRaw']=c['setHandle']
            scn['callbackSlotsRaw']=[B+int(v[3:],16) if str(v).startswith('s1+') else int(str(v),16) for v in c['callbackSlots']]
        native_calls,native_state=run(scn,c['call'])
        req=Path(tmp)/f'{name}.json';out=Path(tmp)/f'{name}-out.json';req.write_text(json.dumps(c))
        r=subprocess.run([sys.argv[1],'duel-stage-setup',str(rel),str(out),str(req)],capture_output=True,text=True);assert r.returncode==0,r.stderr
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        mine=[(x['engine'],x['args']) for x in doc['calls']]
        nat=[(n,list(a)) for n,a in native_calls]
        norm=lambda v:json.loads(json.dumps(v))
        assert norm(mine)==norm(nat),next(((i,a,b) for i,(a,b) in enumerate(zip(norm(mine),norm(nat))) if a!=b),(len(mine),len(nat)))
        assert norm(doc['state'])==norm(native_state),(sorted(set(doc['state'])^set(native_state)),[(k,doc['state'].get(k),native_state.get(k)) for k in doc['state'] if norm(doc['state'][k])!=norm(native_state.get(k))][:4])
        print(name,len(nat),'calls and',len(native_state),'written locations matched installed PPC')
