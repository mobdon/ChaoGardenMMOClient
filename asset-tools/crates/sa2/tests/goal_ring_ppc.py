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
code=bytearray(d[ss[1][0]+FIRST:ss[1][0]+0x2dd10])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not FIRST<=off<0x2dd10:continue
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
# Run the local handoff helper itself; only the imported routines return immediately.
slot_base=DATA+5*0x100000+0x24020c
map_page(slot_base)
u.mem_write(LOCAL,d[ss[1][0]+0x390dc:ss[1][0]+0x39104])
put(LOCAL+0x16,'H',(slot_base+0x8000)>>16);put(LOCAL+0x1a,'H',slot_base&0xffff)
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
    put(slot_base,'II',12345,54321)
    observed=[];put(WORK+1,'B',latched);put(WORK+8,'i',x);put(WORK+0x24,'f',7.0);put(GLOBAL+0x23,'b',byte)
    setreg(26,TASK);setreg(27,WORK)
    u.emu_start(BASE+0x2da4c,BASE+END,count=100)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+END
    print('C',x,byte,latched,hit,get(WORK+1,'B'),get(WORK+0x24,'f'),','.join(observed),get(slot_base,'I'),get(slot_base+4,'I'))

# Run the native regular motion block, then the end-of-frame cleanup dispatch.
cleanup_targets=(0x80065148,0x80071878)
for pc in cleanup_targets:
    if pc & ~0xfff != RANGE & ~0xfff:map_page(pc)
    def cleanup_hook(uc,pc,size,_):
        assert reg(3)==TASK
        observed.append(pc)
        uc.emu_stop()
    u.hook_add(UC_HOOK_CODE,cleanup_hook,begin=pc,end=pc)
const_base=DATA+5*0x100000
map_page(const_base+2219464)
u.mem_write(const_base+2219464,d[ss[5][0]+2219464:ss[5][0]+2219496])
for pc in (BASE+0x2d868,BASE+0x2da4c,BASE+0x2dd10):
    u.hook_add(UC_HOOK_CODE,lambda uc,pc,size,data:uc.emu_stop(),begin=pc,end=pc)
for active,gate,timer,ptr in itertools.product((0,1),(0,1),(0,58,59,60,65535),(0,1)):
    observed=[];setreg(26,TASK);setreg(27,WORK);setreg(28,GLOBAL+0x37)
    put(GLOBAL+0x37,'b',gate);put(WORK+1,'B',active);put(WORK+8,'i',0)
    put(WORK+6,'H',timer);put(WORK+12,'i',2147483640);put(WORK+24,'f',-11.625)
    put(TASK+0x30,'I',ptr)
    u.emu_start(BASE+0x2d674,BASE+(0x2da4c if gate else 0x2d868),count=64)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+(0x2da4c if gate else 0x2d868),(active,gate,timer,hex(u.reg_read(R.UC_PPC_REG_PC)))
    u.emu_start(BASE+0x2dcd4,BASE+0x2dd10,count=32)
    print('M',active,gate,timer,ptr,get(WORK+6,'H'),get(WORK+12,'i'),get(WORK+24,'I'),next(iter(observed),0))

# Execute the separate task only when observed at the word-2 dispatch.
begin,end=0x3ba44,0x3bd8c
map_page(BASE+begin)
u.mem_write(BASE+begin,d[ss[1][0]+begin:ss[1][0]+end])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if module!=12:continue
        address=(BASE if s==1 else DATA+s*0x100000)+val
        if section==5 and 0x2403b4<=off<0x2403b4+93*4:
            assert kind==1 and s==1
            put(const_base+off,'I',address)
        elif section==1 and begin<=off<end and kind in (4,6):
            put(BASE+off,'H',(address if kind==4 else (address+0x8000)>>16)&65535)
def query_hook(cpu,pc,*_):
    setreg(3,query);cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
def local_hook(cpu,pc,*_):
    assert reg(4)==WORK+0x3c
    codes.append(reg(3));cpu.reg_write(R.UC_PPC_REG_PC,cpu.reg_read(R.UC_PPC_REG_LR))
u.hook_add(UC_HOOK_CODE,query_hook,begin=BASE+0x3baa0,end=BASE+0x3baa0)
local_stub=u.hook_add(UC_HOOK_CODE,local_hook,begin=BASE+0x39244,end=BASE+0x39244)
for slot,query in itertools.product((0,1),[-2147483648,*range(2,97),2147483647]):
    codes=[];setreg(5,WORK);setreg(27,TASK);setreg(28,TASK+0x80)
    put(TASK+0x40,'I',WORK);put(WORK+0x54,'I',slot);put(slot_base,'II',41,42);put(slot_base+slot*4,'I',2)
    put(TASK+0x80,'B',77);put(WORK+0x34,'I',77)
    u.emu_start(BASE+begin,BASE+end,count=90)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+end and len(codes)<=1
    assert get(TASK+0x80,'B')==0 and get(WORK+0x34,'I')==120
    print('D',slot,query,next(iter(codes),-1),get(slot_base,'I'),get(slot_base+4,'I'))

# Execute the nested message selector and capture only its engine boundary.
u.hook_del(local_stub)
h=0x39244;bss=DATA+6*0x100000
map_page(bss+0xdb4)
u.mem_write(BASE+h,d[ss[1][0]+h:ss[1][0]+h+0x98])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not h<=off<h+0x98 or kind not in (4,6):continue
        addr=val+(DATA+s*0x100000 if module==12 else 0)
        put(BASE+off,'H',(addr if kind==4 else (addr+0x8000)>>16)&65535)
def message_service(cpu,pc,*_):
    normal=pc==BASE+0x392a4
    args=[reg(i) for i in range(3,9 if normal else 8)]
    if normal:args[3]=struct.unpack('>i',struct.pack('>I',args[3]))[0]
    observed.append([0x800dfe0c if normal else 0x800dfe3c,*args])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x392a4,0x392c8):
    u.hook_add(UC_HOOK_CODE,message_service,begin=BASE+off,end=BASE+off)
for code,count,gamebyte in itertools.product((5,90,109),(-1,5,90,109,110),(-128,0,127)):
    observed=[]
    put(const_base+0x240200,'i',count);put(const_base+0x240214,'II',35,67)
    put(bss+0xdb4,'I',TASK+0x400);put(TASK+0x400+code*4,'I',0x12340000+code)
    put(GLOBAL+17,'b',gamebyte);setreg(1,TASK+0xf00);setreg(3,code);setreg(4,WORK+0x3c)
    u.reg_write(R.UC_PPC_REG_LR,BASE+h+0x98)
    u.emu_start(BASE+h,BASE+h+0x98,count=60)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+h+0x98 and len(observed)==1
    print('T',code,count,gamebyte,*observed[0])

# Child selection and creation are separate from following the parent's link.
create_child=0x80070790;map_page(create_child)
def create_hook(cpu,pc,*_):
    assert reg(3)==2 and reg(5)==TASK
    created.append(reg(4)-BASE)
    put(TASK+12,'I',linked_task if linked else 0)
    setreg(3,returned_task if allocated else 0)
put(create_child,'I',0x4e800020)
u.hook_add(UC_HOOK_CODE,create_hook,begin=create_child,end=create_child)
u.mem_write(DATA+4*0x100000+3140,d[ss[4][0]+3140:ss[4][0]+3144])
returned_task,linked_task,returned_work,linked_work=[TASK+x for x in (0x200,0x300,0x400,0x500)]
u.hook_add(UC_HOOK_CODE,lambda cpu,*_:cpu.emu_stop(),begin=BASE+0x2d394,end=BASE+0x2d394)
for existing,allocated,linked in [(1,-1,1),*[(0,a,l) for a,l in itertools.product((0,1),repeat=2)]]:
    created=[];put(TASK+12,'I',linked_task if existing else 0);put(TASK+32,'I',0)
    for t,work in ((returned_task,returned_work),(linked_task,linked_work)):
        put(t+52,'I',work);u.mem_write(work,bytes([0xcd])*64)
    put(WORK+20,'fff',1.5,-20.,3.75);setreg(30,TASK);setreg(31,WORK)
    u.emu_start(BASE+0x2d318,BASE+0x2d4dc,count=64)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+0x2d394
    assert bytes(u.mem_read(linked_work+20,12))==(struct.pack('>fff',1.5,-20.,3.75) if linked else bytes([0xcd])*12)
    if allocated!=1:
        assert get(TASK+32,'I')==0 and bytes(u.mem_read(returned_work,64))==bytes([0xcd])*64
    print('K',existing,allocated,linked,next(iter(created),-1),get(TASK+32,'I')-BASE if allocated==1 else -1,get(returned_work+32,'I'),get(returned_work+1,'B'))

# Observe nested calls without inventing the individual mode handlers.
ce=0x2faac;map_page(BASE+ce)
u.mem_write(BASE+ce,d[ss[1][0]+ce:ss[1][0]+ce+0x13c])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not ce<=off<ce+0x13c:continue
        if kind in (4,6):
            assert module==12 and s==1
            val+=BASE;put(BASE+off,'H',(val if kind==4 else (val+0x8000)>>16)&65535)
run_nested=False
def child_call(cpu,pc,*_):
    assert reg(3)==TASK
    if pc==BASE+ce+0x30:
        range_checks.append(1);setreg(3,out)
    else:
        dest=REGISTER if pc==BASE+ce+0x10c else pc-BASE+(get(pc,'I')&0x03fffffc)
        handlers.append(dest)
        if run_nested and pc-BASE-ce in (0x100,0x118):return
        # A nested handler may change the mode; detect any later reset.
        put(WORK,'B',93)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x30,0xdc,0xe8,0xf4,0x100,0x10c,0x118):
    u.hook_add(UC_HOOK_CODE,child_call,begin=BASE+ce+off,end=BASE+ce+off)
for m,(flag,out) in itertools.product((-128,-1,0,1,2,3,4,5,6,7,127),((0,1),(0,0),(1,0),(-1,0))):
    handlers=[];range_checks=[]
    put(TASK+52,'I',WORK);put(TASK+20,'I',7);put(TASK+32,'I',11);put(WORK,'bb',m,flag)
    setreg(1,TASK+0xf00);setreg(3,TASK);u.reg_write(R.UC_PPC_REG_LR,BASE+ce+0x13c)
    u.emu_start(BASE+ce,BASE+ce+0x13c,count=95)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+ce+0x13c
    print('U',m,flag,out,len(range_checks),next(iter(handlers),0),get(TASK+20,'I'),get(TASK+32,'I'),get(WORK,'b'))

# Execute the mode 3 and 6 handlers in the dispatcher, observing the engine ABI.
run_nested=True
map_page(BASE+0x300b8)
for start,end in [(0x300b8,0x30114),(0x30388,0x303c0)]:
    u.mem_write(BASE+start,d[ss[1][0]+start:ss[1][0]+end])
imports={}
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not (0x300b8<=off<0x30114 or 0x30388<=off<0x303c0):continue
        if kind==10 and module==0:imports[BASE+off]=val
        elif kind in (4,6):
            assert module==12 and s==5
            val+=const_base;put(BASE+off,'H',(val if kind==4 else (val+0x8000)>>16)&65535)
def child_engine(cpu,pc,*_):
    assert reg(3)==TASK
    args=[imports[pc],"task"]
    if pc==BASE+0x300f0:args.extend([reg(4)-const_base,reg(5),reg(6)])
    services.append(args)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for pc in imports:u.hook_add(UC_HOOK_CODE,child_engine,begin=pc,end=pc)
for m,(flag,out) in itertools.product((3,6),((0,1),(0,0),(1,0))):
    services=[];handlers=[];range_checks=[]
    put(TASK+52,'I',WORK);put(TASK+60,'I',123);put(WORK,'bb',m,flag)
    setreg(1,TASK+0xf00);setreg(3,TASK);u.reg_write(R.UC_PPC_REG_LR,BASE+ce+0x13c)
    u.emu_start(BASE+ce,BASE+ce+0x13c,count=150)
    assert u.reg_read(R.UC_PPC_REG_PC)==BASE+ce+0x13c
    print('H',m,flag,out,get(WORK,'b'),get(TASK+60,'I'),json.dumps(services,separators=(',',':')))

# Run the original mode-1 handler at each distinct verified layout.
for level,entry in ((13,0x2fde8),(16,0x25c74),(18,0x2aacc)):
    image=(Path(os.environ['SA2_SET_DIR'])/f'stg{level:02}D.rel').read_bytes()
    word=lambda at:struct.unpack_from('>I',image,at)[0]
    section4=word(word(16)+32)&~1
    start=(word(word(16)+8)&~1)+entry
    block=bytearray(image[start:start+0x98])
    cpu=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
    for addr in (BASE,DATA,TASK,GLOBAL&~4095):cpu.mem_map(addr,4096)
    for module,at in struct.iter_unpack('>II',image[word(40):word(40)+word(44)]):
        section=off=0
        while True:
            delta,kind,s,val=struct.unpack_from('>HBBI',image,at);at+=8
            if kind==203:break
            if kind==202:section=s;off=0;continue
            off+=delta
            if section!=1 or not entry<=off<entry+0x98:continue
            if module:
                assert s==4 and val+4<=4096
                cpu.mem_write(DATA+val,image[section4+val:section4+val+4]);val+=DATA
            assert kind in (4,6)
            struct.pack_into('>H',block,off-entry,(val if kind==4 else (val+0x8000)>>16)&65535)
    cpu.mem_write(BASE,bytes(block));cpu.reg_write(R.UC_PPC_REG_MSR,8192)
    cpu.mem_write(TASK+52,struct.pack('>I',WORK))
    for initial36,initial40,gates in [(-6.5,0.,[0]*16),(0.,.89,[0,0]),(1.,.88999,[0,0]),(7.,2.,[0]),(3.,-2.,[0,0,-1,0]),(-2.,.99,[127])]:
        cpu.mem_write(WORK,b'\x01');cpu.mem_write(WORK+36,struct.pack('>ff',initial36,initial40))
        frames=[]
        for gate in gates:
            cpu.mem_write(GLOBAL+55,struct.pack('>b',gate))
            cpu.reg_write(R.UC_PPC_REG_3,TASK);cpu.reg_write(R.UC_PPC_REG_LR,BASE+0x98)
            cpu.emu_start(BASE,BASE+0x98,count=60)
            assert cpu.reg_read(R.UC_PPC_REG_PC)==BASE+0x98
            mode=cpu.mem_read(WORK,1)[0];a,b=struct.unpack('>II',cpu.mem_read(WORK+36,8))
            frames.append(dict(mode=mode,work36Bits=a,work40Bits=b))
            if mode==2:break
        print('A',level,initial36,initial40,json.dumps(gates,separators=(',',':')),json.dumps(frames,separators=(',',':')))

# Execute mode 2 to either the first player lookup or the native idle return.
cpu=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for addr in (BASE,DATA,TASK,GLOBAL&~4095):cpu.mem_map(addr,4096)
entry=0x2fe80;block=bytearray(d[ss[1][0]+entry:ss[1][0]+entry+0x238])
imports={}
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not entry<=off<entry+len(block):continue
        if kind==10 and module==0:imports[BASE+off-entry]=val
        if kind in (4,6) and off-entry>=0x1e0:
            if module:
                cpu.mem_write(DATA+val,d[ss[s][0]+val:ss[s][0]+val+4]);val+=DATA
            struct.pack_into('>H',block,off-entry,(val if kind==4 else (val+0x8000)>>16)&65535)
def contact_query(cpu,pc,*_):
    target=imports[pc];calls.append(target)
    arg=cpu.reg_read(R.UC_PPC_REG_3)
    if target==0x80037c74:
        assert arg==TASK+0x400
        obs['identified']=identified;cpu.reg_write(R.UC_PPC_REG_3,identified&0xffffffff)
    else:
        assert arg==TASK
        if target!=REGISTER:
            key='primary' if target==CHECK else 'secondary'
            obs[key]=(first if key=='primary' else second)
            cpu.reg_write(R.UC_PPC_REG_3,TASK+0x400 if obs[key] else 0)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x50,0x60,0x74,0x1e4):cpu.hook_add(UC_HOOK_CODE,contact_query,begin=BASE+off,end=BASE+off)
contact_stop=cpu.hook_add(UC_HOOK_CODE,lambda cpu,*_:cpu.emu_stop(),begin=BASE+0x94,end=BASE+0x94)
cpu.mem_write(BASE,bytes(block));cpu.reg_write(R.UC_PPC_REG_MSR,8192)
for flags,cached,(first,second),identified,gate in itertools.product((0,4),(-2,-1,0,1),((False,False),(False,True),(True,False)),(-1,1),(0,-1)):
    obs=dict(flags=flags,cached=cached);calls=[]
    cpu.mem_write(TASK+52,struct.pack('>II',WORK,TASK+0x200));cpu.mem_write(WORK,b'\x02')
    cpu.mem_write(WORK+4,struct.pack('>H',flags));cpu.mem_write(TASK+0x244,struct.pack('>b',cached))
    cpu.mem_write(WORK+36,struct.pack('>f',1.25));cpu.mem_write(GLOBAL+55,struct.pack('>b',gate))
    cpu.reg_write(R.UC_PPC_REG_1,TASK+0xf00);cpu.reg_write(R.UC_PPC_REG_3,TASK);cpu.reg_write(R.UC_PPC_REG_LR,BASE+0x238)
    cpu.emu_start(BASE,BASE+0x238,count=110);pc=cpu.reg_read(R.UC_PPC_REG_PC)
    result=dict(calls=calls)
    if pc==BASE+0x94:result.update(playerIndex=cpu.reg_read(R.UC_PPC_REG_30),continueAt=entry+0x94)
    else:
        assert pc==BASE+0x238
        obs['idle']=dict(work36=1.25,game_byte_55=gate)
        result.update(playerIndex=None,mode=cpu.mem_read(WORK,1)[0],work36Bits=struct.unpack('>I',cpu.mem_read(WORK+36,4))[0])
    print('P',json.dumps(obs,separators=(',',':')),json.dumps(result,separators=(',',':')))

# Run past the player lookup, observing the engine and nested stage services.
cpu.hook_del(contact_stop)
players=0x801e7328;cpu.mem_map(players&~4095,4096)
table_addr=DATA+0x2216e8;cpu.mem_map(table_addr&~4095,4096)
cpu.mem_write(table_addr,d[ss[5][0]+0x2216e8:ss[5][0]+0x2216e8+88])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if module==12 and section==5 and 0x2216e8<=off<0x2216e8+88 and kind==1:
            assert s==1
            cpu.mem_write(DATA+off,struct.pack('>I',BASE+val))
        if section!=1 or not entry+0x94<=off<entry+0x1e0 or kind not in (4,6):continue
        if module:
            if s==4:cpu.mem_write(DATA+val,d[ss[4][0]+val:ss[4][0]+val+4])
            val+=DATA
        struct.pack_into('>H',block,off-entry,(val if kind==4 else (val+0x8000)>>16)&65535)
cpu.mem_write(BASE,bytes(block));cpu.ctl_remove_cache(BASE,BASE+len(block))
def activated(cpu,pc,*_):
    off=pc-BASE
    reg=lambda i:cpu.reg_read(getattr(R,'UC_PPC_REG_'+str(i)))
    if off in (0xd4,0x174):
        ins=struct.unpack_from('>I',block,off)[0]
        addr=entry+off+((ins&0x3fffffc)^0x2000000)-0x2000000
        if off==0xd4:
            events.append(['local',addr,reg(3),reg(4)])
            cpu.mem_write(TASK+60,struct.pack('>I',after))
        else:
            vectors=[list(struct.unpack('>fff',cpu.mem_read(reg(i),12))) for i in (3,4)]
            f=struct.unpack('>d',cpu.reg_read(R.UC_PPC_REG_FPR1).to_bytes(8,'big'))[0]
            events.append(['local',addr,*vectors,f])
    elif off==0xf4:
        assert reg(3)==TASK
        callback=cpu.reg_read(R.UC_PPC_REG_LR)
        events.append(['local',callback-BASE,'task',reg(4)])
        if callback in action_entries:
            cpu.reg_write(R.UC_PPC_REG_LR,pc+4)
            cpu.reg_write(R.UC_PPC_REG_PC,callback)
            return
    else:
        args={0xfc:[reg(3)],0x18c:[reg(i) for i in range(3,7)],0x1a0:[reg(i) for i in range(3,7)]}.get(off)
        if args is None:
            assert reg(3) in (TASK,TASK+0x600)
            args=['task' if reg(3)==TASK else 'parent']
        events.append(['engine',imports[pc],*args])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0xd4,0xf4,0xfc,0x174,0x18c,0x1a0,0x1b4,0x1dc):
    cpu.hook_add(UC_HOOK_CODE,activated,begin=BASE+off,end=BASE+off)
# Enter every native action routine. Observe imported calls and the nested position helper.
action_entries=set();action_imports={}
cpu.mem_map((BASE+197784)&~4095,4096)
for index,length in enumerate((0x38,0x50,0x88,0x50,0x50,0x38,0x3c,0x44,0x38,4,0x38)):
    dest=struct.unpack('>I',cpu.mem_read(table_addr+8*index+4,4))[0]
    start=dest-BASE;code=bytearray(d[ss[1][0]+start:ss[1][0]+start+length])
    for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
        section=off=0
        while True:
            delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
            if kind==203:break
            if kind==202:section=s;off=0;continue
            off+=delta
            if section!=1 or not start<=off<start+length:continue
            if module==0 and kind==10:action_imports[BASE+off]=val
            elif kind in (4,6):
                assert module==12 and s==4
                cpu.mem_write(DATA+val,d[ss[4][0]+val:ss[4][0]+val+4]);val+=DATA
                struct.pack_into('>H',code,off-start,(val if kind==4 else (val+0x8000)>>16)&65535)
    cpu.mem_write(dest,bytes(code));action_entries.add(dest)
def action_engine(cpu,pc,*_):
    dst=action_imports[pc];args=[cpu.reg_read(R.UC_PPC_REG_3)]
    if dst==0x80063584:args.append(cpu.reg_read(R.UC_PPC_REG_4))
    elif dst==0x80035a88:
        f=struct.unpack('>d',cpu.reg_read(R.UC_PPC_REG_FPR1).to_bytes(8,'big'))[0]
        args.append(dict(f1Bits=struct.unpack('>I',struct.pack('>f',f))[0]))
    elif dst==0x80063f50:args.append(cpu.reg_read(R.UC_PPC_REG_4))
    elif dst==0x800651ac:
        assert args[0] in (TASK,TASK+0x600)
        args=["parent" if args[0]!=TASK else "task"]
    else:assert dst in (0x80063524,0x800374b0,0x80037278,0x80037394,0x80037150)
    events.append(['engine',dst,*args])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for pc in action_imports:cpu.hook_add(UC_HOOK_CODE,action_engine,begin=pc,end=pc)
# Replay the nested position-list routine with the engine allocator observed.
helper=0x30764;manager_slot=DATA+0x400000+0xd98;global_hook=0x803ad3e8
for addr in (manager_slot,global_hook):cpu.mem_map(addr&~4095,4096)
code=bytearray(d[ss[1][0]+helper:ss[1][0]+helper+0x138])
alloc_sites={}
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not helper<=off<helper+len(code):continue
        if module==0 and kind==10:alloc_sites[BASE+off]=val
        elif kind in (4,6):
            if module:
                if s==4:cpu.mem_write(DATA+val,d[ss[4][0]+val:ss[4][0]+val+4])
                val+=BASE if s==1 else DATA+(0x400000 if s==6 else 0)
            struct.pack_into('>H',code,off-helper,(val if kind==4 else (val+0x8000)>>16)&65535)
cpu.mem_write(BASE+helper,bytes(code))
read32=lambda addr:struct.unpack('>I',cpu.mem_read(addr,4))[0]
write32=lambda addr,val:cpu.mem_write(addr,struct.pack('>I',val))
manager,nodeaddr=TASK+0x800,TASK+0x900
def position_helper(cpu,pc,*_):
    assert cpu.reg_read(R.UC_PPC_REG_3)==WORK+20
    events.append(["local",pc-BASE,"work+20"])
def position_alloc(cpu,pc,*_):
    reg=lambda i:cpu.reg_read(getattr(R,'UC_PPC_REG_'+str(i)))
    addr=alloc_sites[pc]
    if addr==0x80071ba4:args=[reg(3),reg(4),reg(5)-BASE,reg(6)-DATA];ret=list_input['created']
    else:args=[reg(3),reg(4)];ret=list_input['node']
    allocations.append([addr,*args])
    cpu.reg_write(R.UC_PPC_REG_3,ret);cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
def position_return(cpu,pc,*_):
    mgr=read32(manager_slot);node=cpu.reg_read(R.UC_PPC_REG_3)
    callbacks=[read32(manager+v)-BASE for v in (20,24)] if list_input['created'] else None
    hook=read32(global_hook)-BASE if node and list_input['hook']==0 else None
    ins=None
    if node:
        x,y,z,a,b,nxt=struct.unpack('>6I',cpu.mem_read(node,24))
        ins=dict(positionBits=[x,y,z],work12Bits=a,work16Bits=b,next=nxt,newHead=read32(mgr+60))
    events.append(["positionList",dict(calls=allocations,manager=mgr,node=node,callbackWrites=callbacks,hookWrite=hook,insert=ins)])
cpu.hook_add(UC_HOOK_CODE,position_helper,begin=BASE+helper,end=BASE+helper)
for pc in alloc_sites:cpu.hook_add(UC_HOOK_CODE,position_alloc,begin=pc,end=pc)
cpu.hook_add(UC_HOOK_CODE,position_return,begin=BASE+helper+0x134,end=BASE+helper+0x134)
# Run the manager on the linked records created by the actual insertion helper.
manager_services={}
for begin,end in [(0x309d4,0x30aa4),(0x30aa4,0x30b34),(0x30b34,0x30bc8),(0x30bc8,0x30c54)]:
    code=bytearray(d[ss[1][0]+begin:ss[1][0]+end])
    for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
        section=off=0
        while True:
            delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
            if kind==203:break
            if kind==202:section=s;off=0;continue
            off+=delta
            if section!=1 or not begin<=off<end:continue
            if kind==10 and module==0:manager_services[BASE+off]=val
            elif kind in (4,6):
                if module:
                    if s==4:cpu.mem_write(DATA+val,d[ss[4][0]+val:ss[4][0]+val+4])
                    val+=BASE if s==1 else DATA+(0x400000 if s==6 else 0)
                struct.pack_into('>H',code,off-begin,(val if kind==4 else (val+0x8000)>>16)&65535)
    cpu.mem_write(BASE+begin,bytes(code))
def manager_service(cpu,pc,*_):
    arg=cpu.reg_read(R.UC_PPC_REG_3)
    if BASE+0x30aa4<=pc<BASE+0x30b34:
        target=manager_services[pc];args=[]
        if target==0x80115e24:args=[arg,list(struct.unpack(">III",cpu.mem_read(arg,12)))]
        elif target==0x801154e0:args=[arg,cpu.reg_read(R.UC_PPC_REG_4)]
        else:assert target in (0x80115d50,0x80115d9c)
        display.append(["engine",target,*args])
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
        return
    if manager_services[pc]==0x8011b314:
        assert cpu.reg_read(R.UC_PPC_REG_4)==TASK+0xc00
        point=lambda a:struct.unpack(">fff",cpu.mem_read(a,12))
        f32=lambda x:struct.unpack(">f",struct.pack(">f",x))[0]
        sq=[f32(f32(a-b)**2) for a,b in zip(point(arg),point(TASK+0xc00))]
        dist=f32(f32(f32(sq[0]+sq[1])+sq[2])**0.5)
        visits.append([arg,struct.unpack(">I",struct.pack(">f",dist))[0]])
        cpu.reg_write(R.UC_PPC_REG_FPR1,int.from_bytes(struct.pack(">d",dist),"big"))
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
        return
    if manager_services[pc]==0x801194e0:released.append([arg,read32(arg+12)])
    else:
        assert manager_services[pc]==0x80071888 and arg==manager
        removals.append(arg)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for pc in manager_services:cpu.hook_add(UC_HOOK_CODE,manager_service,begin=pc,end=pc)
def manager_draw(cpu,pc,*_):
    bits=lambda r:struct.unpack(">I",struct.pack(">f",struct.unpack(">d",cpu.reg_read(r).to_bytes(8,"big"))[0]))[0]
    ins=read32(pc)
    target=pc-BASE+((ins&0x03fffffc)^0x02000000)-0x02000000
    display.append(["local",target,bits(R.UC_PPC_REG_FPR1),bits(R.UC_PPC_REG_FPR2)])
    # Allow the actual branch instruction to enter the original nested helper.
cpu.hook_add(UC_HOOK_CODE,manager_draw,begin=BASE+0x30b04,end=BASE+0x30b04)
mh=0x18ca8;model=0x20e30c;model_mem=DATA+model
cpu.mem_map((BASE+mh)&~4095,4096);cpu.mem_map(model_mem&~4095,4096)
mc=bytearray(d[ss[1][0]+mh:ss[1][0]+mh+0xc8]);model_imports={}
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section!=1 or not mh<=off<mh+len(mc):continue
        if kind==10 and module==0:model_imports[BASE+off]=val
        elif kind in (4,6):
            assert module==12
            if s==4 or (s==5 and val in (0x20e324,0x20e328)):
                cpu.mem_write(DATA+val,d[ss[s][0]+val:ss[s][0]+val+4])
            val+=DATA;struct.pack_into('>H',mc,off-mh,(val if kind==4 else (val+0x8000)>>16)&65535)
cpu.mem_write(BASE+mh,bytes(mc));cpu.mem_write(model_mem+20,d[ss[5][0]+model+20:ss[5][0]+model+24])
def model_service(cpu,pc,*_):
    reg=lambda i:cpu.reg_read(getattr(R,'UC_PPC_REG_'+str(i)))
    bits=lambda i:struct.unpack(">I",struct.pack(">f",struct.unpack(">d",cpu.reg_read(getattr(R,"UC_PPC_REG_FPR"+str(i))).to_bytes(8,"big"))[0]))[0]
    if pc==BASE+mh+0xc4:
        display[-1][1]["field20Restored"]=read32(model_mem+20);return
    if pc==BASE+mh+0x98:
        assert reg(3)==model_mem and reg(4)==0x12345678
        ins=read32(pc);target=pc-BASE+((ins&0x03fffffc)^0x02000000)-0x02000000
        display[-1][1]["calls"].append(["local",target,model,"previousReturn"])
        return  # let the native branch enter the submission wrapper
    else:
        dst=model_imports[pc];args=[]
        if dst==0x80116ad0:
            display.append(["modelDraw",dict(model=model,field20During=read32(model_mem+20),calls=[])])
            args=[reg(3)-DATA]
        elif dst==0x80114e30:args=[reg(3),bits(1),bits(2),bits(3)]
        elif dst==0x80033248:
            args=[reg(3),reg(4),bits(1)];cpu.reg_write(R.UC_PPC_REG_3,0x12345678)
        call=["engine",dst,*args]
    display[-1][1]["calls"].append(call)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for pc in [*model_imports,BASE+mh+0x98,BASE+mh+0xc4]:cpu.hook_add(UC_HOOK_CODE,model_service,begin=pc,end=pc)
# Load the original model pointer and execute the wrapper through its real return.
sub=0x17090
cpu.mem_map((BASE+sub)&~4095,4096)
cpu.mem_write(BASE+sub,d[ss[1][0]+sub:ss[1][0]+sub+0x6c])
for module,at in struct.iter_unpack('>II',d[w(40):w(40)+w(44)]):
    section=off=0
    while True:
        delta,kind,s,val=struct.unpack_from('>HBBI',d,at);at+=8
        if kind==203:break
        if kind==202:section=s;off=0;continue
        off+=delta
        if section==5 and off==model and kind==1:
            assert module==12 and s==5
            write32(model_mem,DATA+val)
        if section==1 and off==sub+0x48 and kind==10:
            assert module==0
            submit_engine=val
def submit_service(cpu,pc,*_):
    record=display[-1][1]
    reg=lambda n:cpu.reg_read(getattr(R,"UC_PPC_REG_"+str(n)))
    if pc==BASE+sub+0x68:
        record["modelPointerRestored"]=read32(model_mem)-DATA;return
    if pc==BASE+sub+0x3c:
        assert reg(5)==0x12345678 and reg(4)==read32(model_mem)
        ins=read32(pc);target=pc-BASE+((ins&0x03fffffc)^0x02000000)-0x02000000
        call=["local",target,reg(3),reg(4)-DATA,"previousReturn",reg(6)]
    else:
        assert reg(3)==model_mem
        call=["engine",submit_engine,reg(3)-DATA,{"modelPointerDuring":read32(reg(3))}]
    record["calls"].append(call)
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for off in (0x3c,0x48,0x68):cpu.hook_add(UC_HOOK_CODE,submit_service,begin=BASE+sub+off,end=BASE+sub+off)
def manager_run(entry,arg=manager):
    cpu.reg_write(R.UC_PPC_REG_1,TASK+0xf00);cpu.reg_write(R.UC_PPC_REG_3,arg)
    cpu.reg_write(R.UC_PPC_REG_LR,BASE+0x238)
    cpu.emu_start(BASE+entry,BASE+0x238,count=1000)
    assert cpu.reg_read(R.UC_PPC_REG_PC)==BASE+0x238
for before,slot in itertools.product(range(11),(0,1)):
    for flags,(linked,byte1,parent) in itertools.product((0,0x4000),((True,0,None),(False,0,None),(False,1,False),(False,-1,True))):
        gate=slot;after=10-before;events=[];calls=[];pos=[1.5,-7.25,30.]
        list_input=None;allocations=[]
        if after==6 and flags==0:
            prev=manager if byte1 else 0
            created=(manager if slot or linked else 0) if not prev else None
            node=(nodeaddr if slot or byte1 else 0) if prev or created else None
            previous=[]
            if prev:
                for idx,(val,limit) in enumerate([(0,10),(18,20),(3,15),(100,103)]):
                    previous.append(dict(address=TASK+0xa00+idx*24,value=val,limit=limit,position=[pos[0]+12*idx,pos[1],pos[2]]))
                for idx,n in enumerate(previous):
                    cpu.mem_write(n["address"],struct.pack(">fff",*n["position"]))
                    cpu.mem_write(n["address"]+12,struct.pack(">ffI",n["value"],n["limit"],previous[idx+1]["address"] if idx+1<len(previous) else 0))
            old=(previous[0]["address"] if previous else 0) if node else None
            hook=(0x3456 if parent else 0) if node else None
            list_input=dict(manager=prev,created=created,node=node,old_head=old,hook=hook)
            write32(manager_slot,prev);write32(global_hook,hook or 0);write32(manager+60,old or 0)
            cpu.mem_write(nodeaddr,bytes([0xdd])*24)
        cpu.mem_write(TASK+52,struct.pack('>III',WORK,TASK+0x200,before))
        cpu.mem_write(TASK+8,struct.pack('>I',TASK+0x600));cpu.mem_write(TASK+48,struct.pack('>I',linked))
        cpu.mem_write(TASK+0x630,struct.pack('>I',bool(parent)));cpu.mem_write(WORK,struct.pack('>Bb2xH',2,byte1,4))
        cpu.mem_write(TASK+0x244,struct.pack('>b',slot));cpu.mem_write(players+slot*4,struct.pack('>I',TASK+0x500))
        cpu.mem_write(TASK+0x510,struct.pack('>H',flags));cpu.mem_write(WORK+20,struct.pack('>fff',*pos))
        cpu.mem_write(WORK+36,struct.pack('>f',1.25));cpu.mem_write(GLOBAL+55,struct.pack('>b',gate))
        cpu.reg_write(R.UC_PPC_REG_1,TASK+0xf00);cpu.reg_write(R.UC_PPC_REG_3,TASK);cpu.reg_write(R.UC_PPC_REG_LR,BASE+0x238)
        cpu.emu_start(BASE,BASE+0x238,count=300)
        assert cpu.reg_read(R.UC_PPC_REG_PC)==BASE+0x238 and calls==[REGISTER]
        events.append(['engine',REGISTER,'task'])
        obs=dict(flags=4,cached=slot,activation=dict(player_flags=flags,indices=None if flags else [before,after],position=pos,linked_task=linked,byte1=byte1,parent_linked=parent,tail=dict(work36=1.25,game_byte_55=gate)))
        result=dict(calls=[],playerIndex=slot,events=events,mode=cpu.mem_read(WORK,1)[0],work36Bits=struct.unpack('>I',cpu.mem_read(WORK+36,4))[0])
        if list_input and (prev or created):
            updates=53 if slot else 1
            frames=[]
            query=[pos[0]+(10 if slot else 12),pos[1],pos[2]]
            cpu.mem_write(TASK+0xc00,struct.pack(">fff",*query))
            display_word=0xfffffffb if slot else 17
            write32(GLOBAL+0x7c,display_word)
            for _ in range(updates):
                released=[];removals=[];manager_run(0x309d4)
                remaining=[];cursor=read32(manager+60)
                while cursor:
                    assert len(remaining)<8
                    remaining.append([cursor,read32(cursor+12)]);cursor=read32(cursor+20)
                frames.append(dict(released=released,remaining=remaining,removeManager=bool(removals)))
                visits=[];manager_run(0x30bc8,TASK+0xc00)
                frames[-1]["rangeQuery"]=dict(visited=visits,hit=bool(cpu.reg_read(R.UC_PPC_REG_3)))
                display=[];manager_run(0x30aa4)
                frames[-1]["display"]=display
                if removals:break
            owns=[bool(slot),bool(linked or parent)]
            write32(manager_slot,manager if owns[0] else 0x1234)
            write32(global_hook,BASE+0x30bc8 if owns[1] else 0x5678)
            released=[];manager_run(0x30b34)
            assert read32(manager+60)==0 and read32(manager_slot)==(0 if owns[0] else 0x1234) and read32(global_hook)==(0 if owns[1] else 0x5678)
            list_input['schedule']=dict(previous=previous,updates=updates,cleanup=owns,query=query,display_word=display_word)
            record=next(ev[1] for ev in events if ev[0]=="positionList")
            record["schedule"]=dict(frames=frames,cleanup=dict(released=[item[0] for item in released],clearManager=owns[0],clearHook=owns[1]))
        if list_input:obs['activation']['position_list']=list_input
        if after==2 and flags==0:
            obs["activation"]["action_links"]=dict(byte1=byte1,parent_link=parent if byte1 else None,task_link=None if byte1 and parent else linked)
        print('Y',json.dumps(obs,separators=(',',':')),json.dumps(result,separators=(',',':')))
