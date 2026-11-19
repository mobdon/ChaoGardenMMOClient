"""Check bonus selection after contact in the installed City Escape checkpoint."""
import hashlib, os, struct
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

d = (Path(os.environ["SA2_GAME_PATH"]) / "resource/gd_PC/stg13D.rel").read_bytes()
assert hashlib.sha256(d).hexdigest() == "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b"
w = lambda i: struct.unpack_from(">I", d, i)[0]
text = w(w(16)+8) & ~1
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
base = 0x81000000
u.mem_map(base, 0x50000)
u.mem_write(base, d[text:text+0x50000])
# Substitute separate data bases for the three external globals. The
# instructions following each relocated address pair run without alteration.
mem = 0x82000000
u.mem_map(mem, 4096)
def address_pair(off, address):
    u.mem_write(base+off+2, struct.pack(">H", (address+32768)>>16))
    u.mem_write(base+off+6, struct.pack(">H", address & 65535))
address_pair(0x44820, mem)
for off in (0x44898,0x448dc,0x44920): address_pair(off,mem+256)
for off in (0x448ac,0x448f0,0x44934): address_pair(off,mem+512)
u.mem_write(mem,struct.pack(">I",mem+768))
u.mem_write(mem+512,struct.pack(">I",mem+1024))
def item_hook(uc,addr,size,items):
    items.append(uc.reg_read(R.UC_PPC_REG_4))
    uc.reg_write(R.UC_PPC_REG_PC,uc.reg_read(R.UC_PPC_REG_LR))
items=[]
u.hook_add(UC_HOOK_CODE,item_hook,items,begin=base+0x3069c,end=base+0x3069c)
for value,flags,item,inc in [(-32768,0,None,0),(19,0,None,0),(20,0,2,5),(39,3,2,5),(40,0,0,10),(59,0,0,10),(60,0,1,20),(79,0,1,20),(80,0,8,0),(89,3,8,0),(90,0,3,0),(90,1,6,0),(90,2,6,0),(32767,4,3,0)]:
    items.clear()
    u.mem_write(mem+768+16,struct.pack(">H",flags))
    for p in (mem+256+0x6e,mem+1024+16): u.mem_write(p,b"\0\0")
    u.reg_write(R.UC_PPC_REG_3,value & 65535)
    u.reg_write(R.UC_PPC_REG_29,0)
    u.reg_write(R.UC_PPC_REG_30,0)
    u.emu_start(base+0x44814,base+0x4494c,count=100)
    assert u.reg_read(R.UC_PPC_REG_PC)==base+0x4494c
    assert items==([] if item is None else [item])
    for p in (mem+256+0x6e,mem+1024+16): assert struct.unpack(">h",u.mem_read(p,2))[0]==inc
print("14 installed checkpoint bonus cases passed")
