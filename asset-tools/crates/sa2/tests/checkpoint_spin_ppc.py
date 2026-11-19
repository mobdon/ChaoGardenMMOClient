"""Execute the installed City Escape spin helper and print comparison vectors."""
from pathlib import Path
import os, hashlib, struct
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN
from unicorn import ppc_const as R

d=(Path(os.environ["SA2_GAME_PATH"])/"resource/gd_PC/stg13D.rel").read_bytes()
assert hashlib.sha256(d).hexdigest()=="bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b"
w=lambda i:struct.unpack_from(">I",d,i)[0]
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
base={1:0x81000000,4:0x82000000,6:0x83000000}
for sec,addr in base.items():
    row=w(16)+8*sec; size=w(row+4)
    u.mem_map(addr,(size+4095)&~4095)
    if sec!=6:u.mem_write(addr,d[w(row)&~1:(w(row)&~1)+size])
for mod,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
    off=0
    while True:
        inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
        if k==203:break
        if k==202:target=s;off=0;continue
        off+=inc
        if target==1 and 0x44980<=off<0x44b78 and mod==12 and k in (4,6):
            val=base[s]+a
            u.mem_write(base[1]+off,struct.pack(">H",(val if k==4 else (val+32768)>>16)&65535))
mem=0x84000000;u.mem_map(mem,4096)
u.mem_write(base[6]+0xdd0,struct.pack(">I",mem))
u.reg_write(R.UC_PPC_REG_MSR,8192)
for a,b,c,e in [(0,0,0,0),(32760,-32760,120,-180),(-16000,18000,-3000,8000),(8200,600,-1,-1)]:
    for off,v in [(4,a),(8,b),(16,c),(20,e)]:u.mem_write(mem+off,struct.pack(">i",v))
    u.mem_write(mem+0x30,struct.pack(">hh",-1,3));u.mem_write(mem+256,b"\x02")
    u.reg_write(R.UC_PPC_REG_1,mem+3000);u.reg_write(R.UC_PPC_REG_4,mem+256)
    u.emu_start(base[1]+0x44980,base[1]+0x44b78,count=250)
    assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0x44b78
    print([struct.unpack(">i",u.mem_read(mem+off,4))[0] for off in (4,8,16,20)],struct.unpack(">hh",u.mem_read(mem+0x30,4)),u.mem_read(mem+256,1)[0])
