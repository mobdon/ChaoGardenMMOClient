"""Independent ASOE Tiny Garden save/reload check. Uses synthetic flash only."""
from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into
import sys
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_SP, UC_ARM_REG_LR, UC_ARM_REG_PC

raw=Path(sys.argv[1]).read_bytes()
assert sha256(raw).hexdigest()=="989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0"
base=0x02000000
u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
u.mem_map(base,0x40000);u.mem_write(base,raw);u.mem_map(0x03000000,0x8000)
regs=(UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3)
flash=bytearray([255])*65536
def run(offset):
    end=base+0x20000
    u.reg_write(UC_ARM_REG_SP,0x03007000);u.reg_write(UC_ARM_REG_LR,end+1)
    u.emu_start(base+offset+1,end,count=500000)
    assert u.reg_read(UC_ARM_REG_PC)==end
def hook(u,pc,size,_):
    off=pc-base
    if off==0xdeac:
        s,o,d,n=(u.reg_read(r) for r in regs)
        u.mem_write(d,bytes(flash[s*4096+o:s*4096+o+n]))
    elif off==0xe084:
        s,d,n=(u.reg_read(r) for r in regs[:3])
        assert s==10 and n==0x858
        flash[s*4096:(s+1)*4096]=bytes([255])*4096
        flash[s*4096:s*4096+n]=u.mem_read(d,n)
        u.reg_write(regs[0],0)
    elif off not in STUBS:return
    u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
# Flash setup calls appear at the beginning of the ASOE loader.
def bl(off):
    h=int.from_bytes(raw[off:off+2],'little');l=int.from_bytes(raw[off+2:off+4],'little')
    assert h>>11==30 and l>>11==31
    v=((h&2047)<<12)|((l&2047)<<1)
    return off+4+(v-(1<<23) if v&(1<<22) else v)
STUBS={0xc7c4,0xc684,0xdb28,0xcc94,0xcd10}
for off in range(0x204,0x22a,2):
    if int.from_bytes(raw[off:off+2],'little')>>11==30:STUBS.add(bl(off))
u.hook_add(UC_HOOK_CODE,hook)
r=bytearray((i*31+7)%256 for i in range(0x858))
pack_into('<HIII',r,2,0x817,17,100,800);pack_into('<I',r,0x18,0xfedcba98)
u.mem_write(0x03002fc0,bytes(r));u.reg_write(regs[0],0x03002fc2);u.reg_write(regs[1],0x856)
run(0x148c);pack_into('<H',r,0,u.reg_read(regs[0])&65535)
flash[15*4096:15*4096+len(r)]=r
u.mem_write(base+8,pack('<I',125));run(0x204);run(0x3ac)
print(sha256(flash[10*4096:10*4096+len(r)]).hexdigest())
run(0x204)
assert u.mem_read(0x030046b4,4)==pack('<I',825)
assert u.mem_read(0x03000840,4)==pack('<I',0xfedcba98)
