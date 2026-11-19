"""Native A3VE Garden boot/home/graphics installation with authentic local art.

Native home setup, task allocation, input, register queues, IRQ data copies and
map helper execute. Flash loader, JoyBus initialize and audio initialization/
song start are explicit intercepted boundaries. BIOS copies/decode are bounded
independent primitives. Only state/source hashes and synthetic inputs are saved.
This does not execute the newly allocated tasks or prove boot/device acceptance.
"""
import argparse,json,os,struct
from pathlib import Path
import reference_parity as p
FIXTURE=Path(__file__).with_name('garden-home-vectors.json')
BASE=0x02000000
ARITY={0x19c:0,0x248:0,0x480:0,0xc6c:0,0x18a0:0,0x105e8:0,0x228:1,0xbe24:0,0xd27c:0,
       0x17a4:0,0xc90:0,0xd04:3,0x167c:2,0x1066c:1}
STUB={0x248,0x480,0x105e8,0x1066c}
def pattern(seed,n):return bytes((seed+i*37)&255 for i in range(n))
def generate(path):
    from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    rom=p.bounded_read(Path(path),p.ROM_LENGTH);p.validate_rom(rom);payload,used=p.decode(rom[p.GARDEN_OFFSET:],p.GARDEN_LENGTH)
    assert used==p.GARDEN_STORED and p.sha256(payload)==p.GARDEN_SHA256
    cpu=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x40000),(0x03000000,0x8000),(0x04000000,0x10000),(0x05000000,0x1000),(0x06000000,0x20000),(0x08000000,0x800000)]:cpu.mem_map(a,n)
    cpu.mem_write(0x08000000,rom);calls=[];root_entry=None;root_pending=False
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    def boundary(c,a,size,user):
        nonlocal root_pending
        if root_pending and a==BASE+root_entry:
            root_pending=False
            return
        off=a-BASE;args=[c.reg_read(r)for r in regs[:ARITY[off]]];calls.append([off,args])
        if off in STUB:c.reg_write(UC_ARM_REG_R0,0);c.reg_write(UC_ARM_REG_PC,c.reg_read(UC_ARM_REG_LR))
    for off in ARITY:cpu.hook_add(UC_HOOK_CODE,boundary,begin=BASE+off,end=BASE+off)
    def bios(c,irq,user):
        assert irq==2
        src,dst,ctl=[c.reg_read(r)for r in regs[:3]];pc=c.reg_read(UC_ARM_REG_PC)
        if pc in (BASE+0x11af6,BASE+0x11afa):
            off=src-BASE;decoded,_=p.decode(payload[off:]);c.mem_write(dst,bytes(decoded));return
        assert pc in (BASE+0x11ae2,BASE+0x11ae6)
        width=4 if pc==BASE+0x11ae2 or ctl&0x04000000 else 2;count=ctl&0x1fffff
        assert count<=0x800
        if ctl&0x01000000:c.mem_write(dst,bytes(c.mem_read(src,width))*count)
        else:
            for i in range(count):c.mem_write(dst+i*width,bytes(c.mem_read(src+i*width,width)))
    cpu.hook_add(UC_HOOK_INTR,bios)
    cases=[]
    for i in range(48):
        seed=(i*59+17)&255;entry=[0xc4,0x18c8,0x1c60,0x19c][i%4];flag=i%5;language=[0,1,2,3,4,5,0xffffffff][i%7];rng=(0x12345678+i*0x01020304)&0xffffffff
        ew=bytearray(0x40000);ew[:len(payload)]=payload;struct.pack_into('<I',ew,12,language);struct.pack_into('<I',ew,16,rng)
        cpu.mem_write(BASE,bytes(ew));cpu.mem_write(0x03000000,pattern(seed,0x8000));cpu.mem_write(0x04000000,pattern(seed+3,0x10000));cpu.mem_write(0x05000000,pattern(seed+7,0x1000));cpu.mem_write(0x06000000,pattern(seed+19,0x20000))
        patches=[[0x03004260,bytes([flag]).hex()]]
        if i%3==0:patches.append([0x03004ad1,bytes(38).hex()])
        if i%5==0:patches.append([0x03004af8,(b'\xff'*10).hex()])
        for a,h in patches:cpu.mem_write(a,bytes.fromhex(h))
        calls.clear();root_entry=entry;root_pending=entry in ARITY;cpu.reg_write(UC_ARM_REG_SP,0x03007e00);cpu.reg_write(UC_ARM_REG_LR,0x03007f01)
        cpu.emu_start(BASE+entry+1,0x03007f00,count=2000000);assert cpu.reg_read(UC_ARM_REG_PC)==0x03007f00
        cases.append(dict(entry=entry,seed=seed,language=language,rng=rng,patches=patches,calls=list(calls),iwramSha256=p.sha256(cpu.mem_read(0x03000000,0x7000)),ewramSha256=p.sha256(cpu.mem_read(BASE,0x40000)),mmioSha256=p.sha256(cpu.mem_read(0x04000000,0x10000)),paletteSha256=p.sha256(cpu.mem_read(0x05000000,0x1000)),vramSha256=p.sha256(cpu.mem_read(0x06000000,0x20000))))
    spans=[('boot',0xc4,0x168),('interruptInstall',0x19c,0x248),('audioHooks',0x18a0,0x18c8),('home',0x18c8,0x1b58),('graphics',0x1c60,0x1d68),('otherModeReset',0xbe24,0xbe3c),('boardReset',0xd27c,0xd294)]
    return dict(schemaVersion=1,romSha256=p.ROM_SHA256,payloadSha256=p.GARDEN_SHA256,scope=__doc__.strip(),routines=[dict(name=n,space='garden',offset=a,length=b-a,sha256=p.sha256(payload[a:b]))for n,a,b in spans],cases=cases)
if __name__=='__main__':
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('--rom',default=os.environ.get('SPP_ROM'));a.add_argument('--write',type=Path);args=a.parse_args()
    if not args.rom:a.error('provide local --rom')
    v=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(v,separators=(',',':'))+'\n',encoding='utf8')
    elif v!=json.loads(FIXTURE.read_text(encoding='utf8')):raise ValueError('Home oracle differs')
    print(json.dumps({'cases':len(v['cases']),'gardenEndToEndValidated':False}))
