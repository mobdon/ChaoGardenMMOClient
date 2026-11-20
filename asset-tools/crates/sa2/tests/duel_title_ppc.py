"""Local PowerPC oracle: run the boss title card system and compare duel-title.

Usage: python duel_title_ppc.py SA2_ASSETS_EXE   (SA2_GAME_PATH = Steam install; Unicorn 2.1)
Executes stg42D.rel BossTitleExec create (+0x22070), update (+0x21DE4),
delete (+0x2200C), glyph update (+0x211FC) and window update (+0x218AC) with
their helpers under Unicorn, for both stage 42 descriptors and a synthetic
descriptor with a window line. The game is never started.
"""
import hashlib,json,os,random,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]);rel=game/"resource/gd_PC/stg42D.rel"
B,D,T,STOP,G=0x81000000,0x82000000,0x90000000,0x80f00000,0x801cbd88
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
place=lambda m,s,a:(B+a if s==1 else D+s*0x400000+a) if m else a
S5,S6=D+5*0x400000,D+6*0x400000
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN);seen=set()
def page(p,n=1):
    for a in range(p&~4095,p+n,4096):
        if a not in seen:u.mem_map(a,4096);seen.add(a)
put=lambda p,f,v:u.mem_write(p,struct.pack('>'+f,v))
get=lambda p,f:struct.unpack('>'+f,u.mem_read(p,struct.calcsize('>'+f)))[0]
reg=lambda n:u.reg_read(getattr(R,'UC_PPC_REG_'+str(n)))
def setf(n,v):v=struct.unpack('>f',struct.pack('>f',v))[0];u.reg_write(getattr(R,f'UC_PPC_REG_FPR{n}'),struct.unpack('>Q',struct.pack('>d',v))[0])
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
SAVE=[0x8010ac50+4*k for k in range(19)];REST=[0x8010ac9c+4*k for k in range(19)]
for a in imports|{STOP}|set(SAVE)|set(REST):page(a,4);put(a,'I',0x4e800020)
page(T,0x200000)
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
heap=[T+0x1000];calls=[];st={}
def alloc(n):
    a=heap[0];heap[0]+=(n+15)&~15;u.mem_write(a,bytes(n));return a
def hook(uc,pc,sz,_):
    if pc in SAVE or pc in REST:
        save=pc in SAVE;first=14+(pc-(SAVE[0] if save else REST[0]))//4;r11=reg(11)
        for r in range(first,32):
            at=r11-4*(32-r)
            if save:put(at,'I',reg(r))
            else:uc.reg_write(getattr(R,f'UC_PPC_REG_{r}'),get(at,'I'))
        return
    E=lambda args:calls.append({'engine':f'0x{pc:08x}','args':args})
    if pc in (0x80071ba4,0x80070790):
        upd=reg(5) if pc==0x80071ba4 else reg(4)
        kind={B+0x21de4:'title',B+0x218ac:'window',B+0x211fc:'glyph'}[upd]
        ok=st['inputs'].get({'title':'task','window':'windowTask','glyph':'glyph'}[kind],True)
        if pc==0x80071ba4:E([reg(3),reg(4),f's1+0x{upd-B:x}','BossTitleExec',ok])
        else:E([reg(3),f's1+0x{upd-B:x}','parent' if kind=='title' else 'task',ok])
        if not ok:uc.reg_write(R.UC_PPC_REG_3,0);return
        t=alloc(0x60);wk=alloc(0x40);put(t+0x34,'I',wk);put(t+8,'I',st.get('title',0))
        if kind=='title':
            st['title']=t;i=st['inputs'];put(wk+4,'h',i['flags']);put(wk+0x24,'f',i['fade']);put(wk+0x28,'f',i['fadeRate'])
        elif kind=='window':st['window']=t
        else:st['glyphs'].append(t)
        uc.reg_write(R.UC_PPC_REG_3,t);return
    if pc==0x801194b4:
        n=reg(3)*reg(4);key={0x2c:'copy'}.get(n) if reg(3)==1 else None
        if key is None:key='lineBuffer' if st['bufs']==('line' if st['hasWindow'] is False else 'line') and False else None
        which=st['allocs'].pop(0);ok=st['inputs'][which]
        E([reg(3),reg(4),ok]);uc.reg_write(R.UC_PPC_REG_3,alloc(max(n,0x10)+0x40) if ok else 0)
        if ok and which=='lineBuffer':st['lineBuf']=reg(3)
        return
    if pc==0x8001dcf4:E([st['inputs']['modeQuery']]);uc.reg_write(R.UC_PPC_REG_3,st['inputs']['modeQuery']&0xffffffff);return
    if pc in (0x80071878,0x80071888):E(['task']);return
    if pc==0x8006b710:E([struct.unpack('>i',struct.pack('>I',reg(3)))[0],reg(4),reg(5),reg(6)]);return
    if pc==0x801194e0:E(['line' if reg(3)==get(st['copy'],'I') else 'windowLine']);return
    raise AssertionError(hex(pc))
for a in imports|set(SAVE)|set(REST):u.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
def run(entry,r3,r4=0,r10=0):
    u.reg_write(R.UC_PPC_REG_3,r3);u.reg_write(R.UC_PPC_REG_4,r4);u.reg_write(R.UC_PPC_REG_10,r10)
    u.reg_write(R.UC_PPC_REG_1,T+0x1f0000);u.reg_write(R.UC_PPC_REG_LR,STOP)
    u.emu_start(B+entry,STOP,count=1<<20);assert reg('PC')==STOP
fb=lambda x:struct.unpack('>I',struct.pack('>f',x))[0]
def glyphs(p,n):
    return [{'code':get(p+16*i,'I'),'width':get(p+16*i+4,'h'),'height':get(p+16*i+6,'h'),'delay':get(p+16*i+8,'H'),
             'scaleX':get(p+16*i+0xa,'h'),'scaleY':get(p+16*i+0xc,'h'),'rest':get(p+16*i+0xe,'H')} for i in range(n)]
def snapshot():
    t=st['title'];wk=get(t+0x34,'I');c=get(t+0x40,'I')
    line=get(c,'I');n=get(c+4,'h');wl=get(c+0xc,'I');wn=get(c+0x10,'h')
    entry=(get(wk+8,'I')-line)//16
    out={'desc':{'count':n,'spacing':get(c+6,'h'),'delay':get(c+8,'h'),'windowCount':wn,'flags':get(c+0x12,'H'),'life':get(c+0x14,'H'),
                 'x':get(c+0x1c,'I'),'y':get(c+0x20,'I'),'scale':get(c+0x24,'I'),'color':get(c+0x28,'I')},
         'line':glyphs(line,n),'windowLine':glyphs(wl,wn) if wl else None,
         'work':{'index':get(wk+1,'b'),'countdown':get(wk+3,'B'),'flags':get(wk+4,'h'),'life':get(wk+6,'H'),'entry':entry,'sound':get(wk+0x10,'i'),
                 'x':get(wk+0x14,'I'),'y':get(wk+0x18,'I'),'fade':get(wk+0x24,'I'),'fadeRate':get(wk+0x28,'I')},
         'glyphs':[{'index':get(gw+6,'H'),'entry':(get(gw+8,'I')-line)//16,'x':get(gw+0x14,'I'),'y':get(gw+0x18,'I'),'k':get(gw+0x1c,'I'),'scale':get(gw+0x20,'I'),
                    'sx':get(gw+0x24,'I'),'sy':get(gw+0x28,'I'),'color':get(gw+0xc,'I')} for gw in (get(g+0x34,'I') for g in st['glyphs'])],
         'window':None}
    if st.get('window'):
        ww=get(st['window']+0x34,'I')
        out['window']={'count':get(ww+1,'b'),'timer':get(ww+6,'H'),'x':get(ww+0x14,'I'),'y':get(ww+0x18,'I'),'k':get(ww+0x1c,'I'),
                       'scale':get(ww+0x20,'I'),'alpha':get(ww+0x28,'I'),'color':get(ww+0xc,'I')}
    return out
def rust_title(t):
    if t is None:return None
    t=json.loads(json.dumps(t));F=lambda v:fb(v)
    for k in ('x','y','scale'):t['desc'][k]=F(t['desc'][k])
    for k in ('x','y','fade','fadeRate'):t['work'][k]=F(t['work'][k])
    for g in t['glyphs']:
        for k in ('x','y','k','scale','sx','sy'):g[k]=F(g[k])
    if t['window']:
        for k in ('x','y','k','scale','alpha'):t['window'][k]=F(t['window'][k])
    d=t['desc'];t['desc']={k:d[k] for k in ('count','spacing','delay','windowCount','flags','life','x','y','scale','color')}
    return t
# A synthetic descriptor with a window line, written into spare module data.
SYN=0x21100
def write_syn():
    lt,wt=S5+SYN+0x40,S5+SYN+0x100
    ents=[(0x1,0x10,0x10,0,0x10,0x10),(0x2,0x8,0x12,3,0x20,0x10),(0x0,0x14,0x10,2,0x10,0x8),(0x1,0x10,0x10,0,0x10,0x10),(0x2a,0x1d,0x10,1,0x20,0x10)]
    for i,(c,wd,h,dl,sx,sy) in enumerate(ents):u.mem_write(lt+16*i,struct.pack('>IhhHhhH',c,wd,h,dl,sx,sy,0))
    for i in range(3):u.mem_write(wt+16*i,struct.pack('>IhhHhhH',[7,1,2][i],0x18,0x10,0,0x10,0x10,0))
    u.mem_write(S5+SYN,struct.pack('>IhhhhIhHHHIfffI',lt,5,4,0,0,wt,3,0x6,0x30,0,S5+0x78,300.0,200.0,1.5,0x00ffeedd))
def rust_descriptor_patch():
    return None
rng=random.Random(8)
P=lambda:{'g37':rng.random()<0.05,'g09':rng.random()<0.05,'g39':rng.random()<0.05}
scenarios=[]
for desc,win in ((0x1c0,False),(0x1ec,False),(SYN,True),(SYN,True)):
    inputs={'active':False,'parent':rng.random()<0.5,'task':True,'copy':True,'lineBuffer':True,'windowBuffer':True,'windowTask':True,
            'modeQuery':rng.choice([0x3c,0x10,0x50]),'flags':0,'fade':1.0,'fadeRate':0.05,'r10':rng.randrange(1<<32)}
    ops=[{'op':'create','descriptor':desc,'inputs':inputs}]
    flags=0
    for f in range(200):
        if f==120:flags|=0x400
        if 60<=f<64:flags|=0x200
        if f==64:flags&=~0x200
        ops.append({'op':'update','pauses':P(),'flags':flags,'beyond':rng.randrange(65536)})
        ops.append({'op':'glyphs','pauses':P(),'flags':flags})
        if win:ops.append({'op':'window','pauses':P(),'flags':flags})
    ops.append({'op':'delete'})
    scenarios.append(ops)
for which in ('task','copy','lineBuffer','windowBuffer'):
    inputs={'active':False,'parent':False,'task':which!='task','copy':which!='copy','lineBuffer':which!='lineBuffer','windowBuffer':which!='windowBuffer',
            'windowTask':True,'modeQuery':0,'flags':0,'fade':1.0,'fadeRate':0.05,'r10':0}
    scenarios.append([{'op':'create','descriptor':SYN,'inputs':inputs}])
scenarios.append([{'op':'create','descriptor':0x1c0,'inputs':dict(scenarios[0][0]['inputs'],active=True)}])
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    total=0
    for si,ops in enumerate(scenarios):
        heap[0]=T+0x1000;write_syn();put(S6+0x6d0,'I',0);put(S6+0x6d4,'I',0)
        st.clear();st.update(glyphs=[],bufs=None,hasWindow=None)
        native=[];rust_ops=[];beyond_used={}
        for op in ops:
            calls.clear()
            if op['op']=='create':
                i=op['inputs'];st['inputs']=i
                if i['active']:put(S6+0x6d0,'I',T+0x900)
                desc=S5+op['descriptor'];has_win=get(desc+0xc,'I')!=0
                st['allocs']=['copy']+(['windowBuffer'] if has_win else [])+['lineBuffer']
                parent=alloc(0x60) if i['parent'] else 0
                run(0x22070,parent,desc,i['r10'])
                st['copy']=get(st['title']+0x40,'I') if st.get('title') and i['task'] and i['copy'] else 0
                ok=get(S6+0x6d0,'I')!=0 and not i['active']
                native.append((list(calls),snapshot() if ok else None));rust_ops.append(op)
                if not ok:break
            elif op['op']=='update':
                put(get(st['title']+0x34,'I')+4,'h',op['flags'])
                for k,o in (('g37',0x37),('g09',9),('g39',0x39)):put(G+o,'b',int(op['pauses'][k]))
                c=get(st['title']+0x40,'I');n=get(c+4,'h');lb=get(c,'I');put(lb+16*n+8,'H',op['beyond'])
                run(0x21de4,st['title'])
                freed=any(x['engine']=='0x80071878' for x in calls)
                native.append((list(calls),snapshot()));rust_ops.append(op)
                if freed:break
            elif op['op']=='glyphs':
                for gi,g in enumerate(list(st['glyphs'])):
                    calls.clear();put(get(st['title']+0x34,'I')+4,'h',op['flags'])
                    for k,o in (('g37',0x37),('g09',9),('g39',0x39)):put(G+o,'b',int(op['pauses'][k]))
                    run(0x211fc,g)
                    native.append((list(calls),snapshot()));rust_ops.append({'op':'glyph','index':gi,'pauses':op['pauses'],'flags':op['flags']})
            elif op['op']=='window':
                put(get(st['title']+0x34,'I')+4,'h',op['flags'])
                for k,o in (('g37',0x37),('g09',9),('g39',0x39)):put(G+o,'b',int(op['pauses'][k]))
                run(0x218ac,st['window'])
                native.append((list(calls),snapshot()));rust_ops.append({'op':'window','pauses':op['pauses'],'flags':op['flags']})
            else:
                run(0x2200c,st['title'])
                native.append((list(calls),None));rust_ops.append({'op':'delete'})
        for ro in rust_ops:
            if ro['op']=='create' and ro.get('descriptor')==SYN:
                desc=S5+SYN;n=get(desc+4,'h');wn=get(desc+0x10,'h')
                ro['inline']={'line':glyphs(get(desc,'I'),n),'count':n,'spacing':get(desc+6,'h'),'delay':get(desc+8,'h'),'window':glyphs(get(desc+0xc,'I'),wn),
                              'windowCount':wn,'flags':get(desc+0x12,'H'),'life':get(desc+0x14,'H'),'texlist':0x78,'x':300.0,'y':200.0,'scale':1.5,'color':get(desc+0x28,'I')}
                del ro['descriptor']
        req=Path(tmp)/f'r{si}.json';out=Path(tmp)/f'o{si}.json';req.write_text(json.dumps({'ops':rust_ops}))
        if si>=4 or op is None:pass
        r=subprocess.run([sys.argv[1],'duel-title',str(rel),str(out),str(req)],capture_output=True,text=True)
        if r.returncode!=0 and 'SYN' in str(r.stderr):pass
        assert r.returncode==0,(si,r.stderr)
        doc=json.loads(out.read_text(encoding='utf-8-sig'));doc=doc.get('document',doc)
        for k,(v,(nc,ns)) in enumerate(zip(doc['ops'],native,strict=True)):
            mine=json.loads(json.dumps([c for c in v['calls'] if 'engine' in c]))
            assert mine==json.loads(json.dumps(nc)),(si,k,rust_ops[k]['op'],mine,nc)
            if ns is not None:
                rt=rust_title(v['title']);
                for key in ('desc','line','windowLine','work','glyphs','window'):
                    assert json.loads(json.dumps(rt[key]))==json.loads(json.dumps(ns[key])),(si,k,rust_ops[k]['op'],key,rt[key] if key!='line' else '',ns[key] if key!='line' else '')
            total+=1
        print('scenario',si,len(native),'ops matched installed PPC')
    print(total,'title operations matched')
