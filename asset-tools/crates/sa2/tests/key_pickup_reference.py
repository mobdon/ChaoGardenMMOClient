"""Private-instruction oracle for the non-null-position ChaoKeyTask pickup route."""
import hashlib,struct
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE
from unicorn import ppc_const as R

def verify(d,sec,refs,constructor):
    run=refs[1,constructor+0x42][2]; dest=refs[1,constructor+0x72][2]
    for pc,n,h in [(run,0x214,"6439a00e46181248913547ba8665387b710d4f3b98bdd8566b39d5e5f7e6b88f"),
                   (dest,0x30,"58b6c4e35b55b607d5e072af22bb10a9d2802fc61d8c9a6a56c2f48e6673861b"),
                   (constructor,0x114,"b32894402c233496eeda3e711e3070a4d3f82b570f92f15800327c1900c2f97c")]:
        assert hashlib.sha256(d[sec(1)+pc:sec(1)+pc+n]).hexdigest()==h
    for off,expected in [(0xaa,6.),(0xda,3.),(0xf2,0.9),(0x106,1.5),(0x13a,0.1),(0x1e2,0.2)]:
        _,s,a,k=refs[1,run+off];assert d[sec(s)+a:sec(s)+a+4]==struct.pack('>f',expected)
    _,s,a,k=refs[1,run+0x4e];assert struct.unpack_from('>fff',d,sec(s)+a)==(-43.,30.,-100.)

def observe(d,sec,refs,steps,world,existing=False,allocation_ok=True):
    B,D,T,W,G=0x81000000,0x82000000,0x100000,0x100100,0x801cc168
    start,end=0x24c24,0x24fc0
    u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
    mapped=set()
    def page(p):
        if p>>12 not in mapped:u.mem_map(p&~4095,4096);mapped.add(p>>12)
    def put(p,fmt,*v):u.mem_write(p,struct.pack('>'+fmt,*v))
    def get(p,fmt):return struct.unpack('>'+fmt,u.mem_read(p,struct.calcsize('>'+fmt)))
    def reg(i):return u.reg_read(getattr(R,'UC_PPC_REG_'+str(i)))
    def setreg(i,v):u.reg_write(getattr(R,'UC_PPC_REG_'+str(i)),v)
    def local(s,a):return B+a if s==1 else D+s*0x400000+a
    code=bytearray(d[sec(1)+start:sec(1)+end])
    for (t,off),(mod,s,a,k) in refs.items():
        if t!=1 or not start<=off<end:continue
        val=local(s,a) if mod else a;page(val)
        if mod and s in (4,5):u.mem_write(val,d[sec(s)+a:sec(s)+a+16])
        if k in (4,6):struct.pack_into('>H',code,off-start,(val if k==4 else (val+32768)>>16)&65535)
        else:
            assert k==10;struct.pack_into('>I',code,off-start,0x48000001|((val-(B+off))&0x3fffffc))
    for p in (T,T+0x2000,B+start,B+end):page(p)
    u.mem_write(B+start,bytes(code));u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|8192)
    owner=local(6,1800);put(T+0x34,'I',W);put(T+600,'fff',*world)
    alloc,sound,transform,vector,cleanup=[refs[1,off][2] for off in (0x24efc,0x24fa4,0x24c64,0x24cc8,0x24e1c)]
    def hook(uc,pc,size,_):
        if pc==alloc:assert not existing and (reg(3),reg(4))==(2,3);setreg(3,T if allocation_ok else 0)
        elif pc==sound:assert reg(3)==0x8006
        elif pc==transform:
            assert get(reg(4),'fff')==tuple(world);put(reg(5),'fff',*step['point'])
        elif pc==vector:
            if 'vector_after_call' in step:put(reg(3),'fff',*step['vector_after_call'])
            u.reg_write(R.UC_PPC_REG_FPR1,struct.unpack('>Q',struct.pack('>d',step['scalar']))[0])
        elif pc==cleanup:assert reg(3)==T;observed.append('cleanup')
    for pc in (alloc,sound,transform,vector,cleanup):
        page(pc);put(pc,'I',0x4e800020);u.hook_add(UC_HOOK_CODE,hook,begin=pc,end=pc)
    def invoke(pc,arg):
        setreg(3,arg);setreg(1,T+0x2ff0);u.reg_write(R.UC_PPC_REG_LR,B+end)
        u.emu_start(B+pc,B+end,count=220);assert u.reg_read(R.UC_PPC_REG_PC)==B+end
    if existing:put(owner,'I',T+888)
    invoke(0x24eac,T+600)
    if existing or not allocation_ok:
        assert get(owner,'I')==((T+888 if existing else 0),)
        assert get(W+40,'f')==(0.,)
        return []
    assert get(owner,'I')==(T,) and get(W+40,'f')==(1.,)
    trace=[];observed=[]
    for step in steps:
        if step['op']=='destroy':
            if not step['still_owner']:put(owner,'I',T+888)
            invoke(0x24e38,T)
            assert get(owner,'I')==((0 if step['still_owner'] else T+888),)
        else:invoke(0x24c24,T)
        trace.append((get(W+20,'III'),get(W+6,'H')[0],get(W+40,'I')[0],get(G+0x44,'B')[0],bool(observed)))
    return trace
