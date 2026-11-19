"""Run actual Tiny Memory callback from initialization through return to Garden.
Only audio/BIOS display services are stubbed. Private board and traces stay under .local.
Usage: python native_memory_round_oracle.py MULTIBOOT LOCAL_TRACE
"""
from pathlib import Path
import struct,sys,hashlib
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_INTR,UC_HOOK_CODE
from unicorn.arm_const import *
b=Path(sys.argv[1]).read_bytes();out=Path(sys.argv[2]);assert '.local' in out.resolve().parts
assert hashlib.sha256(b).hexdigest()=='84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca'
def word(k):return struct.unpack_from('<I',b,k)[0]
inp=word(0xb7c4)
def run_game(seed,losses,fail):
 u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
 for a,n in [(0x02000000,0x40000),(0x03000000,0x8000),(0x04000000,0x1000),(0x05000000,0x1000),(0x06000000,0x20000),(0x07000000,0x1000)]:u.mem_map(a,n)
 u.mem_write(0x02000000,b)
 def bios(u,irq,_):
    ptr=u.reg_read(UC_ARM_REG_PC);opcode=u.mem_read(ptr-2,1)[0]
    if opcode==6:
        x,y=(u.reg_read(reg) for reg in (UC_ARM_REG_R0,UC_ARM_REG_R1));assert y and x<0x80000000 and y<0x80000000
        u.reg_write(UC_ARM_REG_R0,x//y);u.reg_write(UC_ARM_REG_R1,x%y);u.reg_write(UC_ARM_REG_R3,x//y)
    else:assert opcode in [0xb,0xc,0x11,0x12],hex(opcode)
 u.hook_add(UC_HOOK_INTR,bios)
 def noop(u,a,n,_):u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
 # m4aSongNumStart and m4aMPlayFadeOut. Verify BL target from pinned bytes.
 from capstone import Cs,CS_ARCH_ARM,CS_MODE_THUMB
 dis=Cs(CS_ARCH_ARM,CS_MODE_THUMB)
 def bl_at(a):return int(next(iter(dis.disasm(b[a:a+4],0x02000000+a))).op_str[1:],16)
 for pc in [bl_at(0xb800),bl_at(0xa0ec)]:
    u.hook_add(UC_HOOK_CODE,noop,begin=pc,end=pc)
 def load(a,n=1):return int.from_bytes(u.mem_read(a,n),'little')
 def sub(addr):
    u.reg_write(UC_ARM_REG_SP,0x03007e00);u.reg_write(UC_ARM_REG_LR,0x0203f001);u.emu_start(0x02000001+addr,0x0203f000,count=300000)
    assert u.reg_read(UC_ARM_REG_PC)==0x0203f000,hex(u.reg_read(UC_ARM_REG_PC))
 u.mem_write(0x030013a8,struct.pack('<I',seed))
 w=lambda n:load(0x03000000+n,4)
 v=lambda n:load(0x03000000+n)
 records=bytearray();events=[];start=None;previous=0;pre_frames=0
 for frame in range(16000):
    phase=w(0x19d8);substate=w(0x19dc);k=0
    if phase==4 and not records: assert w(0x19e4)==48,(frame,w(0x19e4))
    if phase==6:
        if start is None:start=bytes(u.mem_read(0x03001990,30));pre_frames=frame
        if substate in (0,1):
            layout=bytes(u.mem_read(0x03001990,30));arr=[(n,i) for i,n in enumerate(layout) if n<14]
            if substate==0:n,i=min(arr)
            else:
                first=v(0x19e8);mistake=v(0x19b6)<losses and (not fail or v(0x19f8)>0)
                n,i=next((n,i)for n,i in sorted(arr) if (n//2!=first//2 if mistake else n==(first^1)))
            row,col=divmod(i,6);x,y=v(0x19ae),v(0x19af)
            if x!=col:k=16 if x<col else 32
            elif y!=row:k=128 if y<row else 64
            else:k=1
    # No two identical held edges in a row. Do not touch private board/state.
    if k&previous:k=0
    u.mem_write(inp,struct.pack('<HHH',k,0,k&~previous));previous=k
    sub(0xa8a4)
    if phase in (4,5,6,7,8,9):
        nxt=w(0x19d8); board=bytes(u.mem_read(0x03001990,30)); mask=sum((bit<14)<<i for i,bit in enumerate(board))
        records.extend(struct.pack("<BBBBBBBBII",k,nxt,w(0x19dc),v(0x19ae),v(0x19af),v(0x19b6),v(0x19f8),v(0x1970),mask,w(0x13a8)))
    if w(0x3330)==0x02001bf9:
        assert start is not None and v(0x19b6)==losses
        return struct.pack("<II",seed,len(records)//16)+records+struct.pack("<III",pre_frames,frame+1,w(0x5274))
 raise ValueError((frame,phase,substate,w(0x19e0),w(0x19e4)))
if __name__=='__main__':
 cases=[(0,0,False),(7,1,False),(19,2,False),(91,3,True)]
 data=b''.join(run_game(*case) for case in cases)
 out.parent.mkdir(exist_ok=True,parents=True);out.write_bytes(data);print('trace',len(data),hashlib.sha256(data).hexdigest())
