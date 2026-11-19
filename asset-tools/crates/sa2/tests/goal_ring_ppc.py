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
