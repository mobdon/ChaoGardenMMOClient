"""Execute the installed goal-ring branches off-line. Imported routines are observed, not executed."""
import hashlib,itertools,json,os,struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R
BASE, DATA, TASK,WORK = 0x81000000,0x82000000,0x100000,0x100100
FIRST,END=0x2d208,0x2db48
GLOBAL=0x801cbd88
CHECK,IDENT,REGISTER,MAIN1,MAIN2,SOUND,LOCAL=(0x800066f4,0x80037c74,0x8000f4ec,0x8001c78c,0x8001baa4,0x8006ad94,BASE+0x390dc)
RANGE,ALLOC=0x8006538c,0x801194b4
d=(Path(os.environ['SA2_SET_DIR'])/'stg13D.rel').read_bytes()
assert hashlib.sha256(d).hexdigest()=='bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b'
def w(i):return struct.unpack_from('>I',d,i)[0]
ss=[(w(w(16)+i*8)&~1,w(w(16)+i*8+4)) for i in range(w(12))]
code=bytearray(d[ss[1][0]+FIRST:ss[1][0]+END])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not FIRST<=off<END:continue
        if module==12:val+=(BASE if s==1 else DATA+s*0x100000)
        else:assert module==0
        loc=off-FIRST
        if kind in (4,6):struct.pack_into('>H',code,loc,(val if kind==4 else (val+0x8000)>>16)&0xffff)
        elif kind==10:
            orig=struct.unpack_from('>I',code,loc)[0];dist=val-(BASE+off)
            assert -(1<<25)<=dist<(1<<25)
            struct.pack_into('>I',code,loc,(orig&0xfc000003)|(dist&0x3fffffc))
        else:raise RuntimeError(kind)
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
def map_page(p):u.mem_map(p&~0xfff,0x1000)
for p in {i&~0xfff for i in (BASE+FIRST,BASE+END,DATA+4*0x100000+3136,TASK,GLOBAL,0x801e7368,CHECK,IDENT,REGISTER,MAIN1,MAIN2,SOUND,LOCAL,RANGE,ALLOC)}:map_page(p)
u.mem_write(BASE+FIRST,bytes(code));u.mem_write(DATA+4*0x100000+3136,d[ss[4][0]+3136:ss[4][0]+3140])
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|0x2000)
def put(p,fmt,*x):u.mem_write(p,struct.pack('>'+fmt,*x))
def get(p,fmt):return struct.unpack('>'+fmt,u.mem_read(p,struct.calcsize('>'+fmt)))[0]
def reg(i):return u.reg_read(getattr(R,'UC_PPC_REG_'+str(i)))
def setreg(i,v):u.reg_write(getattr(R,'UC_PPC_REG_'+str(i)),v&0xffffffff)
def hook(uc,pc,size,_):
    if mode=='init' and pc in stops:
        observed.append(stops[pc]);uc.emu_stop();return
    if pc==RANGE:setreg(3,out)
    elif pc==ALLOC:setreg(3,TASK+512 if alloc else 0)
    elif pc==CHECK:assert reg(3)==TASK;observed.append('check');setreg(3,0 if hit==-2 else TASK+512)
    elif pc==IDENT:assert reg(3)==TASK+512;observed.append('identify');setreg(3,hit)
    elif pc==REGISTER:assert reg(3)==TASK;observed.append('register')
    elif pc in (MAIN1,MAIN2,LOCAL):
        assert reg(3)==hit
        if pc==LOCAL:assert reg(4)==2
        observed.append({MAIN1:'main1',MAIN2:'main2',LOCAL:'local'}[pc])
    elif pc==SOUND:
        assert [reg(i) for i in range(4,9)]==[TASK,1,30,180,WORK+0x14]
        observed.append(str(reg(3)))
stops={BASE+0x2d318:'child',BASE+0x2d3d4:'contact',BASE+0x2d4dc:'return'}
for pc in (RANGE,ALLOC,CHECK,IDENT,REGISTER,MAIN1,MAIN2,SOUND,LOCAL):
    put(pc,'I',0x4e800020);u.hook_add(UC_HOOK_CODE,hook,begin=pc,end=pc)
for pc in stops:u.hook_add(UC_HOOK_CODE,hook,begin=pc,end=pc)
mode='init'
for x,ch,byte,timer,out,alloc in itertools.product((-1,0,1,2,4),(0,4,5,6),(0,1,2),(-1,119,120),(0,1),(0,1)):
    observed=[];put(WORK,'BB',0,0);put(WORK+8,'i',x);put(GLOBAL+0x23,'b',byte)
    put(0x801e7368,'I',TASK+768 if timer>=0 else 0);put(TASK+774,'H',max(timer,0))
    setreg(3,ch);setreg(30,TASK);setreg(31,WORK)
    u.emu_start(BASE+0x2d24c,BASE+END,count=110)
    assert len(observed)==1
    print('I',x,ch,byte,timer,out,alloc,observed[0],get(WORK+8,'i'))
mode='contact'
for x,byte,latched,hit in itertools.product((-3,-2,-1,0,1,2,3),(-1,0,1,2,3),(0,1),(-2,-1,0,1)):
    observed=[];put(WORK+1,'B',latched);put(WORK+8,'i',x);put(WORK+0x24,'f',7.0);put(GLOBAL+0x23,'b',byte)
    setreg(26,TASK);setreg(27,WORK)
    u.emu_start(BASE+0x2da4c,BASE+END,count=100)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+END
    print('C',x,byte,latched,hit,get(WORK+1,'B'),get(WORK+0x24,'f'),','.join(observed))
