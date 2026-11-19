"""Execute the local verified A2NE embedded Tiny Garden Thumb; no assets/board dumps retained.
Usage: python native_garden_memory_oracle.py PATH_TO_mb-chao-garden-A2NE.gba
Requires Unicorn (the shared workspace has an ignored installation).
"""
from pathlib import Path
from hashlib import sha256
import struct,sys
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_INTR
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R3,UC_ARM_REG_LR,UC_ARM_REG_PC,UC_ARM_REG_SP
b=Path(sys.argv[1]).read_bytes()
assert sha256(b).hexdigest()=='84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca'
u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
for a,n in [(0x02000000,0x40000),(0x03000000,0x8000),(0x07000000,0x1000)]:u.mem_map(a,n)
u.mem_write(0x02000000,b)
def regsdiv(u,interrupt,_):
    assert interrupt==2 and u.reg_read(UC_ARM_REG_PC)==0x02011962
    x,y=(u.reg_read(reg) for reg in (UC_ARM_REG_R0,UC_ARM_REG_R1))
    assert y and x<0x80000000 and y<0x80000000
    u.reg_write(UC_ARM_REG_R0,x//y);u.reg_write(UC_ARM_REG_R1,x%y);u.reg_write(UC_ARM_REG_R3,x//y)
u.hook_add(UC_HOOK_INTR,regsdiv)
def run(addr,r0=0):
    u.reg_write(UC_ARM_REG_SP,0x03007e00);u.reg_write(UC_ARM_REG_LR,0x0203f001);u.reg_write(UC_ARM_REG_R0,r0)
    u.emu_start(0x02000001+addr,0x0203f000,count=100000)
    assert u.reg_read(UC_ARM_REG_PC)==0x0203f000
    return u.reg_read(UC_ARM_REG_R0)
def fingerprint():
    rng=sha256();deal=sha256();seed=0
    for i in range(4096):
        seed=(i*0x9e3779b9)&0xffffffff;run(0x1528,seed)
        for n in (1,2,5,6,7,255,256,32768,65535):
            a=run(0x1534);z=run(0x1554,n);rng.update(struct.pack('<HHI',a,z,run(0x151c)))
        run(0x1528,seed);u.mem_write(0x03001990,bytes([0x56])*30);u.mem_write(0x07000000,bytes(1024))
        run(0xa554)
        deal.update(u.mem_read(0x03001990,30));deal.update(struct.pack('<I',run(0x151c)))
        # Coordinates are the original OAM writes, not inferred from the board.
        for k in range(14):
            y,x,t=struct.unpack('<HHH',u.mem_read(0x07000000+(k+11)*8,6))
            deal.update(struct.pack('<BBH',y&255,x&255,t))
    return rng.hexdigest(),deal.hexdigest()
print(fingerprint())
for a,z in [(0x151c,0x1588),(0xa45c,0xa554),(0xa554,0xa638)]: print(hex(a),hex(z),sha256(b[a:z]).hexdigest())
