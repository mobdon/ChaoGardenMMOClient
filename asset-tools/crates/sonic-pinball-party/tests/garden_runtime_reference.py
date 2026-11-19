"""Joined native A3VE home/care and Chao Ball frames, without inner stubs.

Original task allocation, callbacks, rules, sprite composition, display queues
and song routing execute as native instructions. BIOS copy/divide are bounded
primitive seams. Inputs and initial RAM are synthetic; audio has not been booted.
This callback test is not a full Garden boot, physical device or service run.
Only hashes, synthetic controls and source metadata are retained.
"""
import argparse,json,os,struct,math
from pathlib import Path
import reference_parity as p
BASE=0x02000000
FIXTURE=Path(__file__).with_name('garden-runtime-vectors.json')
REGIONS=[(0x02000000,0x40000),(0x03000000,0x7000),(0x04000000,0x400),(0x05000000,0x400),(0x06000000,0x18000),(0x07000000,0x400)]
def generate(path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_INTR
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    rom=p.bounded_read(Path(path),p.ROM_LENGTH);p.validate_rom(rom)
    payload,used=p.decode(rom[p.GARDEN_OFFSET:],p.GARDEN_LENGTH)
    assert used==p.GARDEN_STORED and p.sha256(payload)==p.GARDEN_SHA256
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x40000),(0x03000000,0x8000),(0x04000000,0x1000),(0x05000000,0x1000),(0x06000000,0x18000),(0x07000000,0x1000)]:cpu.mem_map(a,n)
    def bios(c,irq,user):
        assert irq==2
        pc=c.reg_read(UC_ARM_REG_PC)-BASE
        src,dst,ctl=[c.reg_read(r)for r in (UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2)]
        if pc in (0x11ae2,0x11ae6):
            width=4 if pc==0x11ae2 or ctl&0x04000000 else 2;count=ctl&0x1fffff
            assert count<=0x1000
            if ctl&0x01000000:c.mem_write(dst,bytes(c.mem_read(src,width))*count)
            else:
                for i in range(count):c.mem_write(dst+i*width,bytes(c.mem_read(src+i*width,width)))
        elif pc in (0x11af6,0x11afa):
            decoded,_=p.decode(payload[src-BASE:]);c.mem_write(dst,bytes(decoded))
        elif pc==0x11b02:c.reg_write(UC_ARM_REG_R0,math.isqrt(src))
        elif pc==0x11ade:
            import math_reference
            c.reg_write(UC_ARM_REG_R0,math_reference.atan2_reference(src,dst))
        else:
            assert pc in (0x11aea,0x11aee) and dst!=0
            a=src if src<0x80000000 else src-0x100000000;b=dst if dst<0x80000000 else dst-0x100000000
            q=abs(a)//abs(b);q=-q if (a<0)!=(b<0) else q
            c.reg_write(UC_ARM_REG_R0,q&0xffffffff);c.reg_write(UC_ARM_REG_R1,(a-q*b)&0xffffffff);c.reg_write(UC_ARM_REG_R3,abs(q))
    cpu.hook_add(UC_HOOK_INTR,bios)
    def call(off):
        cpu.reg_write(UC_ARM_REG_SP,0x03007e00);cpu.reg_write(UC_ARM_REG_LR,0x03007f01)
        cpu.emu_start(BASE+off+1,0x03007f00,count=2000000)
        assert cpu.reg_read(UC_ARM_REG_PC)==0x03007f00,hex(cpu.reg_read(UC_ARM_REG_PC))
    def digest():return p.sha256(b''.join(bytes(cpu.mem_read(a,n))for a,n in REGIONS))
    cases=[]
    for round in range(4):
        for profile in range(3):
            seed=(0x1234567+round*0x12345+profile*0x76543)&0xffffffff
            for a,n in REGIONS:cpu.mem_write(a,bytes(n))
            cpu.mem_write(BASE,bytes(payload));cpu.mem_write(0x03007000,bytes(0x1000))
            cpu.mem_write(0x03001ad1,bytes([round]));cpu.mem_write(0x030013a8,struct.pack('<I',seed))
            call(0xd504);setup=digest();frames=[]
            for tick in range(180):
                held=0 if profile==0 else [0,16,0,32][(tick//16)%4]
                pressed=0 if profile!=2 or tick%37!=24 else 1
                cpu.mem_write(0x03005310,struct.pack('<HHH',held,0xffff^held,pressed))
                call(0xd7cc)
                frames.append(dict(held=held,pressed=pressed,sha256=digest()))
            cases.append(dict(round=round,profile=profile,seed=seed,afterSetup=setup,frames=frames))
    home_cases=[]
    for language in range(5):
        for present in (False,True):
            for a,n in REGIONS:cpu.mem_write(a,bytes(n))
            cpu.mem_write(BASE,bytes(payload));cpu.mem_write(0x03007000,bytes(0x1000));call(0x43c)
            cpu.mem_write(0x03004260,bytes([language]))
            if present:cpu.mem_write(0x03004af8,b'\x00')
            cpu.mem_write(0x03004abf,bytes([88,104]));cpu.mem_write(0x03006194,struct.pack('<I',1000))
            call(0x18c8);setup=digest();frames=[]
            for tick in range(60):
                call(0x1d68);frames.append(digest())
            home_cases.append(dict(language=language,present=present,afterSetup=setup,frames=frames))
    return dict(schemaVersion=1,romSha256=p.ROM_SHA256,payloadSha256=p.GARDEN_SHA256,scope=__doc__.strip(),
        routines=[dict(name=name,space='garden',offset=a,length=b-a,sha256=p.sha256(payload[a:b]))for name,a,b in [('joinedBoardFrame',0xd7cc,0xd8a4),('joinedHomeFrame',0x1d68,0x1e00)]],cases=cases,homeCases=home_cases)
if __name__=='__main__':
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('--rom',default=os.environ.get('SPP_ROM'));a.add_argument('--write',type=Path);args=a.parse_args()
    if not args.rom:a.error('provide local --rom')
    v=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(v,separators=(',',':'))+'\n',encoding='utf8')
    elif v!=json.loads(FIXTURE.read_text(encoding='utf8')):raise ValueError('Joined Garden runtime differs')
    print(json.dumps(dict(boardCases=len(v['cases']),boardFrames=sum(len(c['frames'])for c in v['cases']),homeCases=len(v['homeCases']),homeFrames=sum(len(c['frames'])for c in v['homeCases']),gardenEndToEndValidated=False)))
