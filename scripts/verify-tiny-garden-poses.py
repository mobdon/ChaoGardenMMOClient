"""Exercise the original A2NE body callback across all 63 activity dispatch entries.
Inputs cover each facing sector, alternate/phase counters and clock bits. This is
callback coverage, not a claim that every combination occurs during normal AI."""
import hashlib,json,struct,sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
base=0x02000000
payload=Path(sys.argv[1]).read_bytes()
if hashlib.sha256(payload).hexdigest()!='84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca':raise ValueError('wrong game revision')
u=Uc(UC_ARCH_ARM,UC_MODE_THUMB);u.mem_map(base,0x40000);u.mem_map(0x03000000,0x10000);u.mem_write(base,payload)
# Sound and renderer are sinks; the body task's selected frame is written before submission.
for offset in (0x104e0,0x1398):u.mem_write(base+offset,b'\x70\x47')
u.mem_write(0x0300152e,b'\x01')
u.mem_write(0x0300150c,struct.pack('<I',0x03004400))
rows=[];cases=0
for state in range(1,64):
    found=set()
    for t in range(256):
        for sector in (0,3,6,9):
            parent=bytearray(36);actor=bytearray(36)
            parent[7]=(t&1)*128;parent[0x16:0x1b]=bytes([sector,state,0,t,(t*7)&255])
            parent[0x23]=0x80 if t&1 else 0
            actor[0x22]=t%19;actor[0x23]=t%16
            u.mem_write(0x03004400,bytes(parent));u.mem_write(0x03004424,bytes(actor))
            u.mem_write(0x03003338,struct.pack('<I',t))
            u.reg_write(UC_ARM_REG_R0,0x03004424);u.reg_write(UC_ARM_REG_SP,0x0300f000);u.reg_write(UC_ARM_REG_LR,0x03008001)
            u.emu_start(0x02006a55,0x03008000,count=650)
            if u.reg_read(UC_ARM_REG_PC)!=0x03008000:raise ValueError('callback did not return')
            data=u.mem_read(0x03004424,36);pose=data[0x19];tile=struct.unpack_from('<H',data,0x14)[0]
            if pose>=67 or tile!=0x1a4+9*pose:raise ValueError('pose tile mismatch')
            found.add(pose);cases+=1
    rows.append(dict(activity=state,handler=hex(struct.unpack_from('<I',payload,0x6ad4+(state-1)*4)[0]),bodyPoseIds=sorted(found)))
dest=Path(sys.argv[2])
dest.write_text(json.dumps(dict(sourceSha256=hashlib.sha256(payload).hexdigest(),cases=cases,callback='02006a54',activities=rows),indent=2)+'\n')

